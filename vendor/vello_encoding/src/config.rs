// Copyright 2023 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::SegmentCount;

use super::{
    BinHeader, Clip, ClipBbox, ClipBic, ClipElement, DrawBbox, DrawMonoid, Layout, LineSoup, Path,
    PathBbox, PathMonoid, PathSegment, Tile,
};
use bytemuck::{Pod, Zeroable};

const TILE_WIDTH: u32 = 16;
const TILE_HEIGHT: u32 = 16;

// TODO: Obtain these from the vello_shaders crate
pub(crate) const PATH_REDUCE_WG: u32 = 256;
const PATH_BBOX_WG: u32 = 256;
const FLATTEN_WG: u32 = 256;
const CLIP_REDUCE_WG: u32 = 256;

/// Counters for tracking dynamic allocation on the GPU.
///
/// This must be kept in sync with the struct in `shader/shared/bump.wgsl`
#[derive(Clone, Copy, Debug, Default, Zeroable, Pod)]
#[repr(C)]
pub struct BumpAllocators {
    pub failed: u32,
    // Final needed dynamic size of the buffers. If any of these are larger
    // than the corresponding `_size` element reallocation needs to occur.
    pub binning: u32,
    pub ptcl: u32,
    pub tile: u32,
    pub seg_counts: u32,
    pub segments: u32,
    pub blend: u32,
    pub lines: u32,
}

#[derive(Default)]
pub struct BumpAllocatorMemory {
    pub total: u32,
    pub binning: BufferSize<u32>,
    pub ptcl: BufferSize<u32>,
    pub tile: BufferSize<Tile>,
    pub seg_counts: BufferSize<SegmentCount>,
    pub segments: BufferSize<PathSegment>,
    pub lines: BufferSize<LineSoup>,
}

impl BumpAllocators {
    pub fn memory(&self) -> BumpAllocatorMemory {
        let binning = BufferSize::new(self.binning);
        let ptcl = BufferSize::new(self.ptcl);
        let tile = BufferSize::new(self.tile);
        let seg_counts = BufferSize::new(self.seg_counts);
        let segments = BufferSize::new(self.segments);
        let lines = BufferSize::new(self.lines);
        BumpAllocatorMemory {
            total: binning.size_in_bytes()
                + ptcl.size_in_bytes()
                + tile.size_in_bytes()
                + seg_counts.size_in_bytes()
                + segments.size_in_bytes()
                + lines.size_in_bytes(),
            binning,
            ptcl,
            tile,
            seg_counts,
            segments,
            lines,
        }
    }
}

impl std::fmt::Display for BumpAllocatorMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\n \
                 \tTotal:\t\t\t{} bytes ({:.2} KB | {:.2} MB)\n\
                 \tBinning\t\t\t{} elements ({} bytes)\n\
                 \tPTCL\t\t\t{} elements ({} bytes)\n\
                 \tTile:\t\t\t{} elements ({} bytes)\n\
                 \tSegment Counts:\t\t{} elements ({} bytes)\n\
                 \tSegments:\t\t{} elements ({} bytes)\n\
                 \tLines:\t\t\t{} elements ({} bytes)",
            self.total,
            self.total as f32 / (1 << 10) as f32,
            self.total as f32 / (1 << 20) as f32,
            self.binning.len(),
            self.binning.size_in_bytes(),
            self.ptcl.len(),
            self.ptcl.size_in_bytes(),
            self.tile.len(),
            self.tile.size_in_bytes(),
            self.seg_counts.len(),
            self.seg_counts.size_in_bytes(),
            self.segments.len(),
            self.segments.size_in_bytes(),
            self.lines.len(),
            self.lines.size_in_bytes()
        )
    }
}

/// Storage of indirect dispatch size values.
///
/// The original plan was to reuse [`BumpAllocators`], but the WebGPU compatible
/// usage list rules forbid that being used as indirect counts while also
/// bound as writable.
#[derive(Clone, Copy, Debug, Default, Zeroable, Pod)]
#[repr(C)]
pub struct IndirectCount {
    pub count_x: u32,
    pub count_y: u32,
    pub count_z: u32,
    pub pad0: u32,
}

/// Uniform render configuration data used by all GPU stages.
///
/// This data structure must be kept in sync with the definition in
/// `shaders/shared/config.wgsl`.
#[derive(Clone, Copy, Debug, Default, Zeroable, Pod)]
#[repr(C)]
pub struct ConfigUniform {
    /// Width of the scene in tiles.
    pub width_in_tiles: u32,
    /// Height of the scene in tiles.
    pub height_in_tiles: u32,
    /// Width of the target in pixels.
    pub target_width: u32,
    /// Height of the target in pixels.
    pub target_height: u32,
    /// The base background color applied to the target before any blends.
    pub base_color: u32,
    /// Layout of packed scene data.
    pub layout: Layout,
    /// Size of line soup buffer allocation (in [`LineSoup`]s)
    pub lines_size: u32,
    /// Size of binning buffer allocation (in `u32`s).
    pub binning_size: u32,
    /// Size of tile buffer allocation (in [`Tile`]s).
    pub tiles_size: u32,
    /// Size of segment count buffer allocation (in [`SegmentCount`]s).
    pub seg_counts_size: u32,
    /// Size of segment buffer allocation (in [`PathSegment`]s).
    pub segments_size: u32,
    /// Size of blend spill buffer (in `u32` pixels).
    // TODO: Maybe store in TILE_WIDTH * TILE_HEIGHT blocks of pixels instead?
    pub blend_size: u32,
    /// Size of per-tile command list buffer allocation (in `u32`s).
    pub ptcl_size: u32,
    /// raea: 1 when the fine stage blends in linear light (set_blend_linear), 0 to blend the sRGB-encoded values
    pub blend_linear: u32,
}

/// CPU side setup and configuration.
#[derive(Default)]
pub struct RenderConfig {
    /// GPU side configuration.
    pub gpu: ConfigUniform,
    /// Workgroup counts for all compute pipelines.
    pub workgroup_counts: WorkgroupCounts,
    /// Sizes of all buffer resources.
    pub buffer_sizes: BufferSizes,
}

impl RenderConfig {
    pub fn new(layout: &Layout, width: u32, height: u32, base_color: &peniko::Color) -> Self {
        let new_width = width.next_multiple_of(TILE_WIDTH);
        let new_height = height.next_multiple_of(TILE_HEIGHT);
        let width_in_tiles = new_width / TILE_WIDTH;
        let height_in_tiles = new_height / TILE_HEIGHT;
        let n_path_tags = layout.path_tags_size();
        let workgroup_counts =
            WorkgroupCounts::new(layout, width_in_tiles, height_in_tiles, n_path_tags);
        let buffer_sizes = BufferSizes::new(layout, &workgroup_counts, width_in_tiles * height_in_tiles);
        Self {
            gpu: ConfigUniform {
                width_in_tiles,
                height_in_tiles,
                target_width: width,
                target_height: height,
                base_color: base_color.premultiply().to_rgba8().to_u32(),
                lines_size: buffer_sizes.lines.len(),
                binning_size: buffer_sizes.bin_data.len() - layout.bin_data_start,
                tiles_size: buffer_sizes.tiles.len(),
                seg_counts_size: buffer_sizes.seg_counts.len(),
                segments_size: buffer_sizes.segments.len(),
                blend_size: buffer_sizes.blend_spill.len(),
                ptcl_size: buffer_sizes.ptcl.len(),
                blend_linear: BLEND_LINEAR.load(std::sync::atomic::Ordering::Relaxed) as u32,
                layout: *layout,
            },
            workgroup_counts,
            buffer_sizes,
        }
    }
}

/// Type alias for a workgroup size.
pub type WorkgroupSize = (u32, u32, u32);

/// Computed sizes for all dispatches.
#[derive(Copy, Clone, Debug, Default)]
pub struct WorkgroupCounts {
    pub use_large_path_scan: bool,
    pub path_reduce: WorkgroupSize,
    pub path_reduce2: WorkgroupSize,
    pub path_scan1: WorkgroupSize,
    pub path_scan: WorkgroupSize,
    pub bbox_clear: WorkgroupSize,
    pub flatten: WorkgroupSize,
    pub draw_reduce: WorkgroupSize,
    pub draw_leaf: WorkgroupSize,
    pub clip_reduce: WorkgroupSize,
    pub clip_leaf: WorkgroupSize,
    pub binning: WorkgroupSize,
    pub tile_alloc: WorkgroupSize,
    pub path_count_setup: WorkgroupSize,
    // Note: `path_count` must use an indirect dispatch
    pub backdrop: WorkgroupSize,
    pub coarse: WorkgroupSize,
    pub path_tiling_setup: WorkgroupSize,
    // Note: `path_tiling` must use an indirect dispatch
    pub fine: WorkgroupSize,
}

impl WorkgroupCounts {
    pub fn new(
        layout: &Layout,
        width_in_tiles: u32,
        height_in_tiles: u32,
        n_path_tags: u32,
    ) -> Self {
        let n_paths = layout.n_paths;
        let n_draw_objects = layout.n_draw_objects;
        let n_clips = layout.n_clips;
        let path_tag_padded = align_up(n_path_tags, 4 * PATH_REDUCE_WG);
        let path_tag_wgs = path_tag_padded / (4 * PATH_REDUCE_WG);
        let use_large_path_scan = path_tag_wgs > PATH_REDUCE_WG;
        let reduced_size = if use_large_path_scan {
            align_up(path_tag_wgs, PATH_REDUCE_WG)
        } else {
            path_tag_wgs
        };
        let draw_object_wgs = n_draw_objects.div_ceil(PATH_BBOX_WG);
        let draw_monoid_wgs = draw_object_wgs.min(PATH_BBOX_WG);
        let flatten_wgs = n_path_tags.div_ceil(FLATTEN_WG);
        let clip_reduce_wgs = n_clips.saturating_sub(1) / CLIP_REDUCE_WG;
        let clip_wgs = n_clips.div_ceil(CLIP_REDUCE_WG);
        let path_wgs = n_paths.div_ceil(PATH_BBOX_WG);
        let width_in_bins = width_in_tiles.div_ceil(16);
        let height_in_bins = height_in_tiles.div_ceil(16);
        Self {
            use_large_path_scan,
            path_reduce: (path_tag_wgs, 1, 1),
            path_reduce2: (PATH_REDUCE_WG, 1, 1),
            path_scan1: (reduced_size / PATH_REDUCE_WG, 1, 1),
            path_scan: (path_tag_wgs, 1, 1),
            bbox_clear: (draw_object_wgs, 1, 1),
            flatten: (flatten_wgs, 1, 1),
            draw_reduce: (draw_monoid_wgs, 1, 1),
            draw_leaf: (draw_monoid_wgs, 1, 1),
            clip_reduce: (clip_reduce_wgs, 1, 1),
            clip_leaf: (clip_wgs, 1, 1),
            binning: (draw_object_wgs, 1, 1),
            tile_alloc: (path_wgs, 1, 1),
            path_count_setup: (1, 1, 1),
            backdrop: (path_wgs, 1, 1),
            coarse: (width_in_bins, height_in_bins, 1),
            path_tiling_setup: (1, 1, 1),
            fine: (width_in_tiles, height_in_tiles, 1),
        }
    }
}

/// Typed buffer size primitive.
#[derive(Copy, Clone, Eq, Default, Debug)]
pub struct BufferSize<T: Sized> {
    len: u32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Sized> BufferSize<T> {
    /// Creates a new buffer size from number of elements.
    pub const fn new(len: u32) -> Self {
        Self {
            // Each buffer binding must be large enough to hold at least one element to avoid
            // triggering validation errors.
            //
            // Note: not using `Ord::max` here because it doesn't support const eval yet (except
            // in nightly)
            len: if len > 0 { len } else { 1 },
            _phantom: std::marker::PhantomData,
        }
    }

    /// Creates a new buffer size from size in bytes.
    pub const fn from_size_in_bytes(size: u32) -> Self {
        Self::new(size / size_of::<T>() as u32)
    }

    /// Returns the number of elements.
    #[expect(clippy::len_without_is_empty, reason = "The buffer can never be empty")]
    pub const fn len(self) -> u32 {
        self.len
    }

    /// Returns the size in bytes.
    pub const fn size_in_bytes(self) -> u32 {
        size_of::<T>() as u32 * self.len
    }

    /// Returns the size in bytes aligned up to the given value.
    pub const fn aligned_in_bytes(self, alignment: u32) -> u32 {
        align_up(self.size_in_bytes(), alignment)
    }
}

impl<T: Sized> PartialEq for BufferSize<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
    }
}

impl<T: Sized> PartialOrd for BufferSize<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.len.partial_cmp(&other.len)
    }
}

/// Computed sizes for all buffers.
#[derive(Copy, Clone, Debug, Default)]
pub struct BufferSizes {
    // Known size buffers
    pub path_reduced: BufferSize<PathMonoid>,
    pub path_reduced2: BufferSize<PathMonoid>,
    pub path_reduced_scan: BufferSize<PathMonoid>,
    pub path_monoids: BufferSize<PathMonoid>,
    pub path_bboxes: BufferSize<PathBbox>,
    pub draw_reduced: BufferSize<DrawMonoid>,
    pub draw_monoids: BufferSize<DrawMonoid>,
    pub info: BufferSize<u32>,
    pub clip_inps: BufferSize<Clip>,
    pub clip_els: BufferSize<ClipElement>,
    pub clip_bics: BufferSize<ClipBic>,
    pub clip_bboxes: BufferSize<ClipBbox>,
    pub draw_bboxes: BufferSize<DrawBbox>,
    pub bump_alloc: BufferSize<BumpAllocators>,
    pub indirect_count: BufferSize<IndirectCount>,
    pub bin_headers: BufferSize<BinHeader>,
    pub paths: BufferSize<Path>,
    // Bump allocated buffers
    pub lines: BufferSize<LineSoup>,
    pub bin_data: BufferSize<u32>,
    pub tiles: BufferSize<Tile>,
    pub seg_counts: BufferSize<SegmentCount>,
    pub segments: BufferSize<PathSegment>,
    pub blend_spill: BufferSize<u32>,
    pub ptcl: BufferSize<u32>,
}

impl BufferSizes {
    /// raea: `n_tiles`, the target's 16x16 tiles, sizes the bump-allocated buffers (`bump_sizes_for`)
    pub fn new(layout: &Layout, workgroups: &WorkgroupCounts, n_tiles: u32) -> Self {
        let n_paths = layout.n_paths;
        let n_draw_objects = layout.n_draw_objects;
        let n_clips = layout.n_clips;
        let path_tag_wgs = workgroups.path_reduce.0;
        let reduced_size = if workgroups.use_large_path_scan {
            align_up(path_tag_wgs, PATH_REDUCE_WG)
        } else {
            path_tag_wgs
        };
        let path_reduced = BufferSize::new(reduced_size);
        let path_reduced2 = BufferSize::new(PATH_REDUCE_WG);
        let path_reduced_scan = BufferSize::new(reduced_size);
        let path_monoids = BufferSize::new(path_tag_wgs * PATH_REDUCE_WG);
        let path_bboxes = BufferSize::new(n_paths);
        let binning_wgs = workgroups.binning.0;
        let draw_monoid_wgs = workgroups.draw_reduce.0;
        let draw_reduced = BufferSize::new(draw_monoid_wgs);
        let draw_monoids = BufferSize::new(n_draw_objects);
        let info = BufferSize::new(layout.bin_data_start);
        let clip_inps = BufferSize::new(n_clips);
        let clip_els = BufferSize::new(n_clips);
        let clip_bics = BufferSize::new(n_clips / CLIP_REDUCE_WG);
        let clip_bboxes = BufferSize::new(n_clips);
        let draw_bboxes = BufferSize::new(n_paths);
        let bump_alloc = BufferSize::new(1);
        let indirect_count = BufferSize::new(1);
        let n_paths_aligned = align_up(n_paths, 256);
        let paths = BufferSize::new(n_paths_aligned);
        let width_in_bins = workgroups.coarse.0;
        let height_in_bins = workgroups.coarse.1;
        let n_bins = width_in_bins * height_in_bins;
        let aligned_n_bins = align_up(n_bins, 256);
        let bin_headers = BufferSize::new(binning_wgs * aligned_n_bins);

        // The following buffer sizes have been hand picked to accommodate the vello test scenes as
        // well as paris-30k. These should instead get derived from the scene layout using
        // reasonable heuristics.
        // raea: the embedder's sizes when it set them (set_bump_allocation_sizes), these defaults otherwise or while
        // set_bump_full is on, grown for a target past 1080p (bump_sizes_for)
        let bump = bump_sizes_for(n_tiles);
        let bin_data = BufferSize::new(bump.bin_data.max(layout.bin_data_start + 1));
        let tiles = BufferSize::new(bump.tiles.max(1));
        let lines = BufferSize::new(bump.lines.max(1));
        let seg_counts = BufferSize::new(bump.seg_counts.max(1));
        let segments = BufferSize::new(bump.segments.max(1));
        // 16 * 16 (1 << 8) is one blend spill, so the default allows for 4096 spills.
        // raea: and room for every tile to spill as deep as the scene's layers nest (`blend_spill_for`)
        let blend_spill = BufferSize::new(bump.blend_spill.max(blend_spill_for(n_tiles, LAYER_DEPTH.get())).max(1));
        let ptcl = BufferSize::new(bump.ptcl.max(1));
        Self {
            path_reduced,
            path_reduced2,
            path_reduced_scan,
            path_monoids,
            path_bboxes,
            draw_reduced,
            draw_monoids,
            info,
            clip_inps,
            clip_els,
            clip_bics,
            clip_bboxes,
            draw_bboxes,
            bump_alloc,
            indirect_count,
            lines,
            bin_headers,
            paths,
            bin_data,
            tiles,
            seg_counts,
            segments,
            blend_spill,
            ptcl,
        }
    }
}

/// raea: the element counts of the seven bump-allocated buffers, as linebender/vello#1861's `BumpAllocationSizes`
/// (until it lands). The defaults are Vello's hand-picked ones. A scene that needs more than a buffer holds draws
/// without some of its geometry and reports no error, so smaller sizes are for an embedder that has checked its
/// scenes against them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BumpAllocationSizes {
    pub lines: u32,
    pub bin_data: u32,
    pub tiles: u32,
    pub seg_counts: u32,
    pub segments: u32,
    pub blend_spill: u32,
    pub ptcl: u32,
}

impl Default for BumpAllocationSizes {
    fn default() -> Self {
        Self {
            lines: 1 << 21,
            bin_data: 1 << 18,
            tiles: 1 << 21,
            seg_counts: 1 << 21,
            segments: 1 << 21,
            blend_spill: 1 << 20,
            ptcl: 1 << 23,
        }
    }
}

static BUMP_SIZES: std::sync::OnceLock<BumpAllocationSizes> = std::sync::OnceLock::new();

/// raea: sets the process's bump allocation sizes, once, before the first render; false when they were already set
/// (the earlier sizes stay).
pub fn set_bump_allocation_sizes(sizes: BumpAllocationSizes) -> bool {
    BUMP_SIZES.set(sizes).is_ok()
}

/// raea: ptcl words each tile has before anything is drawn (ptcl.wgsl's PTCL_INITIAL_ALLOC): coarse allocates past
/// `n_tiles` of them, so they're the part of ptcl a target's size alone uses
pub const PTCL_INITIAL_ALLOC: u32 = 64;

/// raea: the 16x16 tiles of a 1920x1080 target, the size the sizes set (or Vello's defaults) are for
const TILES_1080P: f64 = (120 * 68) as f64;

/// raea: the bump sizes a render of `n_tiles` 16x16 tiles has: the embedder's (or Vello's defaults, which
/// set_bump_full brings back) grown past 1080p with the target, as a scene's use grows with it: tiles and ptcl's
/// drawing room with its area, lines, seg_counts and segments with its side. ptcl holds every tile's initial
/// allocation on top, so a big target's own size never fills it (at 3840x2160 that alone is 2,073,600 words, which
/// filled 99% of raea's fixed 2M). bin_data and blend spill don't depend on the size.
pub fn bump_sizes_for(n_tiles: u32) -> BumpAllocationSizes {
    let base = if BUMP_FULL.load(std::sync::atomic::Ordering::Relaxed) { BumpAllocationSizes::default() } else { BUMP_SIZES.get().copied().unwrap_or_default() };
    grown(base, n_tiles)
}

fn grown(base: BumpAllocationSizes, n_tiles: u32) -> BumpAllocationSizes {
    let area = (n_tiles as f64 / TILES_1080P).max(1.0);
    let by = |n: u32, f: f64| (n as f64 * f).ceil().min(u32::MAX as f64) as u32;
    BumpAllocationSizes {
        lines: by(base.lines, area.sqrt()),
        tiles: by(base.tiles, area),
        seg_counts: by(base.seg_counts, area.sqrt()),
        segments: by(base.segments, area.sqrt()),
        ptcl: (n_tiles as u64 * PTCL_INITIAL_ALLOC as u64 + by(base.ptcl, area) as u64).min(u32::MAX as u64) as u32,
        ..base
    }
}

/// raea: the layers a tile's blend stack holds in the fine stage's registers (shader/shared/config.wgsl's
/// BLEND_STACK_SPLIT); each layer deeper is a spill of the tile's 16x16 pixels into the blend spill buffer
pub const BLEND_STACK_SPLIT: u32 = 4;

std::thread_local! {
    /// raea: the nesting of the scene this thread resolved last (resolve.rs `layer_depth`), which the render's config,
    /// made from that resolve's layout on the same thread (vello's render_encoding_coarse), sizes its blend spill by
    static LAYER_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub(crate) fn set_layer_depth(depth: u32) {
    LAYER_DEPTH.set(depth);
}

/// raea: the largest storage buffer the device binds, in bytes (`set_max_buffer_size`); wgpu's default limit until the
/// embedder gives its device's
static MAX_BUFFER_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(128 << 20);

/// raea: the largest storage buffer the device binds (wgpu's `max_storage_buffer_binding_size`), which a blend spill
/// grown for a deep scene stays within
pub fn set_max_buffer_size(bytes: u64) {
    MAX_BUFFER_BYTES.store(bytes.max(1 << 20), std::sync::atomic::Ordering::Relaxed);
}

/// raea: the blend spill (u32 pixels) a render of `n_tiles` needs when its layers nest `depth` deep: every tile spills
/// what's deeper than BLEND_STACK_SPLIT, 256 pixels a layer (coarse.wgsl's `scratch_size`), up to the largest buffer
/// the device binds. Vello's fixed size (1 << 20, raea-page's 1 << 18) holds 4096 spills, and a 1080x1920 page has 8160
/// tiles, so a single full-frame layer past the fourth (two nested masks in a faded group) overflowed it, and the
/// frame wasn't drawn even at Vello's own sizes (raea #247).
pub fn blend_spill_for(n_tiles: u32, depth: u32) -> u32 {
    let need = n_tiles as u64 * depth.saturating_sub(BLEND_STACK_SPLIT) as u64 * (TILE_WIDTH * TILE_HEIGHT) as u64;
    let most = MAX_BUFFER_BYTES.load(std::sync::atomic::Ordering::Relaxed) / 4;
    need.min(most).min(u32::MAX as u64) as u32
}

static BUMP_FULL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// raea: these defaults for the renders from now on (true), the embedder's sizes again (false): a frame drawn again
/// because it came out wrong (raea-page's redraw_full).
pub fn set_bump_full(on: bool) {
    BUMP_FULL.store(on, std::sync::atomic::Ordering::Relaxed);
}

const fn align_up(len: u32, alignment: u32) -> u32 {
    len + (len.wrapping_neg() & (alignment - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// raea-page's sizes (108bbd41)
    const PAGE: BumpAllocationSizes = BumpAllocationSizes { lines: 1 << 20, bin_data: 1 << 18, tiles: 1 << 20, seg_counts: 1 << 20, segments: 1 << 20, blend_spill: 1 << 18, ptcl: 1 << 21 };

    fn tiles(w: u32, h: u32) -> u32 {
        w.div_ceil(TILE_WIDTH) * h.div_ceil(TILE_HEIGHT)
    }

    /// raea #247: two nested masks in a faded group on a 1080x1920 page are layers seven deep over every tile
    #[test]
    fn blend_spill_holds_every_tile_spilling_as_deep_as_the_scene_nests() {
        let n = tiles(1080, 1920);
        assert_eq!(n, 8160);
        // four layers or fewer stay in the fine stage's registers: the set size alone
        assert_eq!(blend_spill_for(n, 4), 0);
        // one past four on every tile is 8160 spills of 256 pixels: past Vello's 4096 spills, which refused the frame
        assert_eq!(blend_spill_for(n, 5), 8160 * 256);
        assert!(blend_spill_for(n, 5) > BumpAllocationSizes::default().blend_spill);
        // the config a render makes after resolving a scene of that depth has the room
        set_layer_depth(7);
        let layout = Layout::default();
        let cfg = RenderConfig::new(&layout, 1080, 1920, &peniko::Color::TRANSPARENT);
        assert_eq!(cfg.gpu.blend_size, 8160 * 3 * 256);
        assert_eq!(cfg.buffer_sizes.blend_spill.len(), cfg.gpu.blend_size);
        // and never past the largest buffer the device binds (128 MiB until it's given)
        assert_eq!(blend_spill_for(tiles(15360, 8640), 40) as u64 * 4, 128 << 20);
        set_layer_depth(0);
    }

    #[test]
    fn bump_sizes_grow_past_1080p() {
        // up to 1080p the set sizes, ptcl with every tile's initial allocation on top
        let hd = grown(PAGE, tiles(1920, 1080));
        assert_eq!(hd, BumpAllocationSizes { ptcl: 8160 * 64 + (1 << 21), ..PAGE });
        assert_eq!(grown(PAGE, tiles(640, 360)).tiles, PAGE.tiles);
        for (w, h, area) in [(3840, 2160, 3.97), (7680, 4320, 15.88), (15360, 8640, 63.53)] {
            let n = tiles(w, h);
            let g = grown(PAGE, n);
            // the target's own ptcl words never fill it: the drawing room past them grows with the area
            assert!(g.ptcl as u64 >= n as u64 * 64 + PAGE.ptcl as u64 * 3, "{w}x{h}");
            assert!((g.tiles as f64 / PAGE.tiles as f64 - area).abs() < 0.01, "{w}x{h}: tiles {}", g.tiles);
            assert!((g.segments as f64 / PAGE.segments as f64 - area.sqrt()).abs() < 0.01, "{w}x{h}");
            assert_eq!((g.bin_data, g.blend_spill), (PAGE.bin_data, PAGE.blend_spill));
        }
        // 3840x2160: 2,073,600 initial words, which is 99% of the fixed 2M this replaces
        assert_eq!(grown(PAGE, tiles(3840, 2160)).ptcl, 2_073_600 + (2_097_152.0f64 * 32400.0 / 8160.0).ceil() as u32);
    }
}

static BLEND_LINEAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// raea: the renders from now on blend in linear light (true), as bevy_ui does on an sRGB target, or the sRGB-encoded
/// values (false, Vello's own and Chrome's way); vendor/vello_shaders fine.wgsl reads it as `config.blend_linear`.
pub fn set_blend_linear(on: bool) {
    BLEND_LINEAR.store(on, std::sync::atomic::Ordering::Relaxed);
}
