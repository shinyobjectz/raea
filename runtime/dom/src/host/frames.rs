//! Frames on a page: each `<iframe>`'s own document and JS, posts between them, fetches and asks.

use super::*;

/// A scripted <iframe>'s root (RAEA_PAGE_IFRAMES, docs/blitz-bevy.feature B5c, iframes 3-6): its document is the
/// iframe's sub-document, which stays under the parent's node (resolved, painted and hit tested there) and is swapped
/// into Docs under the root's own entity only while the root runs. Its JS is a context of its own, sharing nothing with
/// the parent's: they talk by postMessage, routed here a step later, as a browser runs it in a task.
#[derive(Clone, Copy)]
pub struct Frame {
    pub(crate) parent: Entity,
    pub(crate) node: NodeId,
    /// the iframe's wire id in the parent (its contentWindow's key there)
    pub(crate) wid: u64,
    /// sandboxed without allow-same-origin: an opaque origin, and no host calls or app asks at all
    pub(crate) opaque: bool,
    /// the document it was mounted for (`url_key`): its iframe's id can come back as another iframe's
    pub(crate) url: u64,
}

/// A frame document's URL as a frame root keeps it.
fn url_key(url: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    h.finish()
}

/// The JS of every root while iframes are on (raea_sys_page::iframes()): its iframes' windows (contentWindow) and the messages they post it.
pub(crate) const FRAMES_JS: &str = include_str!("../frames.js");

/// Messages for windows, delivered at their recipient's next step.
#[derive(Resource, Default)]
pub(crate) struct FramePosts(pub(crate) Vec<Post>);

pub(crate) enum Post {
    /// a frame's to its parent's window: the parent's page, the frame's wire id there, the data (JSON), its origin
    ToParent { parent: Entity, wid: u64, data: String, origin: String },
    /// a parent's to a frame's window: the parent's page, the iframe's wire id, the data, the target origin it named
    ToFrame { parent: Entity, wid: u64, data: String, target: String },
}

/// The frame root `page`'s document swapped with its iframe's: in Docs while it runs, under the iframe otherwise.
pub(crate) fn swap_frame(world: &mut World, page: Entity, f: &Frame) -> bool {
    let mut docs = world.non_send_mut::<Docs>();
    let Some(mut held) = docs.0.remove(&page) else { return false };
    let ok = docs.0.get_mut(&f.parent).and_then(|p| p.get_node_mut(f.node)).and_then(|n| n.subdoc_mut()).map(|d| std::mem::swap(&mut *d.inner_mut(), &mut held)).is_some();
    docs.0.insert(page, held);
    ok
}

/// The page's net.fetch is registered (mount).
#[derive(Resource)]
pub(crate) struct PageFetch;

/// The roots, in the order they mounted (the JS engines aren't Send: a non-send resource).
#[derive(Default)]
pub struct Roots(pub Vec<Root>);

impl raea_runtime_twins::bridge::OpIds for Roots {
    fn op_ids(&self, page: Entity) -> Option<&HashMap<u64, NodeId>> {
        self.0.iter().find(|r| r.page == page).map(|r| &r.nodes.by_id)
    }
}

impl Roots {
    pub fn get_mut(&mut self, page: Entity) -> Option<&mut Root> {
        self.0.iter_mut().find(|r| r.page == page)
    }
}

/// What a root emitted that the host doesn't act on itself (send, stop, a chat's asks...), in order, for the app.
#[derive(Resource, Default)]
pub struct PageAsks(pub Vec<(Entity, String, Value)>);

/// A root mounted on a new page `w` x `h` (its document opened, empty): the prelude, the viewport, `preamble` and
/// `code` run, its first render applied. Returns the page's entity.
pub fn mount(world: &mut World, w: u32, h: u32, code: &str, preamble: &[String], fonts: Option<blitz_dom::FontContext>) -> R<Entity> {
    world.init_resource::<HostCalls>();
    world.init_resource::<SpaceActions>();
    world.init_resource::<PageAsks>();
    // a page's fetch of its own origin is a file under its roots (net.rs page_fetch), or its host's (RAEA_PAGE_HOST: host.rs); the rest goes out as raea-runtime-js's
    if !world.contains_resource::<PageFetch>() {
        world.insert_resource(PageFetch);
        world.resource_mut::<HostCalls>().on("net.fetch", |_w, args, answer| {
            std::thread::spawn(move || match raea_sys_page::net::fetch(&args).map(Ok).unwrap_or_else(|| raea_io_net::http::fetch(&args)) {
                Ok(v) => answer.ok(v),
                Err(e) => answer.err(e),
            });
        });
        raea_runtime_js::calls::on_sync("canvas.read", raea_sys_canvas::read);
        raea_runtime_js::calls::on_sync("canvas.advances", |a| raea_sys_canvas::with_page_fonts(raea_sys_canvas::advances, a));
        raea_sys_canvas::fonts(blitz_dom::util::svg_fonts);
        raea_sys_canvas::videos(|src| Some(raea_sys_page::net::page_path(src.strip_prefix("http://localhost").unwrap_or(src)).into()));
    }
    if world.get_non_send::<Roots>().is_none() {
        world.insert_non_send(Roots::default());
    }
    let (page, nodes) = raea_sys_page::open_doc(world, w, h, fonts);
    let root = root_on(world, page, nodes, w, h, preamble, code, None)?;
    world.non_send_mut::<Roots>().0.push(root);
    Ok(page)
}

/// A root on `page`'s open document (in Docs): its JS started, the prelude, the viewport, `preamble` and `code` run,
/// its first render applied.
#[allow(clippy::too_many_arguments)]
pub(crate) fn root_on(world: &mut World, page: Entity, nodes: Nodes, w: u32, h: u32, preamble: &[String], code: &str, frame: Option<Frame>) -> R<Root> {
    let out: Shared = Default::default();
    let js = raea_runtime_js::js::start(raea_runtime_js::js::Engine::default(), out.clone())?;
    let mut scripts: Vec<(String, String)> = vec![
        (raea_runtime_js::js::PRELUDE.to_string(), "prelude.js".into()),
        (format!("globalThis.__ui_viewport = {{ w: {w}, h: {h}, dpr: 1 }}; globalThis.__ui_fonts = true;"), "viewport.js".into()),
    ];
    if raea_sys_page::iframes() {
        scripts.push((FRAMES_JS.to_string(), "frames.js".into()));
    }
    scripts.extend(preamble.iter().enumerate().map(|(i, s)| (s.clone(), format!("preamble{i}.js"))));
    scripts.push((code.to_string(), "bundle.js".into()));
    let mut root = Root { page, js, out, nodes, queue: Vec::new(), ticking: false, clicks: 1, capture: None, hover_frozen: None, medias: Default::default(), views: Default::default(), fonts_wait: None, bevy_ui: false, placed_videos: false, frame, frames_seen: Default::default(), synced: false, upkeep: raea_runtime_js::js::Upkeep::from_env() };
    for (src, name) in &scripts {
        root.js(world, |js| js.eval(src, name)).map_err(|e| format!("{name}: {e}"))?;
    }
    // the first render is flushed as the bundle runs; a mount left to a timer commits within these
    for _ in 0..16 {
        root.js(world, timers);
        let had = !root.out.borrow().flushed.is_empty();
        drain(world, &mut root);
        if had {
            break;
        }
    }
    Ok(root)
}

/// Each loaded <iframe> document under a top root's page whose frame may run scripts (no sandbox, or one with
/// allow-scripts) and names a <script src> inside the page's roots (FrameNet): a frame root mounted on it.
pub(crate) fn mount_frames(world: &mut World, roots: &mut Roots) {
    let mut found = vec![];
    {
        let docs = world.non_send::<Docs>();
        for r in roots.0.iter_mut().filter(|r| r.frame.is_none()) {
            let Some(doc) = docs.0.get(&r.page) else { continue };
            let ids = doc.sub_document_node_ids();
            // an iframe gone (its id may come back as a new one's)
            r.frames_seen.retain(|(n, _)| ids.contains(n));
            for n in ids {
                let Some(node) = doc.get_node(n) else { continue };
                // each loaded document once (a static frame's, or a script outside the roots, isn't looked at again)
                let Some(url) = node.subdoc().map(|d| d.inner().url().to_string()) else { continue };
                if !r.frames_seen.insert((n, url)) {
                    continue;
                }
                let Some(el) = node.element_data() else { continue };
                let sandbox = el.attr(blitz_dom::local_name!("sandbox")).map(|v| v.split_ascii_whitespace().map(str::to_ascii_lowercase).collect::<Vec<_>>());
                let may = |t: &str| sandbox.as_ref().is_none_or(|s| s.iter().any(|x| x == t));
                if !may("allow-scripts") {
                    continue;
                }
                let Some(wid) = r.nodes.id_of(n) else { continue };
                let Some(sub) = node.subdoc() else { continue };
                let sub = sub.inner();
                let Some(src) = sub.query_selector("script[src]").ok().flatten().and_then(|s| sub.get_node(s)?.attr(blitz_dom::local_name!("src")).map(str::to_string)) else { continue };
                let Some(body) = sub.query_selector("body").ok().flatten() else { continue };
                let url = sub.url().to_string();
                let size = node.final_layout().content_box_size();
                found.push((r.page, n, wid, !may("allow-same-origin"), src, body, url, (size.width.max(1.0) as u32, size.height.max(1.0) as u32)));
            }
        }
    }
    // a frame root whose iframe is gone, or whose iframe's id now holds another document, goes with its JS, its
    // document and its entity: a page that opened and closed artifacts kept every one's engine running, and a stale
    // root swapped its document into the next iframe given the same id
    let alive: std::collections::HashSet<(Entity, NodeId, u64)> = roots.0.iter().filter(|r| r.frame.is_none()).flat_map(|r| r.frames_seen.iter().map(move |(n, u)| (r.page, *n, url_key(u)))).collect();
    let stale: Vec<Entity> = roots.0.iter().filter(|r| r.frame.is_some_and(|f| !alive.contains(&(f.parent, f.node, f.url)))).map(|r| r.page).collect();
    if !stale.is_empty() {
        for i in (0..roots.0.len()).rev() {
            if stale.contains(&roots.0[i].page) {
                roots.0.remove(i);
            }
        }
        let mut docs = world.non_send_mut::<Docs>();
        for page in &stale {
            docs.0.remove(page);
        }
        for page in stale {
            if let Ok(e) = world.get_entity_mut(page) {
                e.despawn();
            }
        }
    }
    for (parent, node, wid, opaque, src, body, url, (w, h)) in found {
        let path = url::Url::parse(&url).and_then(|u| u.join(&src)).ok().and_then(|u| u.to_file_path().ok());
        let net = raea_sys_page::net::FrameNet { roots: raea_sys_page::net::page_roots() };
        let Some(code) = path.as_deref().and_then(|p| net.confined(p)).and_then(|p| std::fs::read_to_string(p).ok()) else {
            eprintln!("raea-page: a frame's script {src} isn't a file inside the page's root");
            continue;
        };
        // its URL as the frame's location (the page's http://localhost/, as @raea/ui's is), its origin opaque or not
        let at = url::Url::parse(&url).ok().map_or_else(|| "http://localhost/".to_string(), |u| format!("http://localhost{}{}", u.path(), u.query().map(|q| format!("?{q}")).unwrap_or_default()));
        let preamble = format!("globalThis.__raea_frame = {{ url: {}, opaque: {opaque} }};\n{}", json!(at), include_str!("../frame.js"));
        let page = world.spawn_empty().id();
        let held = BaseDocument::new(raea_sys_page::config(raea_sys_page::Page { size: UVec2::new(w, h), scale: 1.0 }));
        world.non_send_mut::<Docs>().0.insert(page, held);
        let frame = Frame { parent, node, wid, opaque, url: url_key(&url) };
        if !swap_frame(world, page, &frame) {
            continue;
        }
        let root = root_on(world, page, Nodes::new(body), w, h, &[preamble], &code, Some(frame));
        if let Ok(r) = &root {
            swap_frame(world, page, r.frame.as_ref().unwrap());
        }
        match root {
            Ok(r) => roots.0.push(r),
            Err(e) => eprintln!("raea-page: a frame's script {src}: {e}"),
        }
    }
}
