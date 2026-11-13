//! raea-sys-page: HTML and CSS pages in Bevy (docs/blitz-bevy.feature). Blitz works out what a page looks like
//! (blitz-dom: the DOM, stylo's styles, Taffy's and Blitz's layout); Vello paints it on Bevy's own GPU device into the
//! page's Bevy `Image`, which anything in Bevy then draws (a UI image, a 3D material, a game screen).
//!
//! - A filtered element's layer is painted apart and run through GPU passes before its page is drawn (filters.rs,
//!   B4a).
//! - A page is an entity with a [`Page`]: its size in CSS pixels and its scale. Its document lives in [`Docs`] (a
//!   non-send resource: stylo's data isn't shared across threads here), its picture in [`PageImage`].
//! - Each frame, in `PostUpdate`: what changed is restyled and laid out (Blitz's incremental resolve), and a page
//!   whose document changed is painted into a `vello::Scene` (blitz-paint through anyrender_vello's
//!   `VelloScenePainter`).
//! - The scene goes to the render world, where a `vello::Renderer` made from Bevy's `RenderDevice` draws it with
//!   `render_to_texture` into the page's texture. That's Vello's non-robust path, which never reads anything back
//!   (B1f); the texture is `Rgba8Unorm` with `STORAGE_BINDING`, sampled by Bevy as sRGB through a view format.
//!
//! Above it: raea-runtime-twins (elements as Bevy entities), raea-runtime-dom (TSX roots on a page),
//! raea-runtime-input (pointer and keys) and raea-page (a composition's page).

mod draw;
mod skipped;
pub use skipped::wait_drawn;
/// a page's loads (raea-sys-page-net: FileNet, FrameNet, the page's fetch, the image decoder)
pub use raea_sys_page_net as net;
pub use blitz_dom::not_applied; // what the page asked for and didn't get, said once when a render ends (#34)
/// Blitz's extension points as Bevy resources (S6d)
pub mod providers;
pub mod widgets;
pub use providers::{PageFonts, PageHtmlParser, PageNavigation, PageNet, PageShell};
pub use raea_sys_page_ops as ops;
mod snapshot;
pub use snapshot::{OFFSET_BODY, OFFSET_NONE, OFFSETS, STRIDE, snapshot, snapshot_by};
pub mod view3d;

pub use draw::{Placed, view, view_at};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use blitz_dom::{BaseDocument, DocumentConfig, LocalName, QualName, ns};
use ops::Nodes;
use blitz_traits::shell::Viewport;
use wgpu::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

/// A page: its viewport in CSS pixels and the device scale it's painted at.
#[derive(Component, Clone, Copy, Debug)]
pub struct Page {
    pub size: UVec2,
    pub scale: f32,
}

impl Page {
    /// The page's size in device pixels: its image's size, and the viewport Blitz is given (Blitz's `window_size` is
    /// in device pixels: it divides by `hidpi_scale` for the CSS viewport). Rounded up, so the image is never short
    /// of the page: at a fractional scale the CSS viewport can be a fraction of a CSS pixel wider than `size`.
    pub fn px(&self) -> UVec2 {
        (self.size.as_vec2() * self.scale).ceil().as_uvec2().max(UVec2::ONE)
    }
}

/// The time pages' CSS animations and transitions are at, in seconds: a render's frame time (`BlitzPage::at_frame`);
/// None, the App's clock (live play).
#[derive(Resource, Default)]
pub struct PageClock(pub Option<f64>);

/// A page whose image is straight alpha, not premultiplied: one Bevy's UI or sprites draw (they blend straight), as
/// an overlay over a scene (docs/blitz-bevy.feature B3 H5).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct StraightAlpha;

/// The page's picture, painted on the GPU each frame its document changes.
#[derive(Component, Clone, Debug)]
pub struct PageImage(pub Handle<Image>);

/// Every page's document, by entity.
#[derive(Default)]
pub struct Docs(pub HashMap<Entity, BaseDocument>);

impl Docs {
    /// A document for `page`, empty, at its size.
    pub fn open(&mut self, page: Entity, at: Page) -> &mut BaseDocument {
        self.0.entry(page).or_insert_with(|| BaseDocument::new(config(at)))
    }
}

/// A document's settings for a page: its viewport, styled on this thread (stylo's own pool would compete with Bevy's).
pub fn config(at: Page) -> DocumentConfig {
    let px = at.px();
    let viewport = Viewport { window_size: (px.x, px.y), hidpi_scale: at.scale, ..default() };
    let mut config = DocumentConfig { viewport: Some(viewport), style_threading: blitz_dom::StyleThreading::Sequential, ..default() };
    // an <iframe> loads its document, parsed by blitz-html, through FrameNet (files under RAEA_PAGE_ROOT only;
    // docs/blitz-bevy.feature B5c, off with RAEA_PAGE_IFRAMES=0)
    if iframes() {
        config.html_parser_provider = Some(std::sync::Arc::new(blitz_html::HtmlProvider));
        let _ = blitz_dom::net::FRAME_NET.set(std::sync::Arc::new(crate::net::FrameNet { roots: crate::net::page_roots() }));
    }
    config
}

/// A page's image made, or remade at its pixel size: `Rgba8UnormSrgb`, premultiplied, as raea-runtime-js's page (its
/// offscreen target), so whatever draws or reads one page draws or reads the other.
fn size_images(mut commands: Commands, pages: Query<(Entity, &Page, Option<&PageImage>)>, mut images: ResMut<Assets<Image>>) {
    for (e, page, have) in &pages {
        let px = page.px();
        if have.and_then(|h| images.get(&h.0)).is_some_and(|i| i.size() == px) {
            continue;
        }
        commands.entity(e).insert(PageImage(raea_sys_image::offscreen::target(&mut images, px.x, px.y)));
    }
}

/// RAEA_PAGE_TIMES: what the pages' frames cost on the CPU (docs/blitz-bevy.feature B2d), summed over the run and
/// written to stderr as one JSON line when the process ends: restyle and layout (blitz-dom's resolve), and paint
/// encode (blitz-paint into the Vello scene); in µs, with the number of page frames.
pub struct Times {
    resolve_us: std::sync::atomic::AtomicU64,
    encode_us: std::sync::atomic::AtomicU64,
    frames: std::sync::atomic::AtomicU64,
}

pub static TIMES: Times = Times { resolve_us: std::sync::atomic::AtomicU64::new(0), encode_us: std::sync::atomic::AtomicU64::new(0), frames: std::sync::atomic::AtomicU64::new(0) };

impl Times {
    fn add_resolve(&self, d: std::time::Duration) {
        self.resolve_us.fetch_add(d.as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    /// a page's paint encoded: one page frame
    fn add_encode(&self, d: std::time::Duration) {
        use std::sync::atomic::Ordering::Relaxed;
        self.encode_us.fetch_add(d.as_micros() as u64, Relaxed);
        self.frames.fetch_add(1, Relaxed);
    }

    /// The sums so far, as RAEA_PAGE_TIMES writes them; None when it isn't set or nothing was painted.
    pub fn report(&self) -> Option<String> {
        use std::sync::atomic::Ordering::Relaxed;
        let n = self.frames.load(Relaxed);
        (std::env::var_os("RAEA_PAGE_TIMES").is_some() && n > 0).then(|| {
            let (r, e) = (self.resolve_us.load(Relaxed), self.encode_us.load(Relaxed));
            format!("{{\"page_frames\": {n}, \"resolve_ms\": {:.3}, \"encode_ms\": {:.3}}}", r as f64 / n as f64 / 1e3, e as f64 / n as f64 / 1e3)
        })
    }
}

/// The scenes painted this frame, for the render world.
#[derive(Resource, Default, Clone)]
pub struct Painted(pub Arc<Mutex<Vec<Drawn>>>);

/// A page's paint: its image, its size in device px, its scene, its filtered layers (inner ones first), its print
/// (`raea_sys_filters::Painter::print`: the same as last frame's, the same picture), whether its image is straight alpha,
/// and whether it blends in linear light (a bevy_ui page, `data-raea-blend="linear"`: Vello's set_blend_linear).
pub type Drawn = (Handle<Image>, UVec2, vello::Scene, Vec<raea_sys_filters::Filtered>, u64, bool, bool);

/// Each page restyled and laid out (what changed).
pub fn resolve(mut docs: NonSendMut<Docs>, pages: Query<(Entity, &Page)>, time: Res<Time>, clock: Res<PageClock>, mut scanned: Local<HashMap<Entity, usize>>) {
    for (e, page) in &pages {
        let Some(doc) = docs.0.get_mut(&e) else { continue };
        let at = clock.0.unwrap_or_else(|| time.elapsed_secs_f64());
        // a page resized or rescaled (the iPad sets its scale late): its viewport follows, or text lays out off its box
        let px = page.px();
        if doc.viewport().window_size != (px.x, px.y) || doc.viewport().hidpi_scale != page.scale {
            let mut v = doc.viewport().clone();
            v.window_size = (px.x, px.y);
            v.hidpi_scale = page.scale;
            doc.set_viewport(v);
        }
        let t0 = std::time::Instant::now();
        doc.resolve(at);
        // families it names that no face here has, from Google Fonts by name (raea-sys-fonts google.rs), looked for when its
        // nodes change: the second resolve below lays its text out in them. Only those raea/text loadFont asked for (or
        // every one with RAEA_PAGE_GOOGLE_FONTS=1): Chrome
        // loads only what a page links, so a family that isn't installed falls back there (ui-parity's p12 cases name
        // Inter, and so does every composition's root through sdk/react)
        if raea_sys_fonts::google::enabled() && scanned.insert(e, doc.tree().len()) != Some(doc.tree().len()) && raea_sys_fonts::google::load_missing(doc) {
            doc.resolve(at);
        }
        // the images that restyle asked for (a background's data URL: FileNet answers at once) wait as messages that
        // resolve takes in first: taken in now, not a frame late, so a render's first frame has them (retro/bisect
        // drew its sprites and text from the second frame on)
        doc.resolve(at);
        TIMES.add_resolve(t0.elapsed());
    }
}

/// Scenes the render world has drawn, given back for paint to reset and fill again instead of making new ones (ZeroHeap:
/// a new scene's encoding grows from empty each frame); at most SPARE_MAX are kept.
static SPARE: Mutex<Vec<vello::Scene>> = Mutex::new(Vec::new());
const SPARE_MAX: usize = 8;

/// Each page painted into a Vello scene.
pub fn paint(mut docs: NonSendMut<Docs>, pages: Query<(Entity, &Page, &PageImage, Has<StraightAlpha>)>, painted: Local<Painted>, mut commands: Commands) {
    let mut out = vec![];
    for (e, page, image, straight) in &pages {
        let Some(doc) = docs.0.get_mut(&e) else { continue };
        let px = page.px();
        // a scene the render world drew last frame, reset: its encoding keeps its capacity (ZeroHeap)
        let mut scene = SPARE.lock().unwrap().pop().unwrap_or_default();
        scene.reset();
        let mut painter = raea_sys_filters::Painter::new(&mut scene, px.x, px.y);
        let t1 = std::time::Instant::now();
        blitz_paint::paint_scene(&mut painter, doc, page.scale as f64, px.x, px.y, 0, 0);
        let (layers, print) = (std::mem::take(&mut painter.layers), painter.print());
        TIMES.add_encode(t1.elapsed());
        // a page with a bevy_ui node (sdk/render bevy3d/ui.tsx marks each root Node) blends in linear light, as
        // Bevy's UI does on its sRGB target; the rest blend the encoded values, as Chrome does
        let linear = doc.query_selector("[data-raea-blend=linear]").ok().flatten().is_some();
        out.push((image.0.clone(), px, scene, layers, print ^ ((linear as u64) << 63), straight, linear));
    }
    *painted.0.lock().unwrap() = out;
    commands.insert_resource(painted.clone());
}

/// While on (redraw_full), every page is drawn again however it's painted, with Vello at its own bump sizes.
static REDRAW_FULL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Every page drawn again from now on, with Vello's own bump buffers in place of raea's smaller ones (true), or as
/// before (false): a render's frame that came out all black drawn once more (sdk/react motion.rs `black_checked`),
/// so a scene past raea's sizes draws whole the second time.
pub fn redraw_full(on: bool) {
    REDRAW_FULL.store(on, std::sync::atomic::Ordering::Relaxed);
    // (once a scene overflowed raea's sizes, Vello's own stay on: skipped.rs)
    vello::low_level::vello_encoding::set_bump_full(on || skipped::STAY_FULL.load(std::sync::atomic::Ordering::Relaxed));
}

/// RAEA_VELLO_BUMP_USE: a line per scene drawn, its bump buffers' use against the sizes its render has (raea_gpu_device::bump).
fn bump_use(what: &str, scene: &vello::Scene, w: u32, h: u32) {
    match raea_gpu_device::bump::measure(scene, w, h) {
        Ok(u) => {
            let (name, full) = u.fullest();
            eprintln!("vello-bump-use {what} {w}x{h}: fullest {name} {:.1}%; {}", 100.0 * full, u.line());
        }
        Err(e) => eprintln!("vello-bump-use {what} {w}x{h}: {e}"),
    }
}

/// Whether Vello's bump buffers for a `w` x `h` render fit `device` (raea-gpu-device check.rs `bump_fits`): the reason not to draw it
/// when one doesn't. A page past 1080p, whose buffers grow with it, says their size on stderr, once a size.
fn bump_guard(device: &wgpu::Device, w: u32, h: u32) -> Result<(), String> {
    static SAID: std::sync::Mutex<Vec<(u32, u32)>> = std::sync::Mutex::new(Vec::new());
    let l = device.limits();
    let total = raea_gpu_device::check::bump_fits(w, h, (l.max_storage_buffer_binding_size as u64).min(l.max_buffer_size))?;
    // more than the same sizes at 1080p
    if total > raea_gpu_device::check::bump_fits(1920, 1080, u64::MAX)? && !SAID.lock().unwrap_or_else(|e| e.into_inner()).contains(&(w, h)) {
        SAID.lock().unwrap_or_else(|e| e.into_inner()).push((w, h));
        eprintln!("raea-page: a {w}x{h} page's Vello bump buffers are {} MB of GPU memory, grown with its size past 1080p's", total >> 20);
    }
    Ok(())
}

/// The page system: documents restyled, laid out and painted each frame, drawn by Vello on Bevy's device. raea-page's
/// PagePlugin adds it with the twins, the overlay and getComputedStyle.
pub struct PaintPlugin;

impl Plugin for PaintPlugin {
    fn build(&self, app: &mut App) {
        // a character the font list's faces don't have: CoreText's pick after the list's first face, as Chrome takes
        // it (whichever font context the page's documents share: raea-page's or bevy_text's)
        #[cfg(target_vendor = "apple")]
        parley::set_fallback_hook(raea_sys_fonts::coretext_fallback);
        parley::set_missing_hook(raea_sys_fonts::missing);
        raea_gpu_device::check::on_settle(skipped::settle);
        providers::defaults(app);
        widgets::install_page_plugins(app);
        app.init_non_send::<Docs>()
            .init_resource::<PageClock>()
            .init_resource::<view3d::ImageViews>()
            .init_resource::<view3d::ElementImages>()
            .init_resource::<view3d::SizedImages>()
            .init_resource::<view3d::NextLayer>()
            .add_systems(PostUpdate, (size_images.in_set(PageSystems::Commands), (view3d::size_to_boxes, view3d::element_images).chain().in_set(PageSystems::Commands), widgets::attach.in_set(PageSystems::Commands), resolve.in_set(PageSystems::Restyle), paint.in_set(PageSystems::Paint)));
        if let Some(r) = app.get_sub_app_mut(RenderApp) {
            r.init_resource::<draw::Vello>()
                .init_resource::<draw::Scenes>()
                .add_systems(ExtractSchedule, draw::extract)
                .add_systems(Render, draw::draw.in_set(RenderSystems::PrepareResources));
        }
    }
}

/// H8's frame order, as sets in `PostUpdate` (input, DOM events and React's commit come before, in `Update` and
/// earlier; game systems order themselves before `Commands`): the queued page changes, restyle and layout, the twins'
/// boxes, then paint. Bevy draws everything after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageSystems {
    /// PreUpdate: input to the pages, each page's own handlers (a React page's, with its commit) and then the twins'
    /// DOM events (pick.rs's route, inside picking's hover step; keys.rs's keys, after picking)
    Events,
    /// PreUpdate, after the events and picking: each host root's step (host.rs: its frame, timers, host events and
    /// answers, each one's commit), so the game systems in Update see what React made this frame
    Step,
    /// [`PageCommands`] applied to the documents
    Commands,
    /// stylo's restyle (blitz-dom's resolve restyles and lays out in one call: `Layout` follows it, empty for now)
    Restyle,
    Layout,
    /// twins spawned, despawned and given their elements' boxes
    Twins,
    /// each changed page painted into its Vello scene
    Paint,
}

/// A page whose elements a React tree renders (its ops write them: every page motion.rs's open_doc opens), so H9's
/// owners apply to it. An HTML page's elements have no live writer: a system's changes to them just land.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Rendered;

/// Whether a page's <iframe>s load, draw and run (docs/blitz-bevy.feature B5c): on unless RAEA_PAGE_IFRAMES=0.
pub fn iframes() -> bool {
    std::env::var_os("RAEA_PAGE_IFRAMES").is_none_or(|v| v != "0")
}

pub use ops::{ALL_DAMAGE, REPAINT_DAMAGE};

/// A picture's halvings handed to the painter ready made (blitz-paint preload_mips): a video's frame is then drawn
/// from the one nearest its drawn size. On the page's thread, where it is painted.
pub fn preload_mips(image: &blitz_dom::node::RasterImageData, levels: Vec<(u32, u32, Vec<u8>)>) {
    blitz_paint::preload_mips(image, levels.into_iter().map(|(w, h, px)| blitz_dom::node::RasterImageData::new(w, h, std::sync::Arc::new(px))).collect());
}

/// A document for a page `w` x `h` on its own entity: html, head and a body with no margin and no background (the
/// page is drawn over 3D and video), which is the root the composition's elements go in. (host.rs opens a root's so.)
pub fn open_doc(world: &mut World, w: u32, h: u32, fonts: Option<blitz_dom::FontContext>) -> (Entity, Nodes) {
    let at = Page { size: UVec2::new(w, h), scale: 1.0 };
    // its elements are React's (B3 H9: what Bevy writes to them is checked against what React set)
    let page = world.spawn((at, Rendered)).id();
    // the App's providers (S6d's resources: PaintPlugin's FileNet unless the App has its own net) and `fonts`, else
    // the App's collection
    let mut config = crate::providers::config_in(world, at, fonts);
    crate::net::install_decoder();
    // RAEA_PAGE_ROOT: where a root-absolute URL that isn't on disk is looked for (an app's public/, served at "/"; a
    // list as PATH is)
    if config.net_provider.is_none() {
        config.net_provider = Some(Arc::new(crate::net::FileNet { roots: crate::net::page_roots() }));
    }
    config.base_url = Some("file:///".into());
    let mut doc = blitz_dom::BaseDocument::new(config);
    let body = {
        let mut m = doc.mutate();
        let el = |m: &mut blitz_dom::DocumentMutator, tag: &str| m.create_element(QualName { prefix: None, ns: ns!(html), local: LocalName::from(tag) }, vec![]);
        let html = el(&mut m, "html");
        let root = m.doc.root_node().id;
        m.append_children(root, &[html]);
        let head = el(&mut m, "head");
        let body = el(&mut m, "body");
        m.append_children(html, &[head, body]);
        m.set_attribute(body, QualName { prefix: None, ns: ns!(), local: LocalName::from("style") }, "margin:0;background:transparent");
        body
    };
    world.non_send_mut::<Docs>().0.insert(page, doc);
    (page, Nodes::new(body))
}
