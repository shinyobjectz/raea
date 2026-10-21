//! Rendering .row without JavaScript (docs/row.feature): the rows become a Blitz document once, one
//! element per row, and each frame (a run of src/world.rs's schedule) sets every visible row's inline
//! style and text from its resolved props, then paints. Native kinds are styled here; a component imported from
//! .tsx is a labelled placeholder until React nodes are wired in.
//!
//!   raea row rows  FILE.row              the rows as JSON
//!   raea row print FILE.row              canonical .row (plain Odin, one declaration per element)
//!   raea row render FILE.row -o DIR [--at F1,F2,...] [--every N] [--scale S]
//!       PNG frames (all frames, every Nth, or the given frame numbers); -o OUT.mp4 (or .mov, .webm) encodes
//!       every frame with ffmpeg

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{BaseDocument, DocumentConfig, LocalName, NodeId, QualName, ns};
use blitz_traits::shell::{ColorScheme, Viewport};
use peniko::kurbo::Rect;
use peniko::{Color, Fill};
use serde_json::Value;

use crate::net::SyncNet;
#[cfg(test)]
use crate::timeline::Edit;
use crate::timeline::Timeline;

/// Default look of each native kind; per-frame props are inline styles on top.
const KIND_CSS: &str = r#"
html, body { margin: 0; overflow: hidden; background: transparent; }
body { font-family: "Helvetica Neue", Helvetica, Arial, sans-serif; color: #fff; }
.row-root { position: absolute; left: 0; top: 0; overflow: hidden; }
.row-stack { position: absolute; left: 0; top: 0; right: 0; bottom: 0; display: flex; flex-direction: column;
  align-items: center; justify-content: center; }
.row-box { position: absolute; }
.row-title { position: absolute; left: 0; right: 0; text-align: center; font-size: 96px; font-weight: 800; line-height: 1.05; }
.row-text { position: absolute; left: 0; right: 0; text-align: center; font-size: 48px; line-height: 1.2; }
.row-captions { position: absolute; left: 60px; right: 60px; bottom: 420px; text-align: center; font-size: 64px;
  font-weight: 800; line-height: 1.1; }
.row-image { position: absolute; object-fit: cover; }
/* a text row's text children are runs inside its line (a caption's words, one lit while it's said) */
.row-text > .row-text, .row-title > .row-text { position: static; display: inline; font-size: inherit; font-weight: inherit;
  line-height: inherit; text-align: inherit; }
/* inside a stack, children flow (flexbox) instead of sitting at absolute positions */
.row-stack > * { position: relative; left: auto; right: auto; top: auto; bottom: auto; }
.row-component { position: absolute; left: 60px; right: 60px; padding: 40px; border-radius: 32px;
  background: #ffffff; color: #111; font-size: 48px; font-weight: 700; text-align: center; }
"#;

pub(crate) fn qn(local: &str) -> QualName {
    QualName { prefix: None, ns: ns!(html), local: LocalName::from(local) }
}

/// Where an image row's picture waits while the row is hidden.
const PICTURE: &str = "data-raea-src";

/// An image row's `src` for its visibility: set when it appears, cleared when it goes (clearing unloads the picture,
/// vendor/blitz-dom's mutator), so an edit of many one-frame stills holds only the ones showing (450 page stills had
/// held 3.7 GB, which the iPhone killed). `Some(Some(src))` to set, `Some(None)` to clear, None when it's as it should be.
pub(crate) fn image_src(doc: &BaseDocument, el: NodeId, on: bool) -> Option<Option<String>> {
    let n = doc.get_node(el)?;
    let want = n.attr(LocalName::from(PICTURE))?;
    match (on, n.attr(LocalName::from("src")).is_some()) {
        (true, false) => Some(Some(want.to_string())),
        (false, true) => Some(None),
        _ => None,
    }
}

fn px(v: &Value) -> Option<String> {
    v.as_f64().map(|n| format!("{}px", (n * 1000.0).round() / 1000.0))
}

/// Inline CSS for a node's resolved props (numbers are CSS px; unknown props are left to the kind).
/// A flag column: an edit can hand `italic: 1` over as a number (1.0), which as_bool refused
fn flag(v: &Value) -> Option<bool> {
    v.as_bool().or_else(|| v.as_f64().map(|n| n != 0.0))
}

pub(crate) fn style_for(props: &serde_json::Map<String, Value>) -> String {
    let mut css = Vec::new();
    let mut transform = Vec::new();
    for (k, v) in props {
        let s = v.as_str().map(str::to_string);
        let line = match k.as_str() {
            "x" => px(v).map(|p| format!("left:{p}")),
            "y" => px(v).map(|p| format!("top:{p}")),
            "w" => px(v).map(|p| format!("width:{p}")),
            "h" => px(v).map(|p| format!("height:{p}")),
            "opacity" => v.as_f64().map(|o| format!("opacity:{}", o.clamp(0.0, 1.0))),
            "color" => s.map(|c| format!("color:{c}")),
            "bg" => s.map(|c| format!("background:{c}")),
            "radius" => px(v).map(|p| format!("border-radius:{p}")),
            "size" => px(v).map(|p| format!("font-size:{p}")),
            "weight" => v.as_f64().map(|w| format!("font-weight:{w}")).or(s.map(|w| format!("font-weight:{w}"))),
            "font" => s.map(|f| format!("font-family:{f}")),
            "align" => s.map(|a| format!("text-align:{a}")),
            "padding" => px(v).map(|p| format!("padding:{p}")),
            "gap" => px(v).map(|p| format!("gap:{p}")),
            "justify" => s.map(|j| format!("justify-content:{j}")),
            // layout: a stack's direction, cross-axis alignment and wrapping (flexbox, through Taffy)
            "dir" => s.map(|d| format!("flex-direction:{d}")),
            "items" => s.map(|a| format!("align-items:{a}")),
            "wrap" => flag(v).map(|w| format!("flex-wrap:{}", if w { "wrap" } else { "nowrap" })),
            "grow" => v.as_f64().map(|g| format!("flex-grow:{g}")),
            // type
            // a multiple of the size (1.2); above 4 it can only mean pixels (`line: 86` for 86 px,
            // which as a multiple pushed the box past the frame and its text out of view)
            "line" => v.as_f64().map(|l| if l > 4.0 { format!("line-height:{l}px") } else { format!("line-height:{l}") })
                .or(s.map(|l| format!("line-height:{l}"))),
            "tracking" => px(v).map(|p| format!("letter-spacing:{p}")),
            "case" => s.map(|c| format!("text-transform:{c}")),
            "italic" => flag(v).filter(|i| *i).map(|_| "font-style:italic".to_string()),
            "stroke" => s.map(|c| format!("-webkit-text-stroke:{c}")),
            "text_shadow" => s.map(|c| format!("text-shadow:{c}")),
            // surface
            "border" => s.map(|b| format!("border:{b}")),
            "shadow" => s.map(|b| format!("box-shadow:{b}")),
            "blur" => px(v).map(|p| format!("filter:blur({p})")),
            "backdrop" => px(v).map(|p| format!("backdrop-filter:blur({p})")),
            "fit" => s.map(|f| format!("object-fit:{f}")),
            "overflow" => s.map(|o| format!("overflow:{o}")),
            // rdb hands numbers over as floats (-1.0), which as_i64 refused: z was dropped and draw order stayed the
            // order rows were written (void's deck eval)
            "z" => v.as_f64().map(|z| format!("z-index:{}", z.round() as i64)),
            "rotate" => {
                if let Some(d) = v.as_f64() {
                    transform.push(format!("rotate({d}deg)"));
                }
                None
            }
            "scale" => {
                if let Some(s) = v.as_f64() {
                    transform.push(format!("scale({s})"));
                }
                None
            }
            _ => None,
        };
        css.extend(line);
    }
    // `align: center` with an x and no width centres the row on x (`right` puts its right edge there), as every agent
    // reads `x: 960, align: center`; with a width, the text aligns inside the box as before (void's eval, 2026-10-15)
    if props.contains_key("x") && !props.contains_key("w") {
        match props.get("align").and_then(|a| a.as_str()) {
            Some("center") => transform.insert(0, "translateX(-50%)".into()),
            Some("right") => transform.insert(0, "translateX(-100%)".into()),
            _ => {}
        }
    }
    // a row with a background and no width is a pill: it hugs its text instead of stretching to the frame's right
    // edge (text and title rows carry left:0;right:0), centred when it has no x (an agent session's ceiling test)
    // a word run inside a caption or title is inline, and Blitz paints no padding or radius on an inline box: a
    // highlighted word was a flat band. inline-block keeps it in the line with its pill (top-level rows are absolute
    // and flex items are blockified, so it changes nothing for them)
    if props.contains_key("bg") {
        css.push("display:inline-block".into());
    }
    if props.contains_key("bg") && !props.contains_key("w") {
        if props.contains_key("x") {
            css.push("right:auto".into());
        } else {
            css.push("width:fit-content;margin-left:auto;margin-right:auto".into());
        }
    }
    // `shadow` on a row with nothing to cast it (no bg, no border) is meant for the text: as a box-shadow it drew a
    // band across a full-width title
    if !props.contains_key("bg") && !props.contains_key("border") {
        if let Some(i) = css.iter().position(|l| l.starts_with("box-shadow:")) {
            css[i] = css[i].replacen("box-shadow:", "text-shadow:", 1);
        }
    }
    if !transform.is_empty() {
        css.push(format!("transform:{}", transform.join(" ")));
    }
    css.join(";")
}

/// The slice-1 renderer, kept as the reference the World's frames are checked against (src/world.rs tests).
#[cfg(test)]
pub struct RowRenderer {
    timeline: Timeline,
    doc: BaseDocument,
    /// row id -> (element, text node)
    nodes: HashMap<String, (NodeId, Option<NodeId>)>,
    imports: HashMap<String, String>,
    width: u32,
    height: u32,
    scale: f32,
}

#[cfg(test)]
impl RowRenderer {
    /// `edit` is the rows (from row::parse, plus any `data` columns); assets resolve against `dir`.
    pub fn new(edit: &Value, dir: &Path, scale: f32) -> Result<RowRenderer, String> {
        let timeline = Timeline::new(Edit::from_value(edit)?);
        let (width, height) = (timeline.edit().width, timeline.edit().height);
        let imports = imports_of(edit);
        let (doc, nodes) = build_document(&timeline, &imports, dir, scale);
        Ok(RowRenderer { timeline, doc, nodes, imports, width, height, scale })
    }

    pub fn frames(&self) -> i64 {
        self.timeline.edit().frames
    }

    pub fn pixel_size(&self) -> (u32, u32) {
        ((self.width as f32 * self.scale).round() as u32, (self.height as f32 * self.scale).round() as u32)
    }

    /// Frame `f`: premultiplied RGBA8.
    pub fn render(&mut self, f: i64, out: &mut Vec<u8>) {
        let snap = self.timeline.frame(f);
        let visible: HashMap<&str, &Value> =
            snap["nodes"].as_array().map(|a| a.iter().map(|n| (n["id"].as_str().unwrap_or_default(), n)).collect()).unwrap_or_default();
        {
            let pictures: Vec<(NodeId, Option<String>)> = self.timeline.edit().instances.iter()
                .filter_map(|inst| self.nodes.get(&inst.id).and_then(|(el, _)| image_src(&self.doc, *el, visible.contains_key(inst.id.as_str())).map(|c| (*el, c))))
                .collect();
            let mut m = self.doc.mutate();
            for (el, c) in pictures {
                match c {
                    Some(src) => m.set_attribute(el, qn("src"), &src),
                    None => m.clear_attribute(el, qn("src")),
                }
            }
            for inst in &self.timeline.edit().instances {
                let Some((el, text)) = self.nodes.get(&inst.id) else { continue };
                match visible.get(inst.id.as_str()) {
                    None => m.set_attribute(*el, qn("style"), "display:none"),
                    Some(n) => {
                        let empty = serde_json::Map::new();
                        let props = n["props"].as_object().unwrap_or(&empty);
                        m.set_attribute(*el, qn("style"), &style_for(props));
                        if let Some(t) = text {
                            m.set_node_text(*t, &node_label(&inst.kind, &self.imports, props));
                        }
                    }
                }
            }
        }
        let (w, h) = self.pixel_size();
        self.doc.resolve(f as f64 / self.timeline.edit().fps_f64());
        paint_document(&mut self.doc, w, h, self.scale, true, out);
    }
}

pub(crate) fn imports_of(edit: &Value) -> HashMap<String, String> {
    edit["imports"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect()).unwrap_or_default()
}

/// The Blitz document for an edit: one element per row (parents before children), all hidden until a
/// frame shows them. Returns the document and row id -> (element, text node).
pub(crate) fn build_document(
    timeline: &Timeline,
    imports: &HashMap<String, String>,
    dir: &Path,
    scale: f32,
) -> (BaseDocument, HashMap<String, (NodeId, Option<NodeId>)>) {
    let (width, height) = (timeline.edit().width, timeline.edit().height);
    let net = Arc::new(SyncNet::new(dir, false));
    let config = DocumentConfig {
        viewport: Some(Viewport::new((width as f32 * scale) as u32, (height as f32 * scale) as u32, scale, ColorScheme::Light)),
        ua_stylesheets: Some(vec![blitz_dom::DEFAULT_CSS.to_string(), KIND_CSS.to_string()]),
        net_provider: Some(net.clone() as _),
        base_url: url::Url::from_directory_path(dir).ok().map(|u| u.to_string()),
        ..Default::default()
    };
    let mut doc = BaseDocument::new(config);
    let mut nodes = HashMap::new();
    {
        let mut m = doc.mutate();
        let html = m.create_element(qn("html"), vec![]);
        let doc_root = m.doc.root_node().id;
        m.append_children(doc_root, &[html]);
        // fonts::inject puts its @font-face rules in the head: without one the bundled faces were dropped silently
        let head = m.create_element(qn("head"), vec![]);
        m.append_children(html, &[head]);
        let body = m.create_element(qn("body"), vec![]);
        m.append_children(html, &[body]);
        let root = m.create_element(
            qn("div"),
            vec![
                blitz_dom::Attribute { name: qn("class"), value: "row-root".into() },
                blitz_dom::Attribute { name: qn("style"), value: format!("width:{width}px;height:{height}px") },
            ],
        );
        m.append_children(body, &[root]);
        // parents before children: the edit's rows are already in that order when they come from .row
        for inst in &timeline.edit().instances {
            let component = imports.contains_key(&inst.kind) || inst.kind.starts_with(|c: char| c.is_uppercase());
            let tag = if inst.kind == "image" { "img" } else { "div" };
            let class = if component { "row-component".to_string() } else { format!("row-{}", inst.kind) };
            let mut attrs = vec![
                blitz_dom::Attribute { name: qn("class"), value: class },
                blitz_dom::Attribute { name: qn("data-id"), value: inst.id.clone() },
                blitz_dom::Attribute { name: qn("style"), value: "display:none".into() },
            ];
            if inst.kind == "image" {
                if let Some(crate::timeline::Prop::Const(Value::Object(src))) = inst.props.get("src") {
                    if let Some(a) = src.get("asset").and_then(Value::as_str) {
                        // `src` is set while the row shows (see `image_src`), so only visible pictures are held
                        attrs.push(blitz_dom::Attribute { name: qn(PICTURE), value: a.to_string() });
                    }
                }
            }
            let el = m.create_element(qn(tag), attrs);
            let parent = inst.parent.as_ref().and_then(|p| nodes.get(p).map(|(e, _): &(NodeId, Option<NodeId>)| *e)).unwrap_or(root);
            m.append_children(parent, &[el]);
            let text = if tag == "div" {
                let t = m.create_text_node("");
                m.append_children(el, &[t]);
                Some(t)
            } else {
                None
            };
            nodes.insert(inst.id.clone(), (el, text));
        }
        // the font families the rows name, in a style element, so the bundled and Google faces load for them as they
        // do for an HTML page (fonts::inject reads style elements; a row's own style is written frame by frame, after
        // this): without it Inter, Montserrat, Poppins and the rest fell back to Helvetica (an agent session's ceiling test)
        let mut families: Vec<String> = Vec::new();
        for inst in &timeline.edit().instances {
            if let Some(crate::timeline::Prop::Const(Value::String(f))) = inst.props.get("font") {
                if !families.contains(f) && !crate::fonts::is_stand_in(f) {
                    families.push(f.clone());
                }
            }
        }
        if !families.is_empty() {
            let css: String = families.iter().enumerate().map(|(i, f)| format!(".row-font-{i}{{font-family:{f}}}\n")).collect();
            let style = m.create_element(qn("style"), vec![]);
            let t = m.create_text_node(&css);
            m.append_children(style, &[t]);
            m.append_children(head, &[style]);
        }
    }
    let _ = crate::fonts::inject(&mut doc, &net);
    (doc, nodes)
}

/// The text a row shows: captions show the words bound to them (as of this frame), a .tsx component its title
/// (a placeholder until React nodes are wired in), anything else its `text`.
pub(crate) fn node_label(kind: &str, imports: &HashMap<String, String>, props: &serde_json::Map<String, Value>) -> String {
    let label = match (kind, imports.get(kind)) {
        ("captions", _) => props.get("words").map(|w| match w {
            Value::String(s) => s.clone(),
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
            _ => String::new(),
        }),
        (_, Some(src)) => Some(props.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("<{kind} from {src}>"))),
        _ => props.get("text").and_then(Value::as_str).map(str::to_string),
    };
    label.unwrap_or_default()
}

/// Paint a resolved document with Vello (CPU): premultiplied RGBA8 into `out`.
/// `clear` fills black first (a video frame); without it the background stays transparent.
/// A document's paint, started: on the CPU it's done already; on the GPU it's a ticket to collect, so the caller can
/// go on (the next frame's React and layout) while the GPU works.
pub enum Painting {
    Done(Vec<u8>),
    Gpu(crate::quads::Ticket),
}

impl Painting {
    /// The pixels (premultiplied RGBA8).
    pub fn finish(self) -> Result<Vec<u8>, String> {
        match self {
            Painting::Done(px) => Ok(px),
            Painting::Gpu(t) => {
                let mut px = Vec::new();
                t.collect(&mut px)?;
                Ok(px)
            }
        }
    }
}

/// `paint_document` without waiting for the GPU.
pub fn submit_document(doc: &mut BaseDocument, w: u32, h: u32, scale: f32, clear: bool) -> Painting {
    if crate::quads::enabled() {
        if let Ok(t) = crate::quads::submit(doc, w, h, scale as f64, clear) {
            return Painting::Gpu(t);
        }
    }
    let mut px = Vec::new();
    paint_cpu(doc, w, h, scale, clear, &mut px);
    Painting::Done(px)
}

pub fn paint_document(doc: &mut BaseDocument, w: u32, h: u32, scale: f32, clear: bool, out: &mut Vec<u8>) {
    // on the GPU (src/quads) unless RAEA_PAINT=cpu or the frame needs the CPU
    if crate::quads::enabled() && crate::quads::paint(doc, w, h, scale as f64, clear, out).is_ok() {
        return;
    }
    paint_cpu(doc, w, h, scale, clear, out);
}

pub(crate) fn paint_cpu(doc: &mut BaseDocument, w: u32, h: u32, scale: f32, clear: bool, out: &mut Vec<u8>) {
    let mut r = VelloCpuImageRenderer::new(w, h);
    out.clear();
    let scale = scale as f64;
    r.render_to_vec(
        |scene| {
            if clear {
                scene.fill(Fill::NonZero, Default::default(), Color::BLACK, Default::default(), &Rect::new(0.0, 0.0, w as f64, h as f64));
            }
            blitz_paint::paint_scene(scene, doc, scale, w, h, 0, 0);
        },
        out,
    );
}

fn load(path: &str) -> Result<(Value, PathBuf), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let rows = crate::row::parse(&src).map_err(|e| format!("{path}:{e}"))?;
    let dir = Path::new(path).parent().map(Path::to_path_buf).filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."));
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    Ok((rows, dir))
}

/// `raea row ...` (see the module docs). Returns the exit code.
pub fn cli(args: &[String], write_png: &dyn Fn(&Path, u32, u32, &[u8]) -> Result<(), String>) -> Result<i32, String> {
    let usage = "usage: raea row rows|print FILE.row | raea row render FILE.row -o DIR|OUT.mp4|OUT.mov|OUT.webm [--at F,F] [--every N] [--scale S] [--data FILE.json]";
    let (cmd, file) = match args {
        [cmd, file, ..] => (cmd.as_str(), file.as_str()),
        _ => return Err(usage.into()),
    };
    let (mut rows, dir) = load(file)?;
    match cmd {
        "rows" => println!("{}", serde_json::to_string_pretty(&rows).unwrap()),
        "print" => print!("{}", crate::row::print(&rows)),
        "render" => {
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
            let out = PathBuf::from(flag("-o").ok_or(usage)?);
            if let Some(data) = flag("--data") {
                let text = std::fs::read_to_string(&data).map_err(|e| format!("{data}: {e}"))?;
                rows["data"] = serde_json::from_str(&text).map_err(|e| format!("{data}: {e}"))?;
            }
            let scale: f32 = flag("--scale").map(|s| s.parse().map_err(|_| "bad --scale")).transpose()?.unwrap_or(1.0);
            let mut r = crate::world::RowWorld::new(&rows, &dir, scale)?;
            let frames: Vec<i64> = match (flag("--at"), flag("--every")) {
                (Some(at), _) => at.split(',').map(|f| f.trim().parse().map_err(|_| format!("bad frame {f:?}"))).collect::<Result<_, _>>()?,
                (None, every) => {
                    let n: i64 = every.map(|e| e.parse().map_err(|_| "bad --every")).transpose()?.unwrap_or(1);
                    (0..r.frames()).step_by(n.max(1) as usize).collect()
                }
            };
            let (w, h) = r.pixel_size();
            let mut buf = Vec::new();
            let t0 = std::time::Instant::now();
            let video = out.extension().and_then(|e| e.to_str()).filter(|e| ["mp4", "mov", "webm"].contains(e)).map(str::to_string);
            if let Some(ext) = video {
                // a video: every frame in order, piped to ffmpeg as raw RGBA (opaque, so premultiplied = straight)
                let ffmpeg = crate::media::ffmpeg_path().ok_or("ffmpeg not found (PATH or $RAEA_FFMPEG) for a video; use -o DIR for PNG frames")?;
                let (n, d) = r.fps();
                let codec: &[&str] = match ext.as_str() {
                    "mov" => &["-c:v", "prores_ks", "-profile:v", "3", "-pix_fmt", "yuv422p10le"],
                    "webm" => &["-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0", "-pix_fmt", "yuv420p"],
                    _ => &["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"],
                };
                let mut child = std::process::Command::new(ffmpeg)
                    .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-framerate", &format!("{n}/{d}"), "-i", "-"])
                    .args(codec)
                    .arg(&out)
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("ffmpeg: {e}"))?;
                let mut stdin = child.stdin.take().unwrap();
                for f in 0..r.frames() {
                    r.render(f, &mut buf);
                    std::io::Write::write_all(&mut stdin, &buf).map_err(|e| format!("ffmpeg stdin: {e}"))?;
                }
                drop(stdin);
                let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
                if !status.success() {
                    return Err(format!("ffmpeg exited with {status}"));
                }
                eprintln!("{} ({} frames, {w}x{h}) in {:.2}s", out.display(), r.frames(), t0.elapsed().as_secs_f64());
                return Ok(0);
            }
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            for f in &frames {
                r.render(*f, &mut buf);
                write_png(&out.join(format!("frame-{f:05}.png")), w, h, &buf)?;
            }
            eprintln!("{} frames ({w}x{h}) in {:.2}s", frames.len(), t0.elapsed().as_secs_f64());
        }
        _ => return Err(usage.into()),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r##"
edit :: Edit{fps = 30, size = {320, 240}, frames = 30}
bg    :: box{w = 320, h = 240, bg = "#204060"}
dot   :: box{in = bg, x = {0 = 10, 20 = 250}, y = 100, w = 40, h = 40, bg = "#ffcc00", radius = 20}
label :: text{"hello row", in = bg, from = 10, y = 180, size = 20}
"##;

    fn pixel(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn rows_paint_without_javascript() {
        let rows = crate::row::parse(SRC).unwrap_or_else(|e| panic!("{e}"));
        let mut r = RowRenderer::new(&rows, Path::new("."), 1.0).unwrap();
        let (w, _) = r.pixel_size();
        let mut f0 = Vec::new();
        r.render(0, &mut f0);
        assert_eq!(pixel(&f0, w, 30, 120), [0xff, 0xcc, 0x00, 0xff], "the dot starts at x=10");
        assert_eq!(pixel(&f0, w, 270, 120), [0x20, 0x40, 0x60, 0xff], "and the background is behind it");
        let mut f20 = Vec::new();
        r.render(20, &mut f20);
        assert_eq!(pixel(&f20, w, 270, 120), [0xff, 0xcc, 0x00, 0xff], "it reaches x=250 at frame 20");
        // seek back: the same bytes as the first time
        let mut again = Vec::new();
        r.render(0, &mut again);
        assert_eq!(f0, again);
        // the label only exists from frame 10
        assert_ne!(f0, { let mut v = Vec::new(); r.render(15, &mut v); v });
    }
}
