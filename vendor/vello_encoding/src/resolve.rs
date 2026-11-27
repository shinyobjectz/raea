// Copyright 2022 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use bytemuck::{Pod, Zeroable};
use peniko::{Extend, ImageData, InterpolationAlphaSpace};
use std::ops::Range;
use std::sync::Arc;

use super::{DrawTag, Encoding, PathTag, StreamOffsets, Style, Transform};

use crate::glyph_cache::GlyphCache;
use crate::image_cache::{ImageCache, Images};
use crate::ramp_cache::{RampCache, Ramps};

/// Layout of a packed encoding.
#[derive(Clone, Copy, Debug, Default, Zeroable, Pod)]
#[repr(C)]
pub struct Layout {
    /// Number of draw objects.
    pub n_draw_objects: u32,
    /// Number of paths.
    pub n_paths: u32,
    /// Number of clips.
    pub n_clips: u32,
    /// Start of binning data.
    pub bin_data_start: u32,
    /// Start of path tag stream.
    pub path_tag_base: u32,
    /// Start of path data stream.
    pub path_data_base: u32,
    /// Start of draw tag stream.
    pub draw_tag_base: u32,
    /// Start of draw data stream.
    pub draw_data_base: u32,
    /// Start of transform stream.
    pub transform_base: u32,
    /// Start of style stream.
    pub style_base: u32,
}

impl Layout {
    /// Creates a zeroed layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the path tag stream.
    pub fn path_tags<'a>(&self, data: &'a [u8]) -> &'a [PathTag] {
        let start = self.path_tag_base as usize * 4;
        let end = self.path_data_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    pub fn path_tags_size(&self) -> u32 {
        let start = self.path_tag_base * 4;
        let end = self.path_data_base * 4;
        end - start
    }

    /// Returns the path tag stream in chunks of 4.
    pub fn path_tags_chunked<'a>(&self, data: &'a [u8]) -> &'a [u32] {
        let start = self.path_tag_base as usize * 4;
        let end = self.path_data_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    /// Returns the path data stream.
    pub fn path_data<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        let start = self.path_data_base as usize * 4;
        let end = self.draw_tag_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    /// Returns the draw tag stream.
    pub fn draw_tags<'a>(&self, data: &'a [u8]) -> &'a [DrawTag] {
        let start = self.draw_tag_base as usize * 4;
        let end = self.draw_data_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    /// Returns the draw data stream.
    pub fn draw_data<'a>(&self, data: &'a [u8]) -> &'a [u32] {
        let start = self.draw_data_base as usize * 4;
        let end = self.transform_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    /// Returns the transform stream.
    pub fn transforms<'a>(&self, data: &'a [u8]) -> &'a [Transform] {
        let start = self.transform_base as usize * 4;
        let end = self.style_base as usize * 4;
        bytemuck::cast_slice(&data[start..end])
    }

    /// Returns the style stream.
    pub fn styles<'a>(&self, data: &'a [u8]) -> &'a [Style] {
        let start = self.style_base as usize * 4;
        bytemuck::cast_slice(&data[start..])
    }
}

/// Resolves and packs an encoding that contains only paths with solid color
/// fills.
///
/// Panics if the encoding contains any late bound resources (gradients, images
/// or glyph runs).
pub fn resolve_solid_paths_only(encoding: &Encoding, packed: &mut Vec<u8>) -> Layout {
    assert!(
        encoding.resources.patches.is_empty(),
        "this resolve function doesn't support late bound resources"
    );
    let data = packed;
    data.clear();
    let mut layout = Layout {
        n_paths: encoding.n_paths,
        n_clips: encoding.n_clips,
        ..Layout::default()
    };
    let SceneBufferSizes {
        buffer_size,
        path_tag_padded,
    } = SceneBufferSizes::new(encoding, &StreamOffsets::default());
    data.reserve(buffer_size);
    // Path tag stream
    layout.path_tag_base = size_to_words(data.len());
    data.extend_from_slice(bytemuck::cast_slice(&encoding.path_tags));
    for _ in 0..encoding.n_open_clips {
        data.extend_from_slice(bytemuck::bytes_of(&PathTag::PATH));
    }
    data.resize(path_tag_padded, 0);
    // Path data stream
    layout.path_data_base = size_to_words(data.len());
    data.extend_from_slice(bytemuck::cast_slice(&encoding.path_data));
    // Draw tag stream
    layout.draw_tag_base = size_to_words(data.len());
    // Bin data follows draw info
    layout.bin_data_start = encoding.draw_tags.iter().map(|tag| tag.info_size()).sum();
    data.extend_from_slice(bytemuck::cast_slice(&encoding.draw_tags));
    for _ in 0..encoding.n_open_clips {
        data.extend_from_slice(bytemuck::bytes_of(&DrawTag::END_CLIP));
    }
    // Draw data stream
    layout.draw_data_base = size_to_words(data.len());
    data.extend_from_slice(bytemuck::cast_slice(&encoding.draw_data));
    // Transform stream
    layout.transform_base = size_to_words(data.len());
    data.extend_from_slice(bytemuck::cast_slice(&encoding.transforms));
    // Style stream
    layout.style_base = size_to_words(data.len());
    data.extend_from_slice(bytemuck::cast_slice(&encoding.styles));
    layout.n_draw_objects = layout.n_paths;
    assert_eq!(buffer_size, data.len());
    layout
}

/// Resolver for late bound resources.
#[derive(Default)]
pub struct Resolver {
    glyph_cache: GlyphCache,
    glyphs: Vec<Arc<Encoding>>,
    ramp_cache: RampCache,
    image_cache: ImageCache,
    pending_images: Vec<PendingImage>,
    patches: Vec<ResolvedPatch>,
}

impl Resolver {
    /// Creates a new resource cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the atlas entry for `image` as dirty.
    ///
    /// If `image` is already resident in the atlas and appears in a later resolve pass,
    /// that pass will include it in the returned image uploads.
    pub fn mark_image_dirty(&mut self, image: &ImageData) {
        self.image_cache.mark_dirty(image);
    }

    /// Resolves late bound resources and packs an encoding. Returns the packed
    /// layout and computed ramp data.
    pub fn resolve<'a>(
        &'a mut self,
        encoding: &Encoding,
        packed: &mut Vec<u8>,
    ) -> (Layout, Ramps<'a>, Images<'a>) {
        // raea: the scene's layer nesting, which sizes its blend spill (config.rs `blend_spill_for`)
        crate::config::set_layer_depth(layer_depth(&encoding.draw_tags));
        let resources = &encoding.resources;
        if resources.patches.is_empty() {
            let layout = resolve_solid_paths_only(encoding, packed);
            // raea: the atlas as it is, nothing to upload. Images::default() said 0x0, and the renderer swapped its
            // persistent atlas for a 1x1 one, so the next scene with images found them "clean" in a new, empty atlas
            // and drew them blank (a page with an SVG mask: the mask's filtered layer is a solid-only scene)
            return (layout, Ramps::default(), self.image_cache.unchanged());
        }
        let patch_sizes = self.resolve_patches(encoding);
        self.resolve_pending_images();
        self.image_cache.finish_resolve();
        let data = packed;
        data.clear();
        let mut layout = Layout {
            n_paths: encoding.n_paths,
            n_clips: encoding.n_clips,
            ..Layout::default()
        };
        let SceneBufferSizes {
            buffer_size,
            path_tag_padded,
        } = SceneBufferSizes::new(encoding, &patch_sizes);
        data.reserve(buffer_size);
        // Path tag stream
        layout.path_tag_base = size_to_words(data.len());
        {
            let mut pos = 0;
            let stream = &encoding.path_tags;
            for patch in &self.patches {
                if let ResolvedPatch::GlyphRun { index, glyphs, .. } = patch {
                    layout.n_paths += 1;
                    let stream_offset = resources.glyph_runs[*index].stream_offsets.path_tags;
                    if pos < stream_offset {
                        data.extend_from_slice(bytemuck::cast_slice(&stream[pos..stream_offset]));
                        pos = stream_offset;
                    }
                    for glyph in &self.glyphs[glyphs.clone()] {
                        data.extend_from_slice(bytemuck::bytes_of(&PathTag::TRANSFORM));
                        data.extend_from_slice(bytemuck::cast_slice(&glyph.path_tags));
                    }
                    data.extend_from_slice(bytemuck::bytes_of(&PathTag::TRANSFORM));
                    data.extend_from_slice(bytemuck::bytes_of(&PathTag::PATH));
                }
            }
            if pos < stream.len() {
                data.extend_from_slice(bytemuck::cast_slice(&stream[pos..]));
            }
            for _ in 0..encoding.n_open_clips {
                data.extend_from_slice(bytemuck::bytes_of(&PathTag::PATH));
            }
            data.resize(path_tag_padded, 0);
        }
        // Path data stream
        layout.path_data_base = size_to_words(data.len());
        {
            let mut pos = 0;
            let stream = &encoding.path_data;
            for patch in &self.patches {
                if let ResolvedPatch::GlyphRun { index, glyphs, .. } = patch {
                    let stream_offset = encoding.resources.glyph_runs[*index]
                        .stream_offsets
                        .path_data;
                    if pos < stream_offset {
                        data.extend_from_slice(bytemuck::cast_slice(&stream[pos..stream_offset]));
                        pos = stream_offset;
                    }
                    for glyph in &self.glyphs[glyphs.clone()] {
                        data.extend_from_slice(bytemuck::cast_slice(&glyph.path_data));
                    }
                }
            }
            if pos < stream.len() {
                data.extend_from_slice(bytemuck::cast_slice(&stream[pos..]));
            }
        }
        // Draw tag stream
        layout.draw_tag_base = size_to_words(data.len());
        // Bin data follows draw info
        layout.bin_data_start = encoding.draw_tags.iter().map(|tag| tag.info_size()).sum();
        {
            data.extend_from_slice(bytemuck::cast_slice(&encoding.draw_tags));
            for _ in 0..encoding.n_open_clips {
                data.extend_from_slice(bytemuck::bytes_of(&DrawTag::END_CLIP));
            }
        }
        // Draw data stream
        layout.draw_data_base = size_to_words(data.len());
        {
            let mut pos = 0;
            let stream = &encoding.draw_data;
            for patch in &self.patches {
                match patch {
                    ResolvedPatch::Ramp {
                        draw_data_offset,
                        ramp_id,
                        extend,
                    } => {
                        if pos < *draw_data_offset {
                            data.extend_from_slice(bytemuck::cast_slice(
                                &encoding.draw_data[pos..*draw_data_offset],
                            ));
                        }
                        let index_mode = (ramp_id << 2) | *extend as u32;
                        data.extend_from_slice(bytemuck::bytes_of(&index_mode));
                        pos = *draw_data_offset + 1;
                    }
                    ResolvedPatch::GlyphRun { .. } => {}
                    ResolvedPatch::Image {
                        index,
                        draw_data_offset,
                    } => {
                        if pos < *draw_data_offset {
                            data.extend_from_slice(bytemuck::cast_slice(
                                &encoding.draw_data[pos..*draw_data_offset],
                            ));
                        }
                        if let Some((x, y)) = self.pending_images[*index].xy {
                            let xy = (x << 16) | y;
                            data.extend_from_slice(bytemuck::bytes_of(&xy));
                            pos = *draw_data_offset + 1;
                        } else {
                            // If we get here, we failed to allocate a slot for this image in the atlas.
                            // In this case, let's zero out the dimensions so we don't attempt to render
                            // anything.
                            // TODO: a better strategy: texture array? downsample large images?
                            data.extend_from_slice(&[0_u8; 8]);
                            pos = *draw_data_offset + 2;
                        }
                    }
                }
            }
            if pos < stream.len() {
                data.extend_from_slice(bytemuck::cast_slice(&stream[pos..]));
            }
        }
        // Transform stream
        layout.transform_base = size_to_words(data.len());
        {
            let mut pos = 0;
            let stream = &encoding.transforms;
            for patch in &self.patches {
                if let ResolvedPatch::GlyphRun {
                    index,
                    glyphs: _,
                    paint_transform,
                    transform,
                    scale,
                    hint,
                } = patch
                {
                    let run = &resources.glyph_runs[*index];
                    let stream_offset = run.stream_offsets.transforms;
                    if pos < stream_offset {
                        data.extend_from_slice(bytemuck::cast_slice(&stream[pos..stream_offset]));
                        pos = stream_offset;
                    }
                    if let Some(glyph_transform) = run.glyph_transform {
                        for glyph in &resources.glyphs[run.glyphs.clone()] {
                            let mut xform = *transform
                                * Transform {
                                    matrix: [1.0, 0.0, 0.0, -1.0],
                                    translation: [glyph.x * scale, glyph.y * scale],
                                }
                                * glyph_transform;
                            if *hint {
                                xform.translation[1] = xform.translation[1].round();
                            }
                            data.extend_from_slice(bytemuck::bytes_of(&xform));
                        }
                    } else {
                        for glyph in &resources.glyphs[run.glyphs.clone()] {
                            let mut xform = *transform
                                * Transform {
                                    matrix: [1.0, 0.0, 0.0, -1.0],
                                    translation: [glyph.x * scale, glyph.y * scale],
                                };
                            if *hint {
                                xform.translation[1] = xform.translation[1].round();
                            }
                            data.extend_from_slice(bytemuck::bytes_of(&xform));
                        }
                    }
                    data.extend_from_slice(bytemuck::bytes_of(paint_transform));
                }
            }
            if pos < stream.len() {
                data.extend_from_slice(bytemuck::cast_slice(&stream[pos..]));
            }
        }
        // Style stream
        layout.style_base = size_to_words(data.len());
        {
            let mut pos = 0;
            let stream = &encoding.styles;
            for patch in &self.patches {
                if let ResolvedPatch::GlyphRun { index, glyphs, .. } = patch {
                    let stream_offset = resources.glyph_runs[*index].stream_offsets.styles;
                    if pos < stream_offset {
                        data.extend_from_slice(bytemuck::cast_slice(&stream[pos..stream_offset]));
                        pos = stream_offset;
                    }
                    for glyph in &self.glyphs[glyphs.clone()] {
                        data.extend_from_slice(bytemuck::cast_slice(&glyph.styles));
                    }
                }
            }
            if pos < stream.len() {
                data.extend_from_slice(bytemuck::cast_slice(&stream[pos..]));
            }
        }
        self.glyphs.clear();
        layout.n_draw_objects = layout.n_paths;
        assert_eq!(buffer_size, data.len());
        (layout, self.ramp_cache.ramps(), self.image_cache.images())
    }

    fn resolve_patches(&mut self, encoding: &Encoding) -> StreamOffsets {
        self.ramp_cache.maintain();
        self.glyphs.clear();
        self.glyph_cache.maintain();
        self.image_cache.begin_resolve();
        self.pending_images.clear();
        self.patches.clear();
        let mut sizes = StreamOffsets::default();
        let resources = &encoding.resources;
        for patch in &resources.patches {
            match patch {
                Patch::Ramp {
                    draw_data_offset,
                    stops,
                    extend,
                    interpolation_alpha_space,
                } => {
                    let ramp_id = self.ramp_cache.add(
                        *interpolation_alpha_space,
                        &resources.color_stops[stops.clone()],
                    );
                    self.patches.push(ResolvedPatch::Ramp {
                        draw_data_offset: *draw_data_offset + sizes.draw_data,
                        ramp_id,
                        extend: *extend,
                    });
                }
                Patch::GlyphRun { index } => {
                    let mut run_sizes = StreamOffsets::default();
                    let run = &resources.glyph_runs[*index];
                    let glyphs = &resources.glyphs[run.glyphs.clone()];
                    let coords = &resources.normalized_coords[run.normalized_coords.clone()];
                    let paint_transform =
                        run.transform * run.brush_transform.unwrap_or(Transform::IDENTITY);
                    let mut hint = run.hint;
                    let mut font_size = run.font_size;
                    let mut transform = run.transform;
                    let mut scale = 1.0;
                    if hint {
                        // If hinting was requested and our transform matrix is just a uniform
                        // scale, then adjust our font size and cancel out the matrix. Otherwise,
                        // disable hinting entirely.
                        if transform.matrix[0] == transform.matrix[3]
                            && transform.matrix[1] == 0.0
                            && transform.matrix[2] == 0.0
                        {
                            scale = transform.matrix[0];
                            font_size *= scale;
                            transform.matrix = [1.0, 0.0, 0.0, 1.0];
                        } else {
                            hint = false;
                        }
                    }
                    let Some(mut session) = self.glyph_cache.session(
                        &run.font,
                        bytemuck::cast_slice(coords),
                        font_size,
                        run.font_embolden,
                        hint,
                        &run.style,
                    ) else {
                        continue;
                    };
                    let glyph_start = self.glyphs.len();
                    for glyph in glyphs {
                        let (encoding, stream_sizes) =
                            session.get_or_insert(glyph.id).unwrap_or_else(|| {
                                // HACK: We pretend that the encoding was empty.
                                // In theory, we should be able to skip this glyph, but there is also
                                // a corresponding entry in `resources`, which means that we would
                                // need to make the patching process skip this glyph.
                                (Arc::new(Encoding::new()), StreamOffsets::default())
                            });
                        run_sizes.add(&stream_sizes);
                        self.glyphs.push(encoding);
                    }
                    let glyph_end = self.glyphs.len();
                    run_sizes.path_tags += glyphs.len() + 2;
                    run_sizes.transforms += glyphs.len() + 1;
                    sizes.add(&run_sizes);
                    self.patches.push(ResolvedPatch::GlyphRun {
                        index: *index,
                        glyphs: glyph_start..glyph_end,
                        paint_transform,
                        transform,
                        scale,
                        hint,
                    });
                }
                Patch::Image {
                    draw_data_offset,
                    image,
                } => {
                    let index = self.pending_images.len();
                    self.pending_images.push(PendingImage {
                        image: image.clone(),
                        xy: None,
                    });
                    self.patches.push(ResolvedPatch::Image {
                        index,
                        draw_data_offset: *draw_data_offset + sizes.draw_data,
                    });
                }
            }
        }
        sizes
    }

    fn resolve_pending_images(&mut self) {
        'outer: loop {
            self.image_cache.restart_resolve_pass();
            // raea: an image the atlas could hold found no room in it at its largest
            let mut crowded = false;
            // Loop over the images, attempting to allocate them all into the atlas.
            for pending_image in &mut self.pending_images {
                // raea: an empty image has no place to find (the allocator gives none, and the atlas grew to its
                // largest looking): it's drawn at no size as it is, and isn't one left out
                if pending_image.image.width == 0 || pending_image.image.height == 0 {
                    continue;
                }
                pending_image.xy = loop {
                    if let Some(xy) = self.image_cache.get_or_insert(&pending_image.image) {
                        break Some(xy);
                    }
                    if self.image_cache.can_fit_image(&pending_image.image)
                        && self.image_cache.evict_stale_entries()
                    {
                        continue;
                    }
                    // We failed to allocate. Try to bump the atlas size.
                    if self.image_cache.bump_size() {
                        // We were able to increase the atlas size. Restart the outer loop.
                        continue 'outer;
                    } else if self.image_cache.can_fit_image(&pending_image.image) && self.image_cache.evict_unused() {
                        // raea: the atlas is at its largest: what this resolve hasn't used goes before an image
                        // is left out, the longest unused first
                        continue;
                    } else {
                        crowded |= self.image_cache.can_fit_image(&pending_image.image);
                        // If the atlas is already maximum size, there's nothing we can do. Set
                        // the xy field to None so this image isn't rendered and then carry on--
                        // other images might still fit.
                        break None;
                    }
                };
            }
            // raea: an image left out for want of room, with nothing unused left to let go: the atlas is packed again
            // with this resolve's images alone, in an order that packs, and the pass run again (once)
            if crowded && self.image_cache.repack_with(self.pending_images.iter().map(|pending| &pending.image)) {
                continue;
            }
            // If we made it here, we've either successfully allocated all images or we reached
            // the maximum atlas size.
            break;
        }
        // raea: an image with no place is drawn at no size and nothing says so: counted, for the embedder to refuse
        // the frame with (`take_images_left_out`)
        let left_out = self.pending_images.iter().filter(|pending| pending.xy.is_none() && pending.image.width > 0 && pending.image.height > 0).count();
        IMAGES_LEFT_OUT.fetch_add(left_out as u32, std::sync::atomic::Ordering::Relaxed);
    }
}

/// raea: how deep the scene's layers (begin clips, a clip layer or a blended one alike) nest at most: a tile's blend
/// stack is never deeper, as coarse.wgsl counts a begin clip that touches the tile into its `max_blend_depth`.
pub fn layer_depth(tags: &[DrawTag]) -> u32 {
    let (mut depth, mut deepest) = (0u32, 0u32);
    for tag in tags {
        if *tag == DrawTag::BEGIN_CLIP {
            depth += 1;
            deepest = deepest.max(depth);
        } else if *tag == DrawTag::END_CLIP {
            depth = depth.saturating_sub(1);
        }
    }
    deepest
}

static IMAGES_LEFT_OUT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// raea: how many images the resolves since the last call left out, the atlas having no room for them at its largest
/// size (or an image being larger than it): each is drawn at no size, and neither the resolve nor the render says so.
pub fn take_images_left_out() -> u32 {
    IMAGES_LEFT_OUT.swap(0, std::sync::atomic::Ordering::Relaxed)
}

/// Patch for a late bound resource.
#[derive(Clone)]
pub enum Patch {
    /// Gradient ramp resource.
    Ramp {
        /// Byte offset to the ramp id in the draw data stream.
        draw_data_offset: usize,
        /// Range of the gradient stops in the resource set.
        stops: Range<usize>,
        /// Extend mode for the gradient.
        extend: Extend,
        /// Interpolation alpha space for the gradient.
        interpolation_alpha_space: InterpolationAlphaSpace,
    },
    /// Glyph run resource.
    GlyphRun {
        /// Index in the glyph run buffer.
        index: usize,
    },
    /// Image resource.
    Image {
        /// Offset to the atlas coordinates in the draw data stream.
        draw_data_offset: usize,
        /// Underlying image data.
        image: ImageData,
    },
}

/// Image to be allocated in the atlas.
#[derive(Clone, Debug)]
struct PendingImage {
    image: ImageData,
    xy: Option<(u32, u32)>,
}

#[derive(Clone, Debug)]
enum ResolvedPatch {
    Ramp {
        /// Offset to the ramp id in draw data stream.
        draw_data_offset: usize,
        /// Resolved ramp index.
        ramp_id: u32,
        /// Extend mode for the gradient.
        extend: Extend,
    },
    GlyphRun {
        /// Index of the original glyph run in the encoding.
        index: usize,
        /// Range into the glyphs encoding range buffer.
        glyphs: Range<usize>,
        /// Transform used for the run brush.
        paint_transform: Transform,
        /// Global transform.
        transform: Transform,
        /// Additional scale factor to apply to translation.
        scale: f32,
        /// Whether the glyph was hinted.
        ///
        /// This determines whether the y-coordinate of the final position
        /// needs to be rounded.
        hint: bool,
    },
    Image {
        /// Index of pending image element.
        index: usize,
        /// Offset to the atlas location in the draw data stream.
        draw_data_offset: usize,
    },
}

struct SceneBufferSizes {
    /// Full size of the scene buffer in bytes.
    buffer_size: usize,
    /// Padded length of the path tag stream in bytes.
    path_tag_padded: usize,
}

impl SceneBufferSizes {
    /// Computes common scene buffer sizes for the given encoding and patch
    /// stream sizes.
    fn new(encoding: &Encoding, patch_sizes: &StreamOffsets) -> Self {
        let n_path_tags =
            encoding.path_tags.len() + patch_sizes.path_tags + encoding.n_open_clips as usize;
        let path_tag_padded = align_up(n_path_tags, 4 * crate::config::PATH_REDUCE_WG);
        let buffer_size = path_tag_padded
            + slice_size_in_bytes(&encoding.path_data, patch_sizes.path_data)
            + slice_size_in_bytes(
                &encoding.draw_tags,
                patch_sizes.draw_tags + encoding.n_open_clips as usize,
            )
            + slice_size_in_bytes(&encoding.draw_data, patch_sizes.draw_data)
            + slice_size_in_bytes(&encoding.transforms, patch_sizes.transforms)
            + slice_size_in_bytes(&encoding.styles, patch_sizes.styles);
        Self {
            buffer_size,
            path_tag_padded,
        }
    }
}

fn slice_size_in_bytes<T: Sized>(slice: &[T], extra: usize) -> usize {
    (slice.len() + extra) * size_of::<T>()
}

fn size_to_words(byte_size: usize) -> u32 {
    (byte_size / size_of::<u32>()) as u32
}

fn align_up(len: usize, alignment: u32) -> usize {
    len + (len.wrapping_neg() & (alignment as usize - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use peniko::{Blob, ImageAlphaType, ImageFormat};

    /// raea #247: the resolve measures how deep the scene's layers nest, and the render's config made after it gives
    /// the blend spill room for every tile to spill that deep: here layers six deep beside a lone one, two past the
    /// fine stage's four on each of a 1080x1920 page's 8160 tiles
    #[test]
    fn a_resolve_sizes_the_blend_spill_by_how_deep_its_layers_nest() {
        use crate::{DrawBeginClip, RenderConfig};
        let mut encoding = Encoding::new();
        let layer = DrawBeginClip::new(peniko::Mix::Normal.into(), 0.9);
        for _ in 0..6 {
            encoding.encode_begin_clip(layer);
        }
        for _ in 0..6 {
            encoding.encode_end_clip();
        }
        encoding.encode_begin_clip(layer);
        encoding.encode_end_clip();
        assert_eq!(layer_depth(&encoding.draw_tags), 6);
        let mut packed = Vec::new();
        let (layout, _, _) = Resolver::new().resolve(&encoding, &mut packed);
        let cfg = RenderConfig::new(&layout, 1080, 1920, &peniko::Color::TRANSPARENT);
        assert_eq!(cfg.gpu.blend_size, 8160 * 2 * 256);
        // a shallow scene after it is back at the set size
        let (layout, _, _) = Resolver::new().resolve(&Encoding::new(), &mut packed);
        let cfg = RenderConfig::new(&layout, 1080, 1920, &peniko::Color::TRANSPARENT);
        assert_eq!(cfg.gpu.blend_size, crate::bump_sizes_for(8160).blend_spill);
    }

    /// raea: frame `f`'s `n` images, each new (a blob of its own) and near a 1080p frame's size, a few px apart and
    /// differently in each frame: raea-page's layers under a moving camera, each a placeholder the size of its box
    fn frame(n: u32, f: u32) -> Encoding {
        let mut encoding = Encoding::new();
        for i in 0..n {
            encoding.encode_image(&image(1920 - (i * 37 + f * 11) % 23, 1080 - (i * 17 + f * 7) % 13), 1.0);
        }
        encoding
    }

    fn image(width: u32, height: u32) -> ImageData {
        ImageData { data: Blob::new(Arc::new([0_u8; 0])), format: ImageFormat::Rgba8, alpha_type: ImageAlphaType::Alpha, width, height }
    }

    fn placed(resolver: &Resolver) -> usize {
        resolver.pending_images.iter().filter(|pending| pending.xy.is_some()).count()
    }

    /// raea: every image of a frame gets a place while the frames before it held as many of their own. The atlas has
    /// the area for 28 this size and kept the last two frames' (36 of them), so the second frame drew 10 of its 18;
    /// with those let go but nothing packed again, 13 frames of 40 still drew 17.
    #[test]
    fn images_new_in_each_frame_all_get_a_place() {
        let (mut resolver, mut packed) = (Resolver::new(), Vec::new());
        for f in 0..40 {
            resolver.resolve(&frame(18, f), &mut packed);
            assert_eq!(placed(&resolver), 18, "frame {f}");
        }
        assert_eq!(take_images_left_out(), 0);
        // more than the atlas holds at once: the ones left out are counted, once
        resolver.resolve(&frame(40, 40), &mut packed);
        let left_out = 40 - placed(&resolver);
        assert!((12..40).contains(&left_out), "{left_out} of 40 left out");
        assert_eq!(take_images_left_out() as usize, left_out);
        assert_eq!(take_images_left_out(), 0);
        // an empty image has no place and isn't one left out, and the atlas doesn't grow looking for one
        let (mut resolver, mut empty) = (Resolver::new(), Encoding::new());
        empty.encode_image(&image(0, 16), 1.0);
        empty.encode_image(&image(16, 16), 1.0);
        let (_, _, images) = resolver.resolve(&empty, &mut packed);
        assert_eq!((images.width, images.images.len()), (1024, 1));
        assert_eq!((placed(&resolver), take_images_left_out()), (1, 0));
    }
}
