//! raea's 2D page on Blitz (docs/blitz-bevy.feature B1e): the same calls as raea-runtime-js's page (src/page.rs there),
//! so sdk/react's BevyPage drives either. The composition's elements arrive as raea-runtime-js's wants, are diffed into its
//! wire ops by the same Tree, and are applied to a Blitz document (ops.rs); the page is painted by Vello on the App's
//! device into its Image (lib.rs). A render reads a frame back (raea-runtime-js's offscreen readback, the MP4 encoder's one
//! copy); live play draws the page's texture where it is.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::ExitCondition;
use raea_core_wire::tree::{Tree, Want};
use blitz_dom::LocalName;

use raea_sys_page::ops::{self, Nodes};
use raea_sys_page::{Docs, Page, PageImage}; use crate::PagePlugin;

mod read;
mod manifest;
mod place;

type R<T> = Result<T, String>;

/// Updates a page waits for its images and stylesheets before it's drawn anyway.
const LOADING_UPDATES: usize = 600;

pub struct BlitzPage {
    app: App,
    page: Entity,
    pub w: u32,
    pub h: u32,
    tree: Tree,
    nodes: Nodes,
    /// the faces every document here is given (B0d)
    fonts: Option<blitz_dom::FontContext>,
    /// each video's decoder and the frame each video element shows
    videos: Videos,
    /// elements whose picture is a texture (place_texture): the placeholder each holds
    views: HashMap<u64, vello::peniko::ImageData>,
    /// each video element's frame texture (place_frame), written again as its frame changes
    frames: HashMap<u64, bevy::render::render_resource::Texture>,
    /// each <Screen>'s nested composition: a document of its own, drawn into its own Image
    subs: HashMap<String, Sub>,
    /// a render's clock: this frame (s), the frame before it (s) and its frame rate
    clock: Option<(f64, f64, f64)>,
    /// RAEA_PAGE_DUMP writes this page (`dump`); off for a page beside the composition's (raea-render's image rows)
    dumps: bool,
}

/// The label of a render's frame copy, the one copy to the CPU a render makes (its encoder's: B1d, tools/readbacks.sh)
pub const ENCODER_COPY: &str = "raea encoder frame";
/// A <Screen>'s picture read back for the 3D it's drawn in: a still's, or a fed render's on the screen's first frame
/// (after that it's copied on the GPU, Engine::copy_screen; B1d)
const SCREEN_COPY: &str = "raea screen picture";

/// A <Screen>'s page: its entity (document and Image), size, and its elements as diffed and applied.
struct Sub {
    page: Entity,
    w: u32,
    h: u32,
    tree: Tree,
    nodes: Nodes,
}

/// Node `n` drawn as an image: `data` (RGBA8, straight) its raster, `w` x `h`, its layout made again for the picture's
/// size.
pub fn picture(doc: &mut blitz_dom::BaseDocument, n: blitz_dom::NodeId, w: u32, h: u32, data: vello::peniko::Blob<u8>) {
    let Some(node) = doc.get_node_mut(n) else { return };
    let Some(el) = node.element_data_mut() else { return };
    el.special_data = blitz_dom::node::SpecialElementData::Image(Box::new(blitz_dom::node::ImageData::Raster(blitz_dom::node::RasterImageData { width: w, height: h, data })));
    node.clear_layout_cache();
    node.insert_damage(raea_sys_page::ALL_DAMAGE);
    node.mark_ancestors_dirty();
}

/// Video elements' frames: a decoder per file, and the frame each element shows (file, frame index), so a frame
/// already shown isn't decoded again.
#[derive(Default)]
pub struct Videos {
    decoders: HashMap<std::path::PathBuf, Box<dyn raea_io_media::VideoDecoder>>,
    shown: HashMap<u64, (std::path::PathBuf, i64)>,
}

impl Videos {
    /// `v`'s frame at its time, or None when its element shows that frame already. A time past the source's last
    /// frame (a looped or held clip run long) gives its last frame, as raea-runtime-js's page does.
    pub fn frame(&mut self, v: &raea_core_wire::tree::VideoAt) -> R<Option<raea_io_media::Frame>> {
        if !self.decoders.contains_key(&v.file) {
            // RAEA_MEDIA's, the platform's, the portable one, then the ffmpeg CLI, the first that opens it: a WebM is read
            // by ffmpeg alone (#113: it was refused, "no video track", where the platform's was the only one asked)
            let mut why = Vec::new();
            let d = raea_io_media::preferred().into_iter().find_map(|b| b.open_video(&v.file).map_err(|e| why.push(format!("{}: {e}", b.name()))).ok());
            let d = d.ok_or_else(|| format!("{}: can't be decoded ({})", v.file.display(), why.join("; ")))?;
            self.decoders.insert(v.file.clone(), d);
        }
        let d = self.decoders.get_mut(&v.file).unwrap();
        let key = d.index_at(v.at).map(|i| i as i64).unwrap_or((v.at * 1e6) as i64 + (1 << 40));
        if self.shown.get(&v.id).is_some_and(|k| k.0 == v.file && k.1 == key) {
            return Ok(None);
        }
        let f = match d.frame_at(v.at) {
            Ok(f) => f,
            Err(e) => {
                let i = d.info();
                let end = (i.duration - 0.5 / i.fps.max(1.0)).max(0.0);
                if v.at > end { d.frame_at(end).map_err(|e2| format!("{}: {e2} (asked for {:.3} s: {e})", v.file.display(), v.at))? } else { return Err(format!("{}: {e}", v.file.display())) }
            }
        };
        Ok(Some(f))
    }

    /// `v`'s frame is now its element's picture.
    pub fn shows(&mut self, v: raea_core_wire::tree::VideoAt) {
        let Some(d) = self.decoders.get_mut(&v.file) else { return };
        let key = d.index_at(v.at).map(|i| i as i64).unwrap_or((v.at * 1e6) as i64 + (1 << 40));
        self.shown.insert(v.id, (v.file, key));
    }

    /// Each video element of one document (`nodes`'s) shown at its time: what a page with no nested documents or
    /// texture slots needs (sdk/react's WorldPagePlugin).
    pub fn place(&mut self, doc: &mut blitz_dom::BaseDocument, nodes: &Nodes, videos: Vec<raea_core_wire::tree::VideoAt>) -> R<()> {
        for v in videos {
            let Some(n) = ops::node(nodes, v.id) else { continue };
            if let Some(f) = self.frame(&v)? {
                // a framed video's crop, as BlitzPage::place_videos takes it (its box as last laid out)
                let crop = v.fit.and_then(|fit| {
                    let size = doc.get_node(n).filter(|n| n.is_element())?.final_layout().size;
                    fit_rect(size.width, size.height, f.width, f.height, &fit)
                });
                match crop {
                    Some([x, y, w, h]) => {
                        let row = f.width as usize * 4;
                        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
                        for r in y..y + h {
                            let at = r as usize * row + x as usize * 4;
                            px.extend_from_slice(&f.rgba[at..at + w as usize * 4]);
                        }
                        picture(doc, n, w, h, vello::peniko::Blob::new(Arc::new(px)));
                    }
                    None => picture(doc, n, f.width, f.height, vello::peniko::Blob::new(Arc::new(f.rgba))),
                }
                self.shows(v);
            }
        }
        Ok(())
    }

    /// Element `id` shows something else.
    pub fn forget(&mut self, id: u64) {
        self.shown.remove(&id);
    }
}

/// The part of a `fw` x `fh` frame a framed video shows in its `bw` x `bh` box ([x, y, w, h] in the frame's pixels),
/// shown stretched to the box (its aspect is the box's): the frame at `fit.scale` times cover (at least cover, so it
/// always fills), with `fit.at` on `fit.to` as near as that keeps it inside the frame. The size is rounded from the box
/// and the scale alone, so it holds while they do (a new size lays the element out again); the origin rounds to whole
/// pixels, the same in every path (render, play, a plain App). None: no box yet, or no frame.
pub fn fit_rect(bw: f32, bh: f32, fw: u32, fh: u32, fit: &raea_core_wire::tree::VideoFit) -> Option<[u32; 4]> {
    if !(bw >= 1.0 && bh >= 1.0) || fw == 0 || fh == 0 {
        return None;
    }
    let s = (bw / fw as f32).max(bh / fh as f32) * fit.scale.max(1.0);
    let (w, h) = (((bw / s).round() as u32).clamp(1, fw), ((bh / s).round() as u32).clamp(1, fh));
    let x = (fit.at.0 * fw as f32 - fit.to.0 * w as f32).round().clamp(0.0, (fw - w) as f32) as u32;
    let y = (fit.at.1 * fh as f32 - fit.to.1 * h as f32).round().clamp(0.0, (fh - h) as f32) as u32;
    Some([x, y, w, h])
}

/// RAEA_PAGE_TIMES: the frames' CPU cost said as the page goes (raea_sys_page::TIMES)
impl Drop for BlitzPage {
    fn drop(&mut self) {
        if let Some(r) = raea_sys_page::TIMES.report() {
            eprintln!("raea-page times: {r}");
        }
    }
}

impl BlitzPage {
    /// A page `w` x `h`, transparent where nothing draws, with the WOFF2 faces in `fonts` (raea's core/fonts), on the
    /// device `render` makes (the process's shared device, so live play draws the page's texture as it is); None:
    /// Bevy's own.
    pub fn new_on(w: u32, h: u32, fonts: Option<&Path>, render: Option<bevy::render::settings::RenderCreation>) -> R<BlitzPage> {
        let mut app = App::new();
        let mut group = DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, close_when_requested: false, ..default() })
            .set(bevy::log::LogPlugin { level: bevy::log::Level::WARN, filter: format!("{}bevy_log=off,bevy_asset::io::web=error,bevy_asset::io::source=error,fontdb=error", bevy::log::DEFAULT_FILTER), ..default() })
            // the render world stays the App's sub-app, as raea-runtime-js's page keeps it: a readback waits on the device it
            // finds there (offscreen.rs), and the page's textures are looked up in it. Pipelined, the frame's readback
            // ran 32 updates without a wait and came back empty under a busy 3D render (kitchen/sky.tsx, real/*.tsx)
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>();
        if let Some(render_creation) = render {
            group = group.set(bevy::render::RenderPlugin { render_creation, ..default() });
        }
        // the registered Bevy plugins' changes, as raea-render's Engine's, and no WinitPlugin (raea_runtime_plugins::group):
        // two Apps in a process can't each make an event loop (the plugin build's page panicked; #182 on Linux)
        app.add_plugins(raea_runtime_plugins::group(group)).add_plugins(PagePlugin);
        app.finish();
        app.cleanup();
        raea_runtime_js::one_thread(&mut app);
        let config_fonts = fonts.map(raea_sys_fonts::context);
        // bevy_text on the same faces (B0d)
        if let Some(dir) = fonts {
            raea_sys_fonts::share_with_text(app.world_mut(), dir);
        }
        let (page, nodes) = raea_sys_page::open_doc(app.world_mut(), w, h, config_fonts.clone());
        // the image is made by the first update
        app.update();
        Ok(BlitzPage { app, page, w, h, tree: Tree::default(), nodes, fonts: config_fonts, videos: Videos::default(), views: HashMap::new(), frames: HashMap::new(), subs: HashMap::new(), clock: None, dumps: true })
    }

    /// The page's document replaced by an HTML file, parsed by Blitz (an HTML page, a WPT test); its URLs resolve
    /// beside it, and root-absolute ones under `root` when given.
    pub fn open_html(&mut self, path: &Path, root: Option<&Path>) -> R<()> {
        let html = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let at = Page { size: UVec2::new(self.w, self.h), scale: 1.0 };
        let mut config = raea_sys_page::config(at);
        config.net_provider = Some(Arc::new(raea_sys_page::net::FileNet { roots: root.map(Path::to_path_buf).into_iter().collect() }));
        let abs = dunce::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        config.base_url = Some(url::Url::from_file_path(&abs).map_err(|_| format!("{}: not a file URL", abs.display()))?.to_string());
        config.font_ctx = self.fonts.clone();
        // a byte order mark is the decoder's, never the document's (it also hid `<?xml` from Blitz's sniffing); the
        // file type picks the parser as Chrome's MIME type does, .xht/.xhtml/.xml being application/xhtml+xml
        let html = html.strip_prefix('\u{feff}').unwrap_or(&html);
        let xml = matches!(path.extension().and_then(|e| e.to_str()), Some("xht" | "xhtml" | "xml"));
        let doc = match xml {
            true => blitz_html::HtmlDocument::from_xml(html, config),
            false => blitz_html::HtmlDocument::from_html(html, config),
        }
        .into_inner();
        self.app.world_mut().non_send_mut::<Docs>().0.insert(self.page, doc);
        self.app.world_mut().entity_mut(self.page).remove::<raea_sys_page::Rendered>();
        self.tree = Tree::default();
        Ok(())
    }

    /// The page made to match `want` (parents before children) with each parent's children in `order`'s order.
    pub fn sync(&mut self, want: &[Want], order: &HashMap<u64, Vec<u64>>) {
        let starts = self.mount_starts(want);
        let (ops, gone) = self.tree.diff(want, order);
        if ops.is_empty() {
            return;
        }
        // an element gone or made again has no picture: its video's frame and its placeholder are placed again
        for id in gone.iter().copied().chain(ops.iter().filter_map(|o| match o {
            raea_core_wire::Op::Create { id, .. } => Some(*id),
            _ => None,
        })) {
            self.videos.forget(id);
            self.frames.remove(&id);
            self.views.remove(&id);
        }
        let mut docs = self.app.world_mut().non_send_mut::<Docs>();
        let doc = docs.0.get_mut(&self.page).expect("the page's document");
        ops::apply(doc, &mut self.nodes, ops);
        // a render that skips frames (`--at`, a frame alone) meets elements after they began: at the composition's start,
        // or their <Sequence>'s. Their CSS animations run from there, as in a render of every frame: the document is
        // resolved at the earliest (between the last frame drawn and this one, so time still only goes forward), and each
        // element that began later has its animations begun that much later. Resolving every element at the earliest
        // <Sequence> start put css/anim.tsx's A, in none, 10 frames behind; with no <Sequence> at all, every animation
        // began on the frame itself (11-22).
        if let Some(at) = starts.iter().map(|s| s.1).min_by(f64::total_cmp) {
            doc.resolve(at);
            for &(id, t) in starts.iter().filter(|s| s.1 > at) {
                if let Some(n) = ops::node(&self.nodes, id) {
                    doc.delay_animations(n, t - at);
                }
            }
        }
    }

    /// Each element `want` adds and when it began, in seconds: its <Sequence>'s start (data-raea-start, frames), else
    /// the composition's (0), when that lies after the last frame drawn and before this one.
    fn mount_starts(&self, want: &[Want]) -> Vec<(u64, f64)> {
        let Some((now, before, fps)) = self.clock else {
            return vec![];
        };
        let by: HashMap<u64, &Want> = want.iter().map(|w| (w.id, w)).collect();
        let start_of = |mut id: u64| {
            while let Some(w) = by.get(&id) {
                if let Some(v) = w.props.get("data-raea-start") {
                    return v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()));
                }
                id = w.parent;
            }
            None
        };
        want.iter()
            .filter(|w| ops::node(&self.nodes, w.id).is_none())
            .map(|w| (w.id, start_of(w.id).unwrap_or(0.0) / fps))
            .filter(|&(_, t)| t > before && t < now)
            .collect()
    }

    fn loading(&mut self) -> bool {
        let docs = self.app.world().non_send::<Docs>();
        docs.0.values().any(|d| d.has_pending_critical_resources())
    }

    /// The document and node holding element `id`: the page's, or a <Screen>'s.
    fn locate(&self, id: u64) -> Option<(Entity, blitz_dom::NodeId)> {
        ops::node(&self.nodes, id).map(|n| (self.page, n)).or_else(|| self.subs.values().find_map(|s| ops::node(&s.nodes, id).map(|n| (s.page, n))))
    }

    fn gpu(&self, image: &Handle<Image>) -> Option<bevy::render::render_resource::Texture> {
        let render = self.app.get_sub_app(bevy::render::RenderApp)?;
        let images = render.world().get_resource::<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>()?;
        images.get(image).map(|g| g.texture.clone())
    }

    fn image_of(&self, page: Entity) -> Option<Handle<Image>> {
        self.app.world().get::<PageImage>(page).map(|i| i.0.clone())
    }

    pub fn app(&mut self) -> &mut App {
        &mut self.app
    }

    /// The page's size as drawn.
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// Whether the page's GPU still runs work (raea-gpu-device check.rs `canary`).
    pub fn canary(&self) -> R<()> {
        let render = self.app.get_sub_app(bevy::render::RenderApp).ok_or("the page's render world")?;
        let device = render.world().resource::<bevy::render::renderer::RenderDevice>().wgpu_device();
        let queue = render.world().resource::<bevy::render::renderer::RenderQueue>();
        raea_gpu_device::check::canary(device, queue)
    }

    /// The page drawn and read back is `page` (a root the host mounted on this App: an HTML composition, html.rs).
    pub fn show(&mut self, page: Entity) {
        self.page = page;
    }

    /// The page drawn at frame `f` of a composition at `fps`: its CSS animations and transitions where they are then.
    pub fn at_frame(&mut self, f: i64, fps: f64) {
        let now = f as f64 / fps;
        self.clock = Some((now, self.clock.map_or(f64::NEG_INFINITY, |c| c.0), fps));
        self.app.world_mut().resource_mut::<raea_sys_page::PageClock>().0 = Some(now);
    }

    /// The page and every <Screen> drawn and read back together: the page's rows, and each screen's by name.
    pub fn frame_all(&mut self) -> R<(Vec<u8>, Vec<(String, u32, u32, Vec<u8>)>)> {
        if self.subs.is_empty() {
            return Ok((self.frame()?, vec![]));
        }
        for _ in 0..LOADING_UPDATES {
            if !self.loading() {
                break;
            }
            self.app.update();
            std::thread::yield_now();
        }
        self.dump();
        let mut keys: Vec<String> = self.subs.keys().cloned().collect();
        keys.sort();
        let mut images = vec![(self.image(), self.w, self.h)];
        for k in &keys {
            let s = &self.subs[k];
            images.push((self.image_of(s.page).ok_or_else(|| format!("screen {k:?} has no image yet"))?, s.w, s.h));
        }
        let labelled: Vec<_> = images.iter().enumerate().map(|(k, (i, w, h))| (i.clone(), *w, *h, if k == 0 { ENCODER_COPY } else { SCREEN_COPY })).collect();
        let mut got = self.read(&labelled)?.into_iter();
        self.layers();
        let page = got.next().ok_or("no page frame")?;
        let subs = keys.into_iter().zip(got).map(|(k, px)| (k.clone(), self.subs[&k].w, self.subs[&k].h, px)).collect();
        Ok((page, subs))
    }

}

/// Pages' first frames (a scripted page's once reftest-wait is gone) as PNGs on the Blitz page (`raea shots --page
/// blitz`, bench/wpt's reftests): stdin lines `PAGE\tOUT.png`, each page parsed by Blitz at `size` (root-absolute URLs
/// under `root`), drawn over white as a browser's canvas is; one `ok OUT` or `err OUT: why` line on stdout each, as
/// raea-render's `shots` (src/html.rs).
pub fn shots(size: (u32, u32), root: Option<&Path>, fonts: Option<&Path>) -> R<()> {
    use std::io::{BufRead, Write};
    let mut page = BlitzPage::new_on(size.0, size.1, fonts, None)?;
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some((file, out)) = line.split_once('\t') else { continue };
        let mut shot = || -> R<()> {
            // a page with scripts (WPT's that change the page after load, under reftest-wait): run by @raea/ui's
            // HTML runtime on a page of its own (crate::html), its shot taken once reftest-wait is gone, as the
            // bevy_ui page's shots did (css-transforms 542 of Chrome's 755 against 527 static); RAEA_SHOTS_SCRIPTS=0
            // draws every page static
            let scripted = std::env::var_os("RAEA_SHOTS_SCRIPTS").is_none_or(|v| v != "0")
                && std::fs::read_to_string(file).is_ok_and(|s| s.to_ascii_lowercase().contains("<script"));
            let mut px = if scripted {
                let mut p = crate::html::HtmlPage::open_with(Path::new(file), 30.0, fonts, Some(size), root)?;
                p.settle_reftest(600);
                p.frame()?
            } else {
                page.open_html(Path::new(file), root)?;
                page.draw();
                page.frame()?
            };
            // premultiplied over white
            for c in px.chunks_exact_mut(4) {
                let under = 255 - c[3];
                for v in &mut c[..3] {
                    *v = v.saturating_add(under);
                }
                c[3] = 255;
            }
            image::save_buffer(out, &px, size.0, size.1, image::ColorType::Rgba8).map_err(|e| format!("{out}: {e}"))
        };
        let said = match shot() {
            Ok(()) => format!("ok {out}"),
            Err(e) => format!("err {out}: {}", e.replace('\n', " ")),
        };
        let mut o = stdout.lock();
        writeln!(o, "{said}").and_then(|_| o.flush()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
