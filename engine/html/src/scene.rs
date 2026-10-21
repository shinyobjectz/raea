// Portions ported from HyperFrames (Apache-2.0, https://github.com/heygen-com/hyperframes); see NOTICE.
//! One loaded composition: the Blitz document with its scripts run against the GSAP shim, the
//! native animation scene, the runtime timing, and the per-frame apply + paint.

use crate::compile::{self, CompileInfo};
use crate::gsap::{self, Builder, Drive, Node, Scene as AnimScene, Target};
use crate::net::SyncNet;
use crate::runtime::{Resolver, Timing};
use crate::value::{fmt_num, CssValue};
use anyrender::ImageRenderer;
#[cfg(feature = "skia")]
use anyrender_skia::SkiaImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{BaseDocument, Document, DocumentConfig, NodeId};
use blitz_html::{DocumentHtmlParser, HtmlProvider};
use blitz_traits::shell::{ColorScheme, Viewport};
use crate::script::{FetchError, ScriptDocument, ScriptFetcher};
use peniko::kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

const SHIM: &str = include_str!("gsap_shim.js");
/// SVG properties that are also presentation attributes (what usvg reads besides `style`).
const SVG_PRESENTATION: &[&str] = &[
    "fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset",
    "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-opacity", "opacity", "visibility", "display",
    "stop-color", "stop-opacity", "font-size", "font-family", "font-weight", "letter-spacing", "text-anchor",
];

/// Browsers' UA sheet gives SVG children `transform-origin: 0 0`; GSAP reads it.
const SVG_UA_CSS: &str = "svg * { transform-origin: 0 0; }";
/// keeps compile::mark_background_clip_text's marker from inheriting
const BGCLIP_UA_CSS: &str = "* { --uv-bgclip: initial; }";
/// compile::mark_query_containers: the container marker doesn't inherit
const CQ_UA_CSS: &str = "* { --uv-ct: initial; }";

/// An animated element inside an inline <svg>: GSAP drives it through attributes
/// (transform="matrix(...)", opacity, fill...) because that is all usvg reads.
#[derive(Clone, Debug)]
struct SvgTarget {
    root: NodeId,
    /// getBBox(): object bounding box in user units
    bbox: [f64; 4],
    /// computed transform-origin in user units (percentages resolved against the viewBox)
    css_origin: (f64, f64),
    /// attributes as authored (restored when nothing animates them)
    orig: HashMap<String, String>,
}

fn svg_bbox(doc: &BaseDocument, node: NodeId) -> Option<[f64; 4]> {
    let html = compile::outer_html(doc, node);
    // strip what would hide the shape or move it: getBBox ignores both
    let end = html.find('>')?;
    let mut tag = html[..end].to_string();
    for attr in ["transform", "opacity", "visibility", "display", "fill", "class", "style"] {
        while let Some(i) = tag.find(&format!(" {attr}=\"")) {
            let rest = &tag[i + attr.len() + 3..];
            let Some(j) = rest.find('"') else { break };
            tag.replace_range(i..i + attr.len() + 4 + j, "");
        }
    }
    let src = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><g fill=\"#000\">{}{}</g></svg>",
        tag,
        &html[end..]
    );
    let tree = usvg::Tree::from_str(&src, &usvg::Options::default()).ok()?;
    let b = tree.root().bounding_box();
    Some([b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64])
}

fn parse_numbers(s: &str) -> Vec<f64> {
    s.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|v| v.trim().parse().ok()).collect()
}

/// An SVG `transform` attribute as a 2D affine (matrix/translate/scale/rotate/skewX/skewY).
fn parse_svg_transform(s: &str) -> Affine {
    let mut m = Affine::IDENTITY;
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim().to_ascii_lowercase();
        let Some(close) = rest[open..].find(')') else { break };
        let args = parse_numbers(&rest[open + 1..open + close]);
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let t = match name.as_str() {
            "matrix" if args.len() >= 6 => Affine::new([a(0), a(1), a(2), a(3), a(4), a(5)]),
            "translate" => Affine::translate((a(0), a(1))),
            "scale" => Affine::scale_non_uniform(a(0), if args.len() > 1 { a(1) } else { a(0) }),
            "rotate" => {
                let r = Affine::rotate(a(0).to_radians());
                if args.len() >= 3 { Affine::translate((a(1), a(2))) * r * Affine::translate((-a(1), -a(2))) } else { r }
            }
            "skewx" => Affine::new([1.0, 0.0, a(0).to_radians().tan(), 1.0, 0.0, 0.0]),
            "skewy" => Affine::new([1.0, a(0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => Affine::IDENTITY,
        };
        m = m * t;
        rest = &rest[open + close + 1..];
    }
    m
}

/// GSAP's _renderSVGTransforms: one matrix with the origin baked in.
fn svg_matrix(s: &TransformState, origin: (f64, f64), bbox: [f64; 4]) -> String {
    let mut rotation = s.rotation as f64;
    let mut skew_x = s.skew_x as f64;
    let skew_y = s.skew_y as f64;
    let (sx, sy) = (s.scale_x as f64, s.scale_y as f64);
    if skew_y != 0.0 {
        skew_x += skew_y;
        rotation += skew_y;
    }
    let (mut a11, mut a21, mut a12, mut a22);
    if rotation != 0.0 || skew_x != 0.0 {
        let r = rotation.to_radians();
        let k = skew_x.to_radians();
        a11 = r.cos() * sx;
        a21 = r.sin() * sx;
        a12 = (r - k).sin() * -sy;
        a22 = (r - k).cos() * sy;
        if k != 0.0 {
            let ky = skew_y.to_radians();
            let t = (k - ky).tan();
            let t = (1.0 + t * t).sqrt();
            a12 *= t;
            a22 *= t;
            if ky != 0.0 {
                let t = ky.tan();
                let t = (1.0 + t * t).sqrt();
                a11 *= t;
                a21 *= t;
            }
        }
    } else {
        a11 = sx;
        a22 = sy;
        a21 = 0.0;
        a12 = 0.0;
    }
    let px = |v: &CssValue, size: f64| -> f64 {
        match v {
            CssValue::Num { v, unit } if unit == "%" => *v as f64 / 100.0 * size,
            other => other.as_f32().unwrap_or(0.0) as f64,
        }
    };
    let mut tx = px(&s.x, bbox[2]);
    let mut ty = px(&s.y, bbox[3]);
    let (xo, yo) = origin;
    tx += xo - (xo * a11 + yo * a12);
    ty += yo - (xo * a21 + yo * a22);
    tx += s.x_percent as f64 / 100.0 * bbox[2];
    ty += s.y_percent as f64 / 100.0 * bbox[3];
    let f = |v: f64| fmt_num(v as f32);
    format!("matrix({},{},{},{},{},{})", f(a11), f(a21), f(a12), f(a22), f(tx), f(ty))
}

/// GSAP transformOrigin for SVG: keywords / % relative to the bbox, px offsets from its corner.
fn svg_origin_from(value: &str, bbox: [f64; 4]) -> (f64, f64) {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let resolve = |p: Option<&&str>, size: f64, default: f64| -> f64 {
        match p.map(|s| s.to_ascii_lowercase()) {
            None => default,
            Some(s) => match s.as_str() {
                "left" | "top" => 0.0,
                "center" => size / 2.0,
                "right" | "bottom" => size,
                s if s.ends_with('%') => s.trim_end_matches('%').parse::<f64>().unwrap_or(0.0) / 100.0 * size,
                s => s.trim_end_matches("px").parse::<f64>().unwrap_or(0.0),
            },
        }
    };
    let x = resolve(parts.first(), bbox[2], bbox[2] / 2.0);
    let y = resolve(parts.get(1), bbox[3], if parts.len() == 1 { x.min(bbox[3]) } else { bbox[3] / 2.0 });
    (bbox[0] + x, bbox[1] + y)
}

/// How a page is read. `the HTML composition format` (the default) applies the composition format's conventions, matching its
/// Chrome renders: named and generic font families resolve to the deterministic bundled/Google faces
/// (Inter in front of system stacks). `Web` reads the page as a browser would, for plain HTML/React
/// layers embedded elsewhere (moonsplice html nodes): font stacks resolve as written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    #[default]
    Html,
    Web,
}

impl Profile {
    pub fn parse(s: &str) -> Result<Profile, String> {
        match s {
            "html" => Ok(Profile::Html),
            "web" => Ok(Profile::Web),
            other => Err(format!("unknown profile {other:?} (the composition format | web)")),
        }
    }
}

/// What a page's own clocks (Date, performance.now, timers, requestAnimationFrame) do while its frames are rendered.
/// `Settled` (the default, the composition format's model): the page runs its timers up to 5 s ahead while it loads, then its
/// clocks hold still and each frame seeks its timelines and CSS animations only. `Frames` (C0, `--clock frames` or
/// `<meta name="raea-clock" content="frames">`): a timer fires at the composition time it's due, and each frame
/// advances the page's clocks to its own time first, running every timer and requestAnimationFrame callback due by
/// then, so an app's own frame loop (motion/react, a game loop) runs with the frames. Time only moves forward in it,
/// so such a page's frames depend on the order they're rendered in, as an app's state does.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Clock {
    #[default]
    Settled,
    Frames,
}

impl Clock {
    pub fn parse(s: &str) -> Result<Clock, String> {
        match s {
            "settled" => Ok(Clock::Settled),
            "frames" => Ok(Clock::Frames),
            other => Err(format!("unknown clock {other:?} (settled | frames)")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub html_path: PathBuf,
    pub project_dir: PathBuf,
    pub fps: (u32, u32),
    pub scale: f32,
    /// CSS-pixel size override (defaults to the root's data-width/height, else 1920x1080)
    pub size: Option<(u32, u32)>,
    pub variables: Option<Value>,
    pub allow_network: bool,
    pub transparent: bool,
    /// video id -> window and frame source
    pub videos: Arc<crate::media::VideoFrames>,
    pub backend: Backend,
    pub profile: Profile,
    /// The page's HTML, when it doesn't come from `html_path` (an html node opened from a string);
    /// `html_path` then only anchors relative URLs and needn't exist
    pub html_source: Option<String>,
    pub clock: Clock,
}

impl Options {
    pub fn fps_f64(&self) -> f64 {
        self.fps.0 as f64 / self.fps.1.max(1) as f64
    }
}

/// The shim's native bindings (`__blitz_native(name, payload)`, vendored blitz-vibey-script): WebGL
/// constants, availability and synchronous queries, canvas text metrics, and SVG path geometry.
/// Families declared by the page's `@font-face` rules (lower-cased).
fn declared_web_fonts(inner: &blitz_dom::BaseDocument) -> std::collections::HashSet<String> {
    let mut declared = std::collections::HashSet::new();
    for id in compile::all_elements(inner) {
        if compile::tag(inner, id) != "style" {
            continue;
        }
        let css = inner.get_node(id).map(|n| n.text_content()).unwrap_or_default();
        for block in css.split("@font-face").skip(1) {
            let body = block.split('}').next().unwrap_or("");
            if let Some(v) = body.split(';').find_map(|d| d.split_once(':').filter(|(k, _)| k.trim_matches(|c: char| c == '{' || c.is_whitespace()).eq_ignore_ascii_case("font-family")).map(|(_, v)| v)) {
                declared.insert(v.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase());
            }
        }
    }
    declared
}

/// The declared web font families text in the document uses (each text-bearing element's first declared
/// family): Chrome requests those faces for the page's first layout.
fn web_fonts_in_use(inner: &BaseDocument, declared: &std::collections::HashSet<String>) -> std::collections::HashSet<String> {
    use style::values::computed::font::SingleFontFamily;
    let mut used = std::collections::HashSet::new();
    for id in compile::all_elements(inner) {
        let Some(node) = inner.get_node(id) else { continue };
        let text = node.children.iter().any(|&c| inner.get_node(c).is_some_and(|t| t.is_text_node() && !t.text_content().trim().is_empty()));
        let Some(styles) = node.primary_styles().filter(|_| text) else { continue };
        for family in styles.get_font().font_family.families.list.iter() {
            match family {
                SingleFontFamily::FamilyName(name) => {
                    let name = name.name.as_str().to_ascii_lowercase();
                    if declared.contains(&name) {
                        used.insert(name);
                        break;
                    }
                }
                SingleFontFamily::Generic(_) => break,
            }
        }
    }
    used
}

/// canvas `getImageData`: the bitmap of each canvas that has asked for pixels, as of its last flushed frame
/// (element ref -> width, height, straight-alpha RGBA8). A ref with `None` has asked but not been drawn yet.
type CanvasReadback = Arc<std::sync::Mutex<HashMap<u64, Option<(i32, i32, Vec<u8>)>>>>;

/// What a script's `tl.time(x)` samples (see `sample_declarations`): the timeline tracks, each animated HTML
/// element's base transform (by element ref) and the frame being rendered. Filled once the composition is set up.
struct SampleCtx {
    anim: std::rc::Rc<std::cell::RefCell<AnimScene>>,
    bases: HashMap<u64, TransformState>,
    html: std::collections::HashSet<u64>,
    frame_time: f64,
}
type SampleSlot = std::rc::Rc<std::cell::RefCell<Option<SampleCtx>>>;

/// A script's `tl.time(local)` on timeline `id`: every HTML element property that timeline drives, as GSAP would
/// render it there, as `[[element ref, {property: value or null}]]` for the shim to write (null: the element's
/// own style). A touched element's other transform components stay at the frame time. SVG children, motion
/// paths, morphs and DOM/text properties are left as the frame has them.
fn sample_declarations(ctx: &SampleCtx, anim: &mut AnimScene, id: u64, local: f64) -> Value {
    let times = anim.seek_times(id, local);
    let mut out: BTreeMap<u64, (Vec<(String, Option<CssValue>)>, serde_json::Map<String, Value>)> = BTreeMap::new();
    for (i, at) in times.into_iter().enumerate() {
        let Some(at) = at else { continue };
        let Target::El(r) = anim.tracks[i].target else { continue };
        let prop = anim.tracks[i].prop.clone();
        if !ctx.html.contains(&r) || prop.starts_with("--uv-") || prop.starts_with("attr:") || prop == "__inline" || is_dom_prop(&prop) {
            continue;
        }
        let value = anim.tracks[i].value_at(at);
        let css = |v: &Option<CssValue>| v.as_ref().map(|v| Value::String(v.to_css())).unwrap_or(Value::Null);
        let entry = out.entry(r).or_default();
        if is_transform_prop(&prop) {
            entry.0.push((prop, value));
        } else if prop == "autoAlpha" {
            let hidden = value.as_ref().and_then(|v| v.as_f32()).is_some_and(|o| o == 0.0);
            entry.1.insert("opacity".into(), css(&value));
            let vis = value.as_ref().map(|_| Value::String(if hidden { "hidden" } else { "inherit" }.into()));
            entry.1.insert("visibility".into(), vis.unwrap_or(Value::Null));
        } else if prop == "transformOrigin" {
            entry.1.insert("transform-origin".into(), css(&value));
        } else {
            let v = match &value {
                Some(v) => Value::String(v.to_css()),
                None => anim.tracks[i].inline.clone().filter(|s| !s.is_empty()).map(Value::String).unwrap_or(Value::Null),
            };
            entry.1.insert(kebab(&prop), v);
        }
    }
    let mut list = Vec::new();
    for (r, (changes, mut decls)) in out {
        if !changes.is_empty() {
            let mut state = ctx.bases.get(&r).cloned().unwrap_or_else(TransformState::identity);
            let mut any = false;
            for track in anim.tracks.iter_mut() {
                if track.target != Target::El(r) || !is_transform_prop(&track.prop) || changes.iter().any(|(p, _)| *p == track.prop) {
                    continue;
                }
                if let Some(v) = track.value_at(ctx.frame_time) {
                    state.set(&track.prop, &v);
                    any = true;
                }
            }
            for (p, v) in &changes {
                if let Some(v) = v {
                    state.set(p, v);
                    any = true;
                }
            }
            decls.insert("transform".into(), if any { Value::String(state.to_css()) } else { Value::Null });
        }
        list.push(json!([r, decls]));
    }
    Value::Array(list)
}

/// Web font families still loading for canvas text drawn while the page's scripts run (Chrome draws those in
/// the system Times); `None` once setup is over.
type ScriptFontsPending = Arc<std::sync::Mutex<Option<std::collections::HashSet<String>>>>;

fn register_natives(sdoc: ScriptDocument, net: Arc<SyncNet>, images: Arc<SrcImages>, html_path: PathBuf, readback: CanvasReadback, fonts_pending: ScriptFontsPending, sample: SampleSlot) -> ScriptDocument {
    let json_arg = |payload: &str| -> Value { serde_json::from_str(payload).unwrap_or(Value::Null) };
    let pixels_ctx = (net.clone(), images.clone(), html_path.clone());
    let gl_canvases = (readback.clone(), fonts_pending.clone());
    let (measure_pending, loaded_pending) = (fonts_pending.clone(), fonts_pending.clone());
    let sdoc = sdoc
        .with_native("glConstants", |_| Ok(include_str!("webgl_constants.json").to_string()))
        // a script seeking a timeline while the page runs (`tl.time(x)`): {id, t} -> the declarations the shim
        // writes (sample_declarations)
        .with_native("uvSample", move |payload| {
            let v: Value = serde_json::from_str(payload).map_err(|_| "bad timeline seek".to_string())?;
            let slot = sample.borrow();
            let Some(ctx) = slot.as_ref() else { return Ok("[]".into()) };
            let Ok(mut anim) = ctx.anim.try_borrow_mut() else { return Ok("[]".into()) };
            Ok(sample_declarations(ctx, &mut anim, v["id"].as_u64().unwrap_or(0), v["t"].as_f64().unwrap_or(0.0)).to_string())
        })
        // a script waited on document.fonts.ready / load(): Chrome resolves those once the faces have loaded
        .with_native("fontsLoaded", move |_| {
            blitz_dom::finish_pending_font_loads();
            *loaded_pending.lock().unwrap() = None;
            Ok(String::new())
        })
        .with_native("glAvailable", |_| Ok(if cfg!(feature = "webgl") && crate::webgl_available() { "true".into() } else { "false".into() }))
        .with_native("gl", move |payload| {
            #[cfg(feature = "webgl")]
            {
                let v: Value = serde_json::from_str(payload).map_err(|_| "bad WebGL call".to_string())?;
                let ops = v["ops"].as_array().cloned().unwrap_or_default();
                let answer = crate::webgl::query(
                    v["id"].as_u64().unwrap_or(0),
                    v["w"].as_i64().unwrap_or(300) as i32,
                    v["h"].as_i64().unwrap_or(150) as i32,
                    v["v"].as_u64().unwrap_or(2) as u8,
                    &v["attrs"],
                    &ops,
                    &v["q"],
                    &|src: &Value| {
                        // a canvas source: its last flush plus the 2D calls it received since (the shim
                        // sends those in `srcs`)
                        #[cfg(feature = "skia")]
                        if let Some(r) = src["ref"].as_u64() {
                            let flushed = gl_canvases.0.lock().unwrap().get(&r).cloned().flatten();
                            let p = &v["srcs"][r.to_string().as_str()];
                            if let (Some(pw), Some(ph), Some(pops)) = (p["w"].as_i64(), p["h"].as_i64(), p["ops"].as_array()) {
                                let (pw, ph) = (pw as i32, ph as i32);
                                let mut sc = crate::canvas::Canvas2D::new(pw, ph)?;
                                if let Some((fw, fh, px)) = flushed.filter(|(fw, fh, _)| (*fw as i32, *fh as i32) == (pw, ph)) {
                                    sc.write_rgba(fw as i32, fh as i32, &px);
                                }
                                sc.pending_fonts = gl_canvases.1.lock().unwrap().clone();
                                sc.apply(pops, &|_| None);
                                return Some((pw as u32, ph as u32, sc.region(0, 0, pw, ph)));
                            }
                            if let Some((fw, fh, px)) = flushed {
                                return Some((fw as u32, fh as u32, px));
                            }
                        }
                        src["src"].as_str().and_then(|s| decode_src(&net, &html_path, &images, s))
                    },
                )?;
                Ok(answer.to_string())
            }
            #[cfg(not(feature = "webgl"))]
            {
                let _ = (payload, &net, &html_path, &images);
                Err("this build has no WebGL".to_string())
            }
        })
        // canvas getImageData: {ref, w, h, ops, x, y, rw, rh} -> base64 straight-alpha RGBA8 of the
        // rectangle: the canvas's last flushed frame with the calls recorded since replayed on top
        .with_native("canvasPixels", {
            let (net, images, html_path) = pixels_ctx;
            move |payload| {
                #[cfg(feature = "skia")]
                {
                    let v: Value = serde_json::from_str(payload).map_err(|_| "bad getImageData call".to_string())?;
                    let r = v["ref"].as_u64().unwrap_or(0);
                    let (w, h) = (v["w"].as_i64().unwrap_or(300) as i32, v["h"].as_i64().unwrap_or(150) as i32);
                    let base = {
                        let mut rb = readback.lock().unwrap();
                        rb.entry(r).or_insert(None).clone()
                    };
                    let mut c = crate::canvas::Canvas2D::new(w, h).ok_or("canvas too large")?;
                    if let Some((bw, bh, px)) = base.filter(|(bw, bh, _)| (*bw, *bh) == (w, h)) {
                        c.write_rgba(bw, bh, &px);
                    }
                    c.pending_fonts = fonts_pending.lock().unwrap().clone();
                    let ops = v["ops"].as_array().cloned().unwrap_or_default();
                    // sources: offscreen canvases from their last flush plus the calls they received since
                    // (the shim sends those in `srcs`), images by URL
                    let pending = &v["srcs"];
                    let srcs: HashMap<u64, (u32, u32, Vec<u8>)> = ops
                        .iter()
                        .filter_map(crate::canvas::op_image_source)
                        .filter_map(|o| {
                            let sr = o["ref"].as_u64()?;
                            let flushed = readback.lock().unwrap().get(&sr).cloned().flatten();
                            let p = &pending[sr.to_string().as_str()];
                            if let (Some(pw), Some(ph), Some(pops)) = (p["w"].as_i64(), p["h"].as_i64(), p["ops"].as_array()) {
                                let (pw, ph) = (pw as i32, ph as i32);
                                let mut sc = crate::canvas::Canvas2D::new(pw, ph)?;
                                if let Some((fw, fh, px)) = flushed.filter(|(fw, fh, _)| (*fw as i32, *fh as i32) == (pw, ph)) {
                                    sc.write_rgba(fw as i32, fh as i32, &px);
                                }
                                sc.pending_fonts = fonts_pending.lock().unwrap().clone();
                                sc.apply(pops, &|_| None);
                                return Some((sr, (pw as u32, ph as u32, sc.region(0, 0, pw, ph))));
                            }
                            match flushed {
                                Some((sw, sh, px)) => Some((sr, (sw as u32, sh as u32, px))),
                                None => Some((sr, decode_src(&net, &html_path, &images, o["src"].as_str()?)?)),
                            }
                        })
                        .collect();
                    c.apply(&ops, &|r| srcs.get(&r).cloned());
                    let px = c.region(
                        v["x"].as_f64().unwrap_or(0.0) as i32,
                        v["y"].as_f64().unwrap_or(0.0) as i32,
                        v["rw"].as_f64().unwrap_or(0.0) as i32,
                        v["rh"].as_f64().unwrap_or(0.0) as i32,
                    );
                    use base64::Engine as _;
                    if v["png"].as_bool().unwrap_or(false) {
                        // toDataURL: the region as a PNG data URL (straight alpha, like the canvas)
                        let (rw, rh) = (v["rw"].as_u64().unwrap_or(0) as u32, v["rh"].as_u64().unwrap_or(0) as u32);
                        let mut png = Vec::new();
                        use image::ImageEncoder as _;
                        image::codecs::png::PngEncoder::new(&mut png)
                            .write_image(&px, rw, rh, image::ExtendedColorType::Rgba8)
                            .map_err(|e| format!("toDataURL: {e}"))?;
                        return Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)));
                    }
                    Ok(base64::engine::general_purpose::STANDARD.encode(px))
                }
                #[cfg(not(feature = "skia"))]
                {
                    let _ = (payload, &net, &images, &html_path, &readback, &fonts_pending);
                    Err("this build has no canvas".to_string())
                }
            }
        })
        // canvas text metrics: [font, text] -> [width, ascent, descent]
        .with_native("measureText", move |payload| {
            let arg = json_arg(payload);
            #[cfg(feature = "skia")]
            {
                let pending = measure_pending.lock().unwrap().clone();
                let (w, a, d) = crate::canvas::measure_text(arg[0].as_str().unwrap_or("10px sans-serif"), arg[1].as_str().unwrap_or(""), pending.as_ref());
                Ok(json!([w, a, d]).to_string())
            }
            #[cfg(not(feature = "skia"))]
            {
                let _ = arg;
                Ok("[0,0,0]".into())
            }
        })
        // a <path>'s getTotalLength / getPointAtLength the way Blink's SVGPathElement traverses it
        // (src/paths.rs blink_length): [d, [lengths]] -> {length, points}
        .with_native("pathMeasure", move |payload| {
            let arg = json_arg(payload);
            let d = arg[0].as_str().unwrap_or("");
            let path = crate::paths::parse(d);
            let points: Vec<[f64; 2]> = arg[1]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_f64).map(|l| crate::paths::svg_point_at_length(d, &path, l)).map(|(x, y)| [x, y]).collect())
                .unwrap_or_default();
            Ok(json!({"length": crate::paths::blink_length(&path), "points": points}).to_string())
        });
    sdoc
}

/// Script fetches (`<script src>`, `fetch()`, XHR; binary bodies through "uv-bytes:").
struct NetFetcher(Arc<SyncNet>);

impl ScriptFetcher for NetFetcher {
    fn fetch(&self, url: &Url) -> Result<String, FetchError> {
        // "uv-bytes:<absolute url>": the body as base64, for the shim's binary fetch / XHR / Image
        if let Some(inner) = url.as_str().strip_prefix("uv-bytes:") {
            let inner = Url::parse(inner).map_err(|e| FetchError::InvalidData(e.to_string()))?;
            let bytes = self.0.load(&inner).map_err(FetchError::InvalidData)?;
            use base64::Engine as _;
            return Ok(base64::engine::general_purpose::STANDARD.encode(bytes));
        }
        let bytes = self.0.load(url).map_err(FetchError::InvalidData)?;
        // binary bodies stay fetchable (their bytes come through "uv-bytes:")
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Name the script and line an uncaught error came from. The engine reports inline scripts as
/// "<inline script> ... (unknown at :LINE:COL)"; scripts carry their origin ("file#n", tagged by
/// compile for sub-compositions) and the error's identifier picks the script when several have
/// that line. The result reads "Uncaught JS error in compositions/stats.html, inline script 2,
/// line 4:95: ReferenceError: ... | `<that line of code>`".
fn locate_script_errors(doc: &BaseDocument, errors: Vec<String>, html_path: &std::path::Path) -> Vec<String> {
    let main = html_path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "index.html".into());
    let mut scripts: Vec<(String, usize, String)> = Vec::new();
    let mut main_index = 0;
    for id in compile::all_elements(doc) {
        if compile::tag(doc, id) != "script" || compile::has_attr(doc, id, "src") {
            continue;
        }
        let text = doc.get_node(id).map(|n| n.text_content()).unwrap_or_default();
        let (origin, offset) = match compile::attr(doc, id, "data-uv-origin") {
            Some(o) => (o, compile::attr(doc, id, "data-uv-line-offset").and_then(|v| v.parse().ok()).unwrap_or(0)),
            None => {
                main_index += 1;
                (format!("{main}#{main_index}"), 0)
            }
        };
        scripts.push((origin, offset, text));
    }
    if crate::env_os("DEBUG").is_some() {
        for e in &errors {
            eprintln!("[debug] raw js error: {e}");
        }
    }
    // each script throws at most once, in document order: later errors come from later scripts
    let mut after = 0usize;
    errors
        .into_iter()
        .map(|e| {
            if !e.contains("<inline script>") {
                return e;
            }
            let Some(at) = e.find("(unknown at :") else { return e };
            // the innermost frame (first "at" line) is where it failed; the header is the outermost
            let inner = e.find("\n").and_then(|nl| e[nl..].find("(unknown at :").map(|k| nl + k)).unwrap_or(at);
            let pos: Vec<usize> = e[inner + 13..].split([':', ')']).take(2).filter_map(|n| n.trim().parse().ok()).collect();
            let [line, col] = pos[..] else { return e };
            let msg_start = e.find(": ").map(|i| i + 2).unwrap_or(0);
            let message = e[msg_start..at].trim();
            // the identifier the message is about, to tell scripts apart
            let ident = message
                .split(['\'', '"', ' '])
                .find(|w| w.len() > 1 && w.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.'))
                .unwrap_or("");
            let line_of = |text: &str| text.lines().nth(line.saturating_sub(1)).map(str::to_string);
            let in_order: Vec<(usize, &(String, usize, String))> = scripts.iter().enumerate().filter(|(i, (_, _, t))| *i >= after && line_of(t).is_some()).collect();
            let any: Vec<(usize, &(String, usize, String))> = scripts.iter().enumerate().filter(|(_, (_, _, t))| line_of(t).is_some()).collect();
            let pool = if in_order.is_empty() { any } else { in_order };
            let mut hits: Vec<&(String, usize, String)> = pool.iter().map(|(_, s)| *s).collect();
            if hits.len() > 1 && !ident.is_empty() {
                let named: Vec<&(String, usize, String)> = hits
                    .iter()
                    .copied()
                    .filter(|(_, _, t)| line_of(t).is_some_and(|l| l.get(col.saturating_sub(1)..).is_some_and(|rest| rest.contains(ident.split('.').next().unwrap_or(ident)))))
                    .collect();
                if !named.is_empty() {
                    hits = named;
                }
            }
            let Some(&hit) = hits.first() else { return e };
            if let Some(i) = scripts.iter().position(|s| std::ptr::eq(s, hit)) {
                after = i + 1;
            }
            let (origin, offset, text) = hit;
            let (file, n) = origin.rsplit_once('#').unwrap_or((origin.as_str(), "1"));
            let code = line_of(text).unwrap_or_default();
            let code = code.trim();
            let code: String = if code.chars().count() > 160 {
                let start = col.saturating_sub(60).min(code.len());
                let s: String = code.chars().skip(start).take(140).collect();
                format!("…{s}…")
            } else {
                code.to_string()
            };
            format!(
                "Uncaught JS error in {file}, inline script {n}, line {}:{col}: {message} | `{code}`",
                line.saturating_sub(*offset)
            )
        })
        .collect()
}


/// GSAP-style transform components of one element.
#[derive(Clone, Debug)]
struct TransformState {
    x: CssValue,
    y: CssValue,
    z: CssValue,
    x_percent: f32,
    y_percent: f32,
    rotation: f32,
    rotation_x: f32,
    rotation_y: f32,
    skew_x: f32,
    skew_y: f32,
    scale_x: f32,
    scale_y: f32,
    perspective: f32,
}

impl TransformState {
    fn identity() -> Self {
        TransformState {
            x: CssValue::num(0.0, "px"),
            y: CssValue::num(0.0, "px"),
            z: CssValue::num(0.0, "px"),
            x_percent: 0.0,
            y_percent: 0.0,
            rotation: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            perspective: 0.0,
        }
    }

    /// GSAP's _parseTransform for a 2D matrix.
    fn from_affine(m: Affine) -> Self {
        let [a, b, c, d, e, f] = m.as_coeffs().map(|v| v as f32);
        let mut s = TransformState::identity();
        s.x = CssValue::num(e, "px");
        s.y = CssValue::num(f, "px");
        s.scale_x = (a * a + b * b).sqrt();
        s.scale_y = (d * d + c * c).sqrt();
        s.rotation = if a != 0.0 || b != 0.0 { b.atan2(a).to_degrees() } else { 0.0 };
        s.skew_x = if c != 0.0 || d != 0.0 { c.atan2(d).to_degrees() + s.rotation } else { 0.0 };
        if s.skew_x != 0.0 {
            s.scale_y *= s.skew_x.to_radians().cos().abs();
        }
        s
    }

    /// GSAP's _parseTransform percentage detection: an x (y) translation of exactly minus half the
    /// element's offsetWidth (offsetHeight), compared with Math.round, is taken as `xPercent: -50`
    /// (`yPercent`), keeping only the residual in px. A later size change (new text) keeps the element
    /// centred, as in Chrome; it also leaves the sub-pixel residual GSAP renders as a px translate.
    fn detect_percent(mut self, x: f32, y: f32, offset_width: f32, offset_height: f32) -> Self {
        let js_round = |v: f32| (v + 0.5).floor();
        if x != 0.0 && offset_width != 0.0 && js_round(offset_width / 2.0) == js_round(-x) {
            self.x_percent = -50.0;
            self.x = CssValue::num(x + offset_width / 2.0, "px");
        }
        if y != 0.0 && offset_height != 0.0 && js_round(offset_height / 2.0) == js_round(-y) {
            self.y_percent = -50.0;
            self.y = CssValue::num(y + offset_height / 2.0, "px");
        }
        self
    }

    /// GSAP's _parseTransform for a matrix3d (CSSPlugin.js, "3D matrix").
    fn from_matrix3d(t: &euclid::default::Transform3D<f64>) -> Self {
        let deg = 180.0 / std::f64::consts::PI;
        let round = |v: f64| (v * 100000.0).round() / 100000.0;
        let (mut a, mut b, mut c, d) = (t.m11, t.m12, t.m13, t.m14);
        let (mut a12, mut a22, mut a32, a42) = (t.m21, t.m22, t.m23, t.m24);
        let (mut a13, mut a23, mut a33, mut a43) = (t.m31, t.m32, t.m33, t.m34);
        let (x, y, z) = (t.m41, t.m42, t.m43);
        let mut angle = a32.atan2(a33);
        let mut rotation_x = angle * deg;
        if angle != 0.0 {
            let (cos, sin) = ((-angle).cos(), (-angle).sin());
            let t1 = a12 * cos + a13 * sin;
            let t2 = a22 * cos + a23 * sin;
            let t3 = a32 * cos + a33 * sin;
            a13 = a12 * -sin + a13 * cos;
            a23 = a22 * -sin + a23 * cos;
            a33 = a32 * -sin + a33 * cos;
            a43 = a42 * -sin + a43 * cos;
            a12 = t1;
            a22 = t2;
            a32 = t3;
        }
        angle = (-c).atan2(a33);
        let mut rotation_y = angle * deg;
        if angle != 0.0 {
            let (cos, sin) = ((-angle).cos(), (-angle).sin());
            let t1 = a * cos - a13 * sin;
            let t2 = b * cos - a23 * sin;
            let t3 = c * cos - a33 * sin;
            a43 = d * sin + a43 * cos;
            a = t1;
            b = t2;
            c = t3;
        }
        angle = b.atan2(a);
        let mut rotation = angle * deg;
        if angle != 0.0 {
            let (cos, sin) = (angle.cos(), angle.sin());
            let t1 = a * cos + b * sin;
            let t2 = a12 * cos + a22 * sin;
            b = b * cos - a * sin;
            a22 = a22 * cos - a12 * sin;
            a = t1;
            a12 = t2;
        }
        let _ = b;
        if rotation_x != 0.0 && rotation_x.abs() + rotation.abs() > 359.9 {
            rotation_x = 0.0;
            rotation = 0.0;
            rotation_y = 180.0 - rotation_y;
        }
        let scale_x = round((a * a + b * b + c * c).sqrt());
        let scale_y = round((a22 * a22 + a32 * a32).sqrt());
        angle = a12.atan2(a22);
        let skew_x = if angle.abs() > 0.0002 { angle * deg } else { 0.0 };
        let perspective = if a43 != 0.0 { 1.0 / a43.abs() } else { 0.0 };
        let mut s = TransformState::identity();
        s.x = CssValue::num(x as f32, "px");
        s.y = CssValue::num(y as f32, "px");
        s.z = CssValue::num(z as f32, "px");
        s.scale_x = scale_x as f32;
        s.scale_y = scale_y as f32;
        s.rotation = round(rotation) as f32;
        s.rotation_x = round(rotation_x) as f32;
        s.rotation_y = round(rotation_y) as f32;
        s.skew_x = skew_x as f32;
        s.perspective = perspective as f32;
        s
    }

    fn set(&mut self, prop: &str, v: &CssValue) {
        let n = || v.as_f32().unwrap_or(0.0);
        match prop {
            "x" => self.x = v.clone(),
            "y" => self.y = v.clone(),
            "z" => self.z = v.clone(),
            "xPercent" => self.x_percent = n(),
            "yPercent" => self.y_percent = n(),
            "rotation" => self.rotation = n(),
            "rotationX" => self.rotation_x = n(),
            "rotationY" => self.rotation_y = n(),
            "skewX" => self.skew_x = n(),
            "skewY" => self.skew_y = n(),
            "scaleX" => self.scale_x = n(),
            "scaleY" => self.scale_y = n(),
            "transformPerspective" => self.perspective = n(),
            _ => {}
        }
    }

    /// GSAP's _renderCSSTransforms order.
    fn to_css(&self) -> String {
        let mut out = String::new();
        if self.perspective != 0.0 {
            out.push_str(&format!("perspective({}px) ", fmt_num(self.perspective)));
        }
        if self.x_percent != 0.0 || self.y_percent != 0.0 {
            out.push_str(&format!("translate({}%, {}%) ", fmt_num(self.x_percent), fmt_num(self.y_percent)));
        }
        let zero = |v: &CssValue| v.as_f32() == Some(0.0) && !matches!(v, CssValue::Mix { .. });
        if !zero(&self.z) {
            out.push_str(&format!("translate3d({}, {}, {}) ", self.x.to_css(), self.y.to_css(), self.z.to_css()));
        } else if !zero(&self.x) || !zero(&self.y) {
            out.push_str(&format!("translate({}, {}) ", self.x.to_css(), self.y.to_css()));
        }
        if self.rotation != 0.0 {
            out.push_str(&format!("rotate({}deg) ", fmt_num(self.rotation)));
        }
        if self.rotation_y != 0.0 {
            out.push_str(&format!("rotateY({}deg) ", fmt_num(self.rotation_y)));
        }
        if self.rotation_x != 0.0 {
            out.push_str(&format!("rotateX({}deg) ", fmt_num(self.rotation_x)));
        }
        if self.skew_x != 0.0 || self.skew_y != 0.0 {
            out.push_str(&format!("skew({}deg, {}deg) ", fmt_num(self.skew_x), fmt_num(self.skew_y)));
        }
        if self.scale_x != 1.0 || self.scale_y != 1.0 {
            out.push_str(&format!("scale({}, {}) ", fmt_num(self.scale_x), fmt_num(self.scale_y)));
        }
        let s = out.trim_end().to_string();
        if s.is_empty() { "none".into() } else { s }
    }
}

fn is_transform_prop(p: &str) -> bool {
    matches!(
        p,
        "x" | "y" | "z" | "xPercent" | "yPercent" | "rotation" | "rotationX" | "rotationY" | "skewX" | "skewY"
            | "scaleX" | "scaleY" | "transformPerspective"
    )
}

/// Element properties GSAP sets as DOM properties rather than CSS.
fn is_dom_prop(p: &str) -> bool {
    matches!(p, "innerText" | "textContent" | "innerHTML" | "value" | "scrollTop" | "scrollLeft" | "src" | "currentTime")
}

fn kebab(p: &str) -> String {
    if p.starts_with("--") {
        return p.to_string();
    }
    let mut s = String::with_capacity(p.len() + 4);
    for c in p.chars() {
        if c.is_ascii_uppercase() {
            s.push('-');
            s.push(c.to_ascii_lowercase());
        } else {
            s.push(c);
        }
    }
    // vendor prefixes: webkitClipPath -> -webkit-clip-path
    if s.starts_with("webkit-") || s.starts_with("moz-") || s.starts_with("ms-") {
        s.insert(0, '-');
    }
    s
}

/// Which rasterizer paints frames. Skia is Chrome's rasterizer (parity); vello_cpu is pure Rust; Gpu is instanced
/// quads composited on the GPU with vello_cpu's arithmetic (src/quads), level for level the same pictures.
/// `Auto` resolves at load: Skia when the composition uses backdrop-filter (vello_cpu has none),
/// otherwise the GPU (vello_cpu with RAEA_PAINT=cpu).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Backend {
    Skia,
    Vello,
    /// instanced quads on the GPU (src/quads), vello_cpu for a frame it can't paint
    Gpu,
    #[default]
    Auto,
}

/// A 3D plane's flat layer drawn through its 4x4 matrix with Skia (`concat_44`, bilinear, antialiased
/// edges) into a device-space image (see blitz_paint::LayerWarper).
#[cfg(feature = "skia")]
#[allow(clippy::too_many_arguments)]
fn warp_layer_skia(src: &[u8], sw: u32, sh: u32, sx0: f64, sy0: f64, m: [[f64; 4]; 4], dx0: f64, dy0: f64, dw: u32, dh: u32) -> Option<Vec<u8>> {
    use skia_safe as sk;
    let info = sk::ImageInfo::new((sw as i32, sh as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let image = sk::images::raster_from_data(&info, sk::Data::new_copy(src), sw as usize * 4)?;
    let dinfo = sk::ImageInfo::new((dw as i32, dh as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let mut surface = sk::surfaces::raster(&dinfo, None, None)?;
    let canvas = surface.canvas();
    canvas.clear(sk::Color::TRANSPARENT);
    canvas.translate((-dx0 as f32, -dy0 as f32));
    let r = |i: usize| m[i].map(|v| v as f32);
    let (r0, r1, r2, r3) = (r(0), r(1), r(2), r(3));
    canvas.concat_44(&sk::M44::row_major(&[
        r0[0], r0[1], r0[2], r0[3], r1[0], r1[1], r1[2], r1[3], r2[0], r2[1], r2[2], r2[3], r3[0], r3[1], r3[2], r3[3],
    ]));
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    // the layer without blitz-paint's 2 px transparent margin, clamped at its edges, with the quad's
    // edges antialiased analytically (cc's layer quad: clamp-to-edge texture, AA edges)
    const MARGIN: f32 = 2.0;
    let (fw, fh) = (sw as f32, sh as f32);
    if fw > 2.0 * MARGIN && fh > 2.0 * MARGIN {
        let src_rect = sk::Rect::new(MARGIN, MARGIN, fw - MARGIN, fh - MARGIN);
        let dst_rect = src_rect.with_offset((sx0 as f32, sy0 as f32));
        canvas.draw_image_rect_with_sampling_options(
            &image,
            Some((&src_rect, sk::canvas::SrcRectConstraint::Strict)),
            dst_rect,
            sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
            &paint,
        );
    } else {
        canvas.draw_image_with_sampling_options(&image, (sx0 as f32, sy0 as f32), sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None), Some(&paint));
    }
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    surface.read_pixels(&dinfo, &mut out, dw as usize * 4, (0, 0)).then_some(out)
}

/// MotionPathPlugin's setup for spec `k` of `node` (recorded by the shim in `data-uv-motion`):
/// the path in the units of the target's x/y. With `align`, path coordinates are mapped to the
/// page through the owning <svg>'s viewBox and shifted so the target's alignOrigin point sits on
/// the path (GSAP's _align / getAlignMatrix for an untransformed path).
fn motion_path(doc: &BaseDocument, node: NodeId, k: usize, elements: &HashMap<u64, NodeId>) -> Option<(crate::paths::RawPath, Option<f64>, f64, f64)> {
    use crate::paths;
    let specs: Value = serde_json::from_str(&compile::attr(doc, node, "data-uv-motion")?).ok()?;
    let spec = specs.get(k)?;
    let mut path = paths::parse(spec["d"].as_str()?);
    let first = path.segs.first()?;
    let (x0, y0) = (first.pts[0], first.pts[1]);
    match &spec["align"] {
        Value::String(s) if s == "self" => paths::transform(&mut path, 1.0, 0.0, 0.0, 1.0, -x0, -y0),
        Value::Number(r) => {
            let align = *elements.get(&r.as_u64()?)?;
            let m = svg_user_to_page(doc, align).unwrap_or(Affine::IDENTITY);
            let target = doc.get_node(node)?;
            let pos = target.absolute_position(0.0, 0.0);
            let size = target.final_layout().size;
            let ao = spec["alignOrigin"].as_array();
            let o = |i: usize| ao.and_then(|a| a.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
            let [a, b, c, d, e, f] = m.as_coeffs();
            paths::transform(&mut path, a, b, c, d, e - (pos.x as f64 + o(0) * size.width as f64), f - (pos.y as f64 + o(1) * size.height as f64));
        }
        _ => {}
    }
    let (ox, oy) = (spec["offsetX"].as_f64().unwrap_or(0.0), spec["offsetY"].as_f64().unwrap_or(0.0));
    if ox != 0.0 || oy != 0.0 {
        paths::transform(&mut path, 1.0, 0.0, 0.0, 1.0, ox, oy);
    }
    paths::measure(&mut path, spec["resolution"].as_u64().unwrap_or(12) as usize);
    let (start, end) = (spec["start"].as_f64().unwrap_or(0.0), spec["end"].as_f64().unwrap_or(1.0));
    Some((path, spec["autoRotate"].as_f64(), start, end))
}

/// TextPlugin's render for spec `k` of `node` (recorded by the shim in `data-uv-text`) at tween
/// progress `p`: the string GSAP's plugin writes, and whether it goes to `textContent` (SVG text)
/// rather than `innerHTML`.
fn text_plugin_at(doc: &BaseDocument, node: NodeId, k: usize, p: f64) -> Option<(String, bool)> {
    let specs: Value = serde_json::from_str(&compile::attr(doc, node, "data-uv-text")?).ok()?;
    let s = specs.get(k)?;
    let list = |key: &str| -> Vec<String> {
        s[key].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default()
    };
    let flag = |key: &str| s[key].as_bool().unwrap_or(false);
    let (text, original) = (list("text"), list("original"));
    let d = s["delimiter"].as_str().unwrap_or_default();
    let mut ratio = p.clamp(0.0, 1.0);
    if flag("from") {
        ratio = 1.0 - ratio;
    }
    let l = text.len();
    let i = (((if flag("rtl") { 1.0 - ratio } else { ratio }) * l as f64 + 0.5).floor().max(0.0) as usize).min(l);
    let head = text[..i].join(d);
    let tail = original.get(i..).map(|t| t.join(d)).unwrap_or_default();
    let (new_class, old_class) = (s["newClass"].as_str().unwrap_or_default(), s["oldClass"].as_str().unwrap_or_default());
    let out = if (!new_class.is_empty() || !old_class.is_empty()) && ratio > 0.0 {
        let (apply_new, apply_old) = (!new_class.is_empty() && i > 0, !old_class.is_empty() && i != l);
        let mut o = String::new();
        if apply_new {
            o += &format!("<span class='{new_class}'>");
        }
        o += &head;
        if apply_new {
            o += "</span>";
        }
        if apply_old {
            o += &format!("<span class='{old_class}'>");
        }
        o += d;
        o += &tail;
        if apply_old {
            o += "</span>";
        }
        o
    } else {
        format!("{head}{d}{tail}")
    };
    let svg = flag("svg");
    let out = if !svg && s["fillChar"].as_str() == Some("&nbsp;") && out.contains("  ") { out.split("  ").collect::<Vec<_>>().join("&nbsp;&nbsp;") } else { out };
    Some((out, svg))
}

/// MorphSVGPlugin's setup for spec `k` of path `node` (recorded by the shim in `data-uv-morph`):
/// it starts from the authored `d` (or the previous morph's shape) and equalizes the two shapes.
fn svg_morph(doc: &BaseDocument, node: NodeId, k: usize, original: Option<&str>) -> Option<crate::paths::Morph> {
    let specs: Value = serde_json::from_str(&compile::attr(doc, node, "data-uv-morph")?).ok()?;
    let spec = specs.get(k)?;
    let start = if k == 0 {
        original.map(str::to_string).or_else(|| compile::attr(doc, node, "data-original"))?
    } else {
        specs.get(k - 1)?["shape"].as_str()?.to_string()
    };
    // GSAP's fillSafe: a visible nonzero fill lets it reverse/offset subpaths freely
    let fill = compile::attr(doc, node, "fill").unwrap_or_default().to_ascii_lowercase();
    let transparent = fill == "none" || crate::value::parse_color(&fill).is_some_and(|c| c[3] == 0.0);
    let evenodd = compile::attr(doc, node, "fill-rule").is_some_and(|r| r.trim() == "evenodd");
    crate::paths::Morph::new(
        &start,
        spec["shape"].as_str()?,
        spec["shapeIndex"].as_i64(),
        spec["map"].as_str().unwrap_or("size"),
        !(transparent || evenodd),
        spec["precision"].as_u64().unwrap_or(2) as u32,
    )
}

/// SVG user space of `el` -> page CSS pixels: the owning <svg>'s box and viewBox
/// (preserveAspectRatio xMidYMid meet).
fn svg_user_to_page(doc: &BaseDocument, el: NodeId) -> Option<Affine> {
    let mut cur = Some(el);
    while let Some(id) = cur {
        if compile::tag(doc, id) == "svg" {
            let svg = doc.get_node(id)?;
            let pos = svg.absolute_position(0.0, 0.0);
            let size = svg.final_layout().size;
            let (w, h) = (size.width as f64, size.height as f64);
            let vb: Vec<f64> = compile::attr(doc, id, "viewBox")
                .map(|v| v.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|n| n.parse().ok()).collect())
                .unwrap_or_default();
            let user = if vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0 {
                let s = (w / vb[2]).min(h / vb[3]);
                Affine::translate(((w - vb[2] * s) / 2.0 - vb[0] * s, (h - vb[3] * s) / 2.0 - vb[1] * s)) * Affine::scale(s)
            } else {
                Affine::IDENTITY
            };
            return Some(Affine::translate((pos.x as f64, pos.y as f64)) * user);
        }
        cur = doc.get_node(id)?.parent;
    }
    None
}

/// DrawSVG's render step: "dash offset length" -> (stroke-dasharray, stroke-dashoffset).
/// A zero dash only happens at a tween's ends (GSAP uses 0.0001 mid-tween).
fn draw_svg_dash(v: &CssValue) -> (String, String) {
    let n: Vec<f64> = match v {
        CssValue::Complex { nums, .. } => nums.iter().map(|&x| x as f64).collect(),
        other => other.to_css().split_whitespace().filter_map(|x| x.parse().ok()).collect(),
    };
    let (dash, mut offset, total) = (n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(0.0), n.get(2).copied().unwrap_or(0.0));
    let round = |x: f64| (x * 10000.0).round() / 10000.0;
    let (dash, total) = (round(dash), round(total));
    let mut length = total - dash + 0.1;
    if dash != 0.0 && offset != 0.0 && total > 0.0 && dash + (offset % total).abs() > total - 0.05 {
        offset += if offset < 0.0 { 0.005 } else { -0.005 };
        length += 0.005;
    }
    let offset = if dash != 0.0 { offset } else { offset + 0.001 };
    let array = if length < 0.1 {
        "none".to_string()
    } else if dash != 0.0 {
        format!("{}px,{}px", fmt_num(dash as f32), fmt_num(length as f32))
    } else {
        "0px, 999999px".to_string()
    };
    (array, fmt_num(round(offset) as f32))
}

/// Offscreen layer for a 3D-transformed plane: premultiplied RGBA8.
fn raster_layer_vello(scene: anyrender::Scene, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let mut r = VelloCpuImageRenderer::new(w, h);
    r.render(|s| anyrender::PaintScene::append_scene(s, scene, Affine::IDENTITY), &mut out);
    out
}

#[cfg(feature = "skia")]
fn raster_layer_skia(scene: anyrender::Scene, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let mut r = SkiaImageRenderer::new(w, h);
    r.render(|s| anyrender::PaintScene::append_scene(s, scene, Affine::IDENTITY), &mut out);
    // the vendored Skia renderer hands back straight alpha
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    out
}

/// An inline SVG's recording rasterized by Skia straight into premultiplied RGBA8 (a reused surface
/// and painter cache per thread).
#[cfg(feature = "skia")]
fn raster_svg_skia(scene: anyrender::Scene, w: u32, h: u32) -> Vec<u8> {
    use skia_safe as sk;
    thread_local! {
        static CACHE: std::cell::RefCell<anyrender_skia::SkiaSceneCache> = std::cell::RefCell::new(anyrender_skia::SkiaSceneCache::new());
    }
    let info = sk::ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
    let props = sk::SurfaceProps::new_with_text_properties(Default::default(), sk::PixelGeometry::Unknown, 0.0, 1.0);
    let mut out = vec![0u8; (w * h * 4) as usize];
    let Some(mut surface) = sk::surfaces::wrap_pixels(&info, &mut out, w as usize * 4, Some(&props)) else {
        return raster_layer_skia(scene, w, h);
    };
    surface.canvas().clear(sk::Color::TRANSPARENT);
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let mut painter = anyrender_skia::SkiaScenePainter::new(surface.canvas(), &mut cache);
        anyrender::PaintScene::append_scene(&mut painter, scene, Affine::IDENTITY);
        drop(painter);
        // fonts/images unused for a generation age out
        cache.next_gen();
    });
    drop(surface);
    out
}

/// CSS filter functions vello_cpu can't paint (it has blur, drop-shadow and offset only).
const COLOR_FILTER_FNS: [&str; 8] =
    ["brightness(", "contrast(", "saturate(", "grayscale(", "hue-rotate(", "invert(", "sepia(", "opacity("];

/// Whether any stylesheet, inline style or script mentions backdrop-filter, a colour filter function
/// (brightness(), saturate(), ...) or can point an element's `filter` at an SVG `<filter>` (`url(#id)`
/// while the document has one). vello_cpu paints none of these; filters inside SVGs rasterize through
/// Skia under either backend.
/// Whether a composition uses what Skia renders closest to Chrome (backdrop-filter, SVG `url(#)` filters,
/// colour filter functions, data-fx), and whether it uses backdrop-filter, the one of them vello doesn't paint.
fn skia_features(doc: &BaseDocument) -> (bool, bool) {
    let ids = compile::all_elements(doc);
    // data-fx effects are SVG filters fx.js builds at load (style set through CSSOM, so no text shows them)
    let mut any = ids.iter().any(|&id| compile::attr(doc, id, "data-fx").is_some());
    let mut backdrop = false;
    let has_svg_filter = ids.iter().any(|&id| compile::tag(doc, id) == "filter");
    for id in ids {
        let text = match compile::tag(doc, id).as_str() {
            "style" | "script" => doc.get_node(id).map(|n| n.text_content()),
            _ => compile::attr(doc, id, "style"),
        };
        let Some(t) = text.map(|t| t.to_ascii_lowercase()) else { continue };
        if t.contains("backdrop-filter") || t.contains("backdropfilter") {
            backdrop = true;
            any = true;
        } else if (has_svg_filter && t.contains("url(#")) || (t.contains("filter") && COLOR_FILTER_FNS.iter().any(|f| t.contains(f))) {
            any = true;
        }
    }
    (any, backdrop)
}

enum Raster {
    Vello(VelloCpuImageRenderer),
    #[cfg(feature = "skia")]
    Skia(SkiaImageRenderer),
}

fn paint_frame<S: anyrender::PaintScene>(scene: &mut S, doc: &mut BaseDocument, scale: f64, w: u32, h: u32, transparent: bool) {
    if !transparent {
        scene.fill(Fill::NonZero, Default::default(), Color::WHITE, Default::default(), &Rect::new(0.0, 0.0, w as f64, h as f64));
    }
    blitz_paint::paint_scene(scene, doc, scale, w, h, 0, 0);
}

pub struct Composition {
    pub doc: ScriptDocument,
    pub net: Arc<SyncNet>,
    pub anim: std::rc::Rc<std::cell::RefCell<AnimScene>>,
    pub timing: Timing,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub authored_duration: Option<f64>,
    pub timeline_duration: f64,
    pub warnings: Vec<String>,
    pub js_errors: Vec<String>,
    pub unsupported: Vec<String>,
    opts: Options,
    elements: HashMap<u64, NodeId>,
    transforms: HashMap<NodeId, TransformState>,
    applied: HashMap<(NodeId, String), String>,
    /// clearProps "all": authored inline declarations removed while the clear is in effect
    inline_cleared: HashMap<NodeId, Vec<(String, String)>>,
    /// MotionPath specs (element, index) -> aligned, measured path and autoRotate offset
    motion_paths: HashMap<(NodeId, usize), Option<(crate::paths::RawPath, Option<f64>, f64, f64)>>,
    /// MorphSVG specs (element, index) -> equalized start/end shapes
    morphs: HashMap<(NodeId, usize), Option<crate::paths::Morph>>,
    /// `<canvas>` 2D surfaces by element, and calls recorded while scripts loaded
    #[cfg(feature = "skia")]
    canvases: HashMap<NodeId, crate::canvas::Canvas2D>,
    canvas_readback: CanvasReadback,
    /// canvases not in the document (offscreen buffers), by element ref, for drawImage/createPattern
    #[cfg(feature = "skia")]
    detached_canvases: HashMap<u64, crate::canvas::Canvas2D>,
    canvas_pending: Vec<Value>,
    /// decoded pixels of image sources Blitz never loaded, by src (WebGL textures and canvas images)
    #[cfg_attr(not(any(feature = "webgl", feature = "skia")), allow(dead_code))]
    src_images: Arc<SrcImages>,
    /// frames rendered so far, and the time being sought
    frames_rendered: u64,
    /// `Clock::Frames`: the page's clock when it finished loading (composition time 0)
    clock_epoch: Option<std::time::Instant>,
    seek_time: f64,
    /// scripts listen for the composition format's hf-seek event (or draw on canvases): dispatch it per frame
    seek_dispatch: bool,
    starts_sent: bool,
    prev_time: f64,
    renderer: Option<Raster>,
    video_nodes: Vec<(String, NodeId)>,
    video_shown: HashMap<String, Option<(usize, u64)>>,
    /// <video> nodes hidden because they have no frame at the current time
    video_hidden: std::collections::HashSet<NodeId>,
    pub root_comp_id: Option<String>,
    svg_targets: HashMap<NodeId, SvgTarget>,
    /// what a script's `tl.time(x)` samples (shared with the uvSample native)
    sample: SampleSlot,
}

type SrcImages = std::sync::Mutex<HashMap<String, Option<Arc<(u32, u32, Vec<u8>)>>>>;

/// Pixels of an `<img>` Blitz never loaded (scripts' detached `new Image()` / createElement images, e.g.
/// three.js textures): fetch the src relative to the document and decode it. Cached per composition.
fn decode_src(net: &SyncNet, html_path: &Path, cache: &SrcImages, src: &str) -> Option<(u32, u32, Vec<u8>)> {
    let hit = cache.lock().unwrap().get(src).cloned();
    let decoded = match hit {
        Some(d) => d,
        None => {
            let d = Url::parse(&file_url(html_path))
                .ok()
                .and_then(|base| base.join(src).ok())
                .and_then(|u| net.load(&u).ok())
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|img| {
                    let img = img.to_rgba8();
                    Arc::new((img.width(), img.height(), img.into_raw()))
                });
            cache.lock().unwrap().insert(src.to_string(), d.clone());
            d
        }
    };
    decoded.map(|d| (d.0, d.1, d.2.clone()))
}

/// SVG-only properties Servo's Stylo doesn't compute (so neither getComputedStyle nor the shim's
/// syncSvgStyles sees them), applied from the page's stylesheets as presentation attributes on SVG
/// elements that don't set them: `.axis-label { text-anchor: middle }` centres chart labels in Chrome.
fn apply_svg_only_css(doc: &mut BaseDocument) {
    const PROPS: &[&str] = &["text-anchor", "dominant-baseline", "alignment-baseline", "baseline-shift", "paint-order", "shape-rendering", "text-rendering", "marker-start", "marker-mid", "marker-end", "vector-effect"];
    let css: String = compile::all_elements(doc)
        .into_iter()
        .filter(|&id| compile::tag(doc, id) == "style")
        .filter_map(|id| doc.get_node(id).map(|n| n.text_content()))
        .filter(|t| PROPS.iter().any(|p| t.contains(p)))
        .collect::<Vec<_>>()
        .join("\n");
    if css.is_empty() {
        return;
    }
    #[derive(Clone, Copy)]
    struct El<'a>(&'a BaseDocument, NodeId);
    impl simplecss::Element for El<'_> {
        fn parent_element(&self) -> Option<Self> {
            let p = self.0.get_node(self.1)?.parent?;
            self.0.get_node(p)?.element_data()?;
            Some(El(self.0, p))
        }
        fn prev_sibling_element(&self) -> Option<Self> {
            let parent = self.0.get_node(self.0.get_node(self.1)?.parent?)?;
            let at = parent.children.iter().position(|&c| c == self.1)?;
            parent.children[..at].iter().rev().find(|&&c| self.0.get_node(c).is_some_and(|n| n.element_data().is_some())).map(|&c| El(self.0, c))
        }
        fn has_local_name(&self, name: &str) -> bool {
            compile::tag(self.0, self.1) == name
        }
        fn attribute_matches(&self, local_name: &str, operator: simplecss::AttributeOperator<'_>) -> bool {
            compile::attr(self.0, self.1, local_name).is_some_and(|v| operator.matches(&v))
        }
        fn pseudo_class_matches(&self, class: simplecss::PseudoClass<'_>) -> bool {
            matches!(class, simplecss::PseudoClass::FirstChild) && self.prev_sibling_element().is_none()
        }
    }
    let sheet = simplecss::StyleSheet::parse(&css);
    let mut rules: Vec<(usize, &simplecss::Rule<'_>)> = sheet.rules.iter().enumerate().filter(|(_, r)| r.declarations.iter().any(|d| PROPS.contains(&d.name))).collect();
    // cascade order: specificity, then source order
    rules.sort_by_key(|(i, r)| (r.selector.specificity(), *i));
    let svg_elements: Vec<NodeId> = compile::all_elements(doc)
        .into_iter()
        .filter(|&id| doc.get_node(id).and_then(|n| n.element_data()).is_some_and(|e| e.name.ns == blitz_dom::ns!(svg)))
        .collect();
    let mut writes: Vec<(NodeId, String, String)> = Vec::new();
    for &id in &svg_elements {
        let mut decided: HashMap<&str, &str> = HashMap::new();
        for (_, rule) in &rules {
            if rule.selector.matches(&El(doc, id)) {
                for d in &rule.declarations {
                    if PROPS.contains(&d.name) {
                        decided.insert(d.name, d.value);
                    }
                }
            }
        }
        for (prop, value) in decided {
            if compile::attr(doc, id, prop).is_none() && !value.contains("var(") {
                writes.push((id, prop.to_string(), value.trim().to_string()));
            }
        }
    }
    let mut m = doc.mutate();
    for (id, prop, value) in writes {
        m.set_attribute(id, compile::qual(&prop), &value);
    }
}

fn file_url(p: &Path) -> String {
    Url::from_file_path(p).map(|u| u.to_string()).unwrap_or_else(|_| format!("file://{}", p.display()))
}

impl Composition {
    /// Lint: timed media (`<video>`/`<audio>` with data-start) inside another timed element of the
    /// same composition. The two windows don't nest (each start is composition-relative and the
    /// wrapper's window also hides it), so such edits render black or silent.
    pub fn structure_errors(&self) -> Vec<String> {
        let inner = self.doc.inner();
        let doc: &BaseDocument = &inner;
        let describe = |id: NodeId, tag: &str| match compile::attr(doc, id, "id") {
            Some(i) => format!("<{tag} id=\"{i}\">"),
            None => format!("<{tag}>"),
        };
        let mut out = Vec::new();
        for id in compile::all_elements(doc) {
            let tag = if compile::has_attr(doc, id, "data-uv-video") { "video".to_string() } else { compile::tag(doc, id) };
            if !(tag == "video" || tag == "audio") || !compile::has_attr(doc, id, "data-start") {
                continue;
            }
            let mut cur = doc.get_node(id).and_then(|n| n.parent);
            while let Some(p) = cur {
                if compile::has_attr(doc, p, "data-composition-id") || compile::has_attr(doc, p, "data-composition-file") {
                    break;
                }
                if let Some(ps) = compile::attr(doc, p, "data-start") {
                    let ptag = compile::tag(doc, p);
                    let media_start = compile::attr(doc, id, "data-start").unwrap_or_default();
                    out.push(format!(
                        "{} (data-start=\"{media_start}\") is inside {} which also has data-start=\"{ps}\": timed media can't nest in a timed wrapper (both starts count from the composition, and the wrapper's window hides it), so it renders black or silent. Fix: remove data-start/data-duration from {}, or move the {tag} out of it.",
                        describe(id, &tag),
                        describe(p, &ptag),
                        describe(p, &ptag),
                    ));
                    break;
                }
                cur = doc.get_node(p).and_then(|n| n.parent);
            }
        }
        out
    }

    pub fn load(opts: Options) -> Result<Composition, String> {
        #[cfg(not(feature = "skia"))]
        if opts.backend == Backend::Skia {
            return Err("this build has no Skia backend (built without the `skia` feature); use the vello backend".into());
        }
        let html = match &opts.html_source {
            Some(html) => html.clone(),
            None => std::fs::read_to_string(&opts.html_path).map_err(|e| format!("{}: {e}", opts.html_path.display()))?,
        };
        let html = compile::mark_query_containers(&compile::mark_background_clip_text(&html));
        let net = Arc::new(SyncNet::new(&opts.project_dir, opts.allow_network));
        let (w0, h0) = opts.size.unwrap_or((1920, 1080));
        let config = DocumentConfig {
            base_url: Some(file_url(&opts.html_path)),
            net_provider: Some(net.clone() as _),
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            ua_stylesheets: Some(vec![blitz_dom::DEFAULT_CSS.to_string(), SVG_UA_CSS.to_string(), BGCLIP_UA_CSS.to_string(), CQ_UA_CSS.to_string()]),
            viewport: Some(Viewport::new(
                (w0 as f32 * opts.scale) as u32,
                (h0 as f32 * opts.scale) as u32,
                opts.scale,
                ColorScheme::Light,
            )),
            ..Default::default()
        };
        let mut doc = BaseDocument::new(config);
        {
            let mut m = doc.mutate();
            DocumentHtmlParser::parse_into_mutator(&mut m, &html);
        }
        let mut info = CompileInfo::default();
        compile::inline_sub_compositions(&mut doc, &opts.project_dir, &mut info);
        // The composition format's deterministic fonts: named families resolve to bundled/Google faces
        let font_notes = if opts.profile == Profile::Html { crate::fonts::inject(&mut doc, &net) } else { Vec::new() };
        info.warnings.extend(font_notes);
        // canvas text matches the document's font faces (Chrome); this thread draws the canvases
        #[cfg(feature = "skia")]
        {
            let css: Vec<String> = compile::all_elements(&doc)
                .into_iter()
                .filter(|&id| compile::tag(&doc, id) == "style")
                .map(|id| doc.get_node(id).map(|n| n.text_content()).unwrap_or_default())
                .collect();
            crate::canvas::set_document_fonts(&css, net.clone(), url::Url::from_file_path(&opts.html_path).ok());
        }
        let removed = compile::remove_gsap_scripts(&mut doc);
        let _ = removed;
        compile::replace_videos(&mut doc, &mut info);
        let root = compile::root_composition(&doc);
        let root_comp_id = root.and_then(|r| compile::attr(&doc, r, "data-composition-id"));
        let (width, height) = match opts.size {
            Some(s) => s,
            None => {
                let dim = |name: &str| {
                    root.and_then(|r| compile::attr(&doc, r, name)).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
                };
                (dim("data-width").unwrap_or(1920.0) as u32, dim("data-height").unwrap_or(1080.0) as u32)
            }
        };
        if let Some(r) = root {
            compile::apply_root_sizing(&mut doc, r);
        }
        doc.set_viewport(Viewport::new(
            (width as f32 * opts.scale).round() as u32,
            (height as f32 * opts.scale).round() as u32,
            opts.scale,
            ColorScheme::Light,
        ));
        let authored_duration = root
            .and_then(|r| compile::attr(&doc, r, "data-duration"))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|d| d.is_finite() && *d > 0.0);

        let src_images: Arc<SrcImages> = Default::default();
        // Scripts: variables, the shim, then the document's own scripts in order.
        let sdoc = ScriptDocument::from_base_document(doc)
            .without_timer_thread()
            .with_virtual_time()
            .with_fetcher(NetFetcher(net.clone()));
        let canvas_readback: CanvasReadback = Default::default();
        let script_fonts: ScriptFontsPending = Default::default();
        let sample: SampleSlot = Default::default();
        let mut sdoc = register_natives(sdoc, net.clone(), src_images.clone(), opts.html_path.clone(), canvas_readback.clone(), script_fonts.clone(), sample.clone());
        *script_fonts.lock().unwrap() = Some(declared_web_fonts(&sdoc.inner()));
        let mut prelude = String::new();
        if let Some(vars) = &opts.variables {
            prelude.push_str(&format!("window.__hfVariables = {};\n", vars));
        }
        sdoc.eval(&prelude);
        sdoc.eval(SHIM);
        // Chrome loads a web font once layout first needs it: faces the page's own text uses are ready by the time
        // its scripts run (the inline data fonts the HTML composition format injects decode at once), but a family only script-made
        // text uses is still loading when the script measures it, so that text is laid out in the next available
        // family; the HTML composition format captures after the fonts load
        {
            let mut pending = script_fonts.lock().unwrap().clone().unwrap_or_default();
            let mut inner = sdoc.inner_mut();
            inner.resolve(0.0);
            for used in web_fonts_in_use(&inner, &pending) {
                pending.remove(&used);
            }
            inner.set_pending_font_families(Some(pending));
        }
        sdoc.execute_scripts();
        sdoc.inner_mut().set_pending_font_families(None);
        // The composition format's runtime init lays out the root and its timed clips after the page's scripts
        sdoc.eval("if (window.__uvVariableBindings) window.__uvVariableBindings(); if (window.__uvClipLayout) window.__uvClipLayout();");
        // data-fx effects: run effects/fx.js when the composition didn't include it, then expand (a no-op
        // when the composition's own copy already did)
        sdoc.eval(&format!(
            "if (!window.__uvFx && document.querySelector('[data-fx]')) {{\n{}\n}}\nif (window.__uvFx) {{ window.__uvFx.expand(); window.__uvFx.seek(0); }}",
            crate::effects::FX_JS
        ));
        // Let timers / rAF-deferred setup run (virtual clock, bounded). With Clock::Frames only what's due now:
        // a timer runs at the composition time it's due, the frames advance the clock (seek)
        let frames_clock = opts.clock == Clock::Frames
            || sdoc.inner().query_selector("meta[name=raea-clock][content=frames]").ok().flatten().is_some();
        let start = sdoc.clock_now();
        for _ in 0..2000 {
            let Some(deadline) = sdoc.next_timer_deadline() else { break };
            if deadline.duration_since(start).as_secs_f64() > if frames_clock { 0.0 } else { 5.0 } {
                break;
            }
            sdoc.advance_clock_to(deadline);
            sdoc.poll(None);
        }
        let clock_epoch = frames_clock.then(|| sdoc.clock_now());
        sdoc.eval("__hfb.flush(); __hfb.syncSvgStyles();");
        apply_svg_only_css(&mut sdoc.inner_mut());
        *script_fonts.lock().unwrap() = None;

        let mut warnings: Vec<String> = info.warnings.clone();
        let mut unsupported = Vec::new();
        let mut tree: Option<Node> = None;
        let mut named: BTreeMap<String, u64> = BTreeMap::new();
        let mut canvas_pending: Vec<Value> = Vec::new();
        let mut seek_dispatch = false;
        for msg in sdoc.take_messages() {
            let Ok(v) = serde_json::from_str::<Value>(&msg) else { continue };
            match v.get("type").and_then(Value::as_str) {
                Some("canvas") | Some("webgl") => {
                    seek_dispatch = true;
                    canvas_pending.push(v);
                }
                Some("seek-listener") => seek_dispatch = true,
                Some("timelines") => {
                    tree = v.get("root").cloned().and_then(|r| serde_json::from_value(r).map_err(|e| warnings.push(format!("timeline tree: {e}"))).ok());
                    if let Some(obj) = v.get("named").and_then(Value::as_object) {
                        for (k, id) in obj {
                            if let Some(id) = id.as_u64() {
                                named.insert(k.clone(), id);
                            }
                        }
                    }
                }
                Some("warn") => {
                    let m = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
                    if m.contains("not supported") { unsupported.push(m) } else { warnings.push(m) }
                }
                Some("unsupported") => unsupported.push(v.get("message").and_then(Value::as_str).unwrap_or("").to_string()),
                _ => {}
            }
        }
        // The HTML composition format color grading runs in its runtime's WebGL shader (core/src/runtime/colorGrading.ts)
        {
            let inner = sdoc.inner();
            if compile::all_elements(&inner).into_iter().any(|id| compile::has_attr(&inner, id, "data-color-grading")) {
                unsupported.push("data-color-grading (the HTML composition format color grading) is not supported: the media renders ungraded".into());
            }
        }
        let raw_errors = sdoc.take_js_errors();
        let js_errors = locate_script_errors(&sdoc.inner(), raw_errors, &opts.html_path);
        for (authored, runtime) in &info.aliases {
            if let Some(id) = named.get(authored).copied() {
                named.entry(runtime.clone()).or_insert(id);
            }
        }

        // Initial style + layout pass (needed for the timing in-flow checks and base transforms).
        {
            let mut inner = sdoc.inner_mut();
            for _ in 0..4 {
                inner.resolve(0.0);
            }
        }

        let global = match tree {
            Some(Node::Timeline(t)) => t,
            _ => gsap::TimelineNode {
                id: 0,
                start: 0.0,
                duration: 0.0,
                repeat: 0,
                yoyo: false,
                repeat_delay: 0.0,
                time_scale: 1.0,
                paused: false,
                callbacks: HashMap::new(),
                children: vec![],
            },
        };
        // Durations of every registered timeline (compositions without authored timing use them).
        let global_node = Node::Timeline(global.clone());
        let mut timeline_durations: HashMap<String, f64> = HashMap::new();
        for (k, id) in &named {
            if let Some(n) = global_node.find(*id) {
                timeline_durations.insert(k.clone(), n.total_duration());
            }
        }
        let media_windows = opts.videos.windows();

        // Root timeline: __timelines[root id], else the only registered one.
        let root_tl = root_comp_id
            .as_ref()
            .and_then(|id| named.get(id).copied())
            .or_else(|| if named.len() == 1 { named.values().next().copied() } else { None });
        if root_tl.is_none() && !named.is_empty() {
            warnings.push(format!(
                "no timeline registered for the root composition {:?}; nesting the {} sub-composition timelines",
                root_comp_id.as_deref().unwrap_or("?"),
                named.len()
            ));
        }
        let root_tl_node = root_tl.and_then(|id| global.children.iter().find(|c| c.id() == id).cloned());
        let timeline_duration = root_tl_node.as_ref().map(|n| n.total_duration()).unwrap_or(0.0);

        // Sub-composition timelines the runtime nests at their host's start.
        let mut nested: HashMap<u64, f64> = HashMap::new();
        {
            let inner = sdoc.inner();
            let mut resolver = Resolver::new(&inner, &timeline_durations, &media_windows);
            let root_el = compile::root_composition(&inner);
            let comps: Vec<NodeId> = match root_el {
                Some(r) => compile::elements_under(&inner, r).into_iter().skip(1).filter(|&id| compile::has_attr(&inner, id, "data-composition-id")).collect(),
                None => vec![],
            };
            let mut seen = Vec::new();
            for c in comps {
                let Some(cid) = compile::attr(&inner, c, "data-composition-id") else { continue };
                if seen.contains(&cid) || Some(&cid) == root_comp_id.as_ref() {
                    continue;
                }
                seen.push(cid.clone());
                let Some(&tid) = named.get(&cid) else { continue };
                if Some(tid) == root_tl {
                    continue;
                }
                // already placed by the author somewhere inside the root timeline?
                let inside_root = root_tl_node.as_ref().is_some_and(|r| r.find(tid).is_some());
                if inside_root {
                    continue;
                }
                // a mounted file's root that kept its own id shares its host's timeline (an alias): the
                // host, first in document order, places it
                if !nested.contains_key(&tid) && global.children.iter().any(|ch| ch.id() == tid) {
                    nested.insert(tid, resolver.start(c, 0.0));
                }
            }
        }

        let mut builder = Builder::new();
        for child in &global.children {
            let id = child.id();
            let drive = if Some(id) == root_tl {
                Drive::Root
            } else if let Some(at) = nested.get(&id) {
                Drive::NestedAt(*at)
            } else {
                let paused = matches!(child, Node::Timeline(t) if t.paused);
                if paused { Drive::Frozen } else { Drive::Free }
            };
            builder.add_top(child, drive, timeline_duration);
        }
        warnings.extend(builder.warnings.drain(..));
        let duration = authored_duration.unwrap_or_else(|| {
            let nested_end = nested
                .iter()
                .filter_map(|(id, at)| global.children.iter().find(|c| c.id() == *id).map(|n| at + n.total_duration()))
                .fold(0.0, f64::max);
            let d = timeline_duration.max(nested_end);
            if d.is_finite() && d < 7200.0 { d } else { 0.0 }
        });
        let anim = builder.finish(duration);
        if crate::env_os("DEBUG").is_some() {
            eprintln!("[debug] named timelines: {named:?}");
            eprintln!("[debug] root timeline: {root_tl:?} nested: {nested:?} duration {duration} timeline {timeline_duration}");
            for c in &global.children {
                let kind = match c { Node::Timeline(t) => format!("timeline paused={} children={}", t.paused, t.children.len()), Node::Tween(t) => format!("tween items={}", t.items.len()), Node::Call(_) => "call".into() };
                eprintln!("[debug]   top {} start {} {kind}", c.id(), c.start());
            }
            for t in &anim.tracks {
                eprintln!("[debug]   track {:?} {} base {:?}", t.target, t.prop, t.base.to_css());
            }
        }

        let timing = {
            let inner = sdoc.inner();
            let root_el = compile::root_composition(&inner);
            Timing::build(&inner, root_el, opts.fps_f64(), duration, &timeline_durations, &media_windows)
        };
        // CSS animations run on clip-local time: currentTime = t - start of the closest [data-start]
        sdoc.inner_mut().set_animation_origins(Some(timing.timed.iter().map(|c| (c.node, c.origin)).collect()));

        // data-hfb ref -> node, video nodes
        let mut elements = HashMap::new();
        let mut video_nodes = Vec::new();
        {
            let inner = sdoc.inner();
            for id in compile::all_elements(&inner) {
                if let Some(r) = compile::attr(&inner, id, "data-hfb").and_then(|v| v.parse::<u64>().ok()) {
                    elements.insert(r, id);
                }
                if let Some(v) = compile::attr(&inner, id, "data-uv-video") {
                    video_nodes.push((v, id));
                }
            }
        }
        if crate::env_os("DEBUG").is_some() {
            let missing: std::collections::BTreeSet<u64> = anim.tracks.iter().filter_map(|t| match t.target { Target::El(r) if !elements.contains_key(&r) => Some(r), _ => None }).collect();
            if !missing.is_empty() {
                eprintln!("[debug] track targets without an element in the document: {missing:?}");
            }
            let inner = sdoc.inner();
            let mut refs: Vec<_> = elements.iter().collect();
            refs.sort();
            for (r, n) in refs {
                eprintln!("[debug]   El({r}) = {n:?} <{}#{}>", compile::tag(&inner, *n), compile::attr(&inner, *n, "id").unwrap_or_default());
            }
        }
        if !opts.videos.entries.is_empty() {
            for v in &info.videos {
                if !opts.videos.has(v) {
                    warnings.push(format!("no frames for video {v}: it renders empty"));
                }
            }
        }
        warnings.extend(net.warnings.lock().unwrap().drain(..));

        let mut opts = opts;
        if opts.backend == Backend::Auto {
            let (wants_skia, backdrop) = skia_features(&sdoc.inner());
            // the GPU paints everything vello_cpu does, to the level (src/quads); Skia where Chrome's filters need it
            opts.backend = if wants_skia && cfg!(feature = "skia") { Backend::Skia } else if crate::quads::enabled() { Backend::Gpu } else { Backend::Vello };
            if backdrop && !cfg!(feature = "skia") {
                warnings.push("backdrop-filter is not painted: this build has no Skia backend (filters, filter: url(#id) and data-fx effects are)".into());
            }
        }
        let mut comp = Composition {
            doc: sdoc,
            net,
            anim: std::rc::Rc::new(std::cell::RefCell::new(anim)),
            timing,
            width,
            height,
            duration,
            authored_duration,
            timeline_duration,
            warnings,
            js_errors,
            unsupported,
            opts,
            elements,
            transforms: HashMap::new(),
            applied: HashMap::new(),
            inline_cleared: HashMap::new(),
            motion_paths: HashMap::new(),
            morphs: HashMap::new(),
            #[cfg(feature = "skia")]
            canvases: HashMap::new(),
            canvas_readback,
            #[cfg(feature = "skia")]
            detached_canvases: HashMap::new(),
            src_images,
            frames_rendered: 0,
            clock_epoch,
            seek_time: 0.0,
            canvas_pending,
            seek_dispatch,
            starts_sent: false,
            prev_time: -1e-9,
            renderer: None,
            video_nodes,
            video_shown: HashMap::new(),
            video_hidden: Default::default(),
            root_comp_id,
            svg_targets: HashMap::new(),
            sample: sample.clone(),
        };
        comp.init_transforms();
        comp.reparse_inline_svgs();
        let bases = comp.elements.iter().filter_map(|(&r, n)| comp.transforms.get(n).map(|b| (r, b.clone()))).collect();
        let html = comp.elements.iter().filter(|(_, n)| !comp.svg_targets.contains_key(*n)).map(|(&r, _)| r).collect();
        *sample.borrow_mut() = Some(SampleCtx { anim: comp.anim.clone(), bases, html, frame_time: 0.0 });
        Ok(comp)
    }

    /// SVG descendants GSAP animates: their outermost <svg>, bbox and CSS transform-origin.
    fn init_svg_targets(&mut self) {
        let mut svg_style_moves: Vec<(NodeId, String, Vec<(String, String)>)> = Vec::new();
        let inner = self.doc.inner();
        let mut nodes: Vec<NodeId> = Vec::new();
        for track in &self.anim.clone().borrow().tracks {
            if let Target::El(r) = track.target {
                if let Some(&n) = self.elements.get(&r) {
                    if !nodes.contains(&n) {
                        nodes.push(n);
                    }
                }
            }
        }
        for n in nodes {
            let Some(el) = inner.get_node(n).and_then(|x| x.element_data()) else { continue };
            if el.name.ns != blitz_dom::ns!(svg) {
                continue;
            }
            // outermost <svg> ancestor; the outermost <svg> itself is a CSS box, not a target
            let mut root = None;
            let mut cur = inner.get_node(n).and_then(|x| x.parent);
            while let Some(p) = cur {
                let Some(pe) = inner.get_node(p).and_then(|x| x.element_data()) else { break };
                if pe.name.ns != blitz_dom::ns!(svg) {
                    break;
                }
                if &*pe.name.local == "svg" {
                    root = Some(p);
                }
                cur = inner.get_node(p).and_then(|x| x.parent);
            }
            let Some(root) = root else { continue };
            let bbox = svg_bbox(&inner, n).unwrap_or([0.0; 4]);
            // view box size for resolving percentage origins (transform-box: view-box)
            let vb = compile::attr(&inner, root, "viewBox").map(|v| parse_numbers(&v)).filter(|v| v.len() == 4);
            let (vw, vh) = match vb {
                Some(v) => (v[2], v[3]),
                None => {
                    let size = inner.get_node(root).map(|x| x.final_layout().size).unwrap_or_default();
                    (size.width as f64, size.height as f64)
                }
            };
            let css_origin = inner
                .get_node(n)
                .and_then(|x| x.primary_styles())
                .map(|s| {
                    let o = &s.get_box().transform_origin;
                    let ox = o.horizontal.resolve(style::values::computed::CSSPixelLength::new(vw as f32)).px() as f64;
                    let oy = o.vertical.resolve(style::values::computed::CSSPixelLength::new(vh as f32)).px() as f64;
                    (ox, oy)
                })
                .unwrap_or((0.0, 0.0));
            let base = compile::attr(&inner, n, "transform").map(|t| TransformState::from_affine(parse_svg_transform(&t))).unwrap_or_else(TransformState::identity);
            self.transforms.insert(n, base);
            let mut orig: HashMap<String, String> = el.attrs().iter().map(|a| (a.name.local.to_string(), a.value.clone())).collect();
            // In SVG the style attribute beats presentation attributes, so declarations a script
            // put in `style` would mask the attributes we animate: move them into attributes.
            if let Some(style) = orig.get("style").cloned() {
                let mut keep = Vec::new();
                let mut moved = Vec::new();
                for decl in style.split(';') {
                    let Some((k, v)) = decl.split_once(':') else { continue };
                    let k = k.trim().to_ascii_lowercase();
                    if SVG_PRESENTATION.contains(&k.as_str()) {
                        moved.push((k, v.trim().to_string()));
                    } else if !k.is_empty() {
                        keep.push(format!("{k}: {}", v.trim()));
                    }
                }
                if !moved.is_empty() {
                    svg_style_moves.push((n, keep.join("; "), moved.clone()));
                    for (k, v) in moved {
                        orig.insert(k, v);
                    }
                }
            }
            self.svg_targets.insert(n, SvgTarget { root, bbox, css_origin, orig });
        }
        drop(inner);
        if !svg_style_moves.is_empty() {
            let mut inner = self.doc.inner_mut();
            let mut m = inner.mutate();
            for (node, keep, moved) in svg_style_moves {
                for (k, v) in moved {
                    m.set_attribute(node, compile::qual(&k), v.trim_end_matches("px"));
                }
                if keep.is_empty() {
                    m.clear_attribute(node, compile::qual("style"));
                } else {
                    m.set_attribute(node, compile::qual("style"), &keep);
                }
            }
        }
    }

    /// Base transform components of every element GSAP transforms, from its computed transform.
    fn init_transforms(&mut self) {
        self.init_svg_targets();
        let mut els: Vec<NodeId> = Vec::new();
        for track in &self.anim.clone().borrow().tracks {
            if let Target::El(r) = track.target {
                if is_transform_prop(&track.prop) {
                    if let Some(&n) = self.elements.get(&r) {
                        if !els.contains(&n) {
                            els.push(n);
                        }
                    }
                }
            }
        }
        let inner = self.doc.inner();
        for n in els {
            if self.svg_targets.contains_key(&n) {
                continue;
            }
            let Some(node) = inner.get_node(n) else { continue };
            let state = node
                .primary_styles()
                .and_then(|s| {
                    let size = node.final_layout().size;
                    let reference = euclid::Rect::new(
                        euclid::Point2D::new(style::values::computed::CSSPixelLength::new(0.0), style::values::computed::CSSPixelLength::new(0.0)),
                        euclid::Size2D::new(
                            style::values::computed::CSSPixelLength::new(size.width),
                            style::values::computed::CSSPixelLength::new(size.height),
                        ),
                    );
                    // a 3D `transform` (GSAP reads the computed matrix3d)
                    if !s.get_box().transform.0.is_empty() {
                        if let Ok((t, _)) = s.get_box().transform.to_transform_3d_matrix_f64(Some(&reference)) {
                            if !t.is_2d() {
                                return Some(TransformState::from_matrix3d(&t).detect_percent(t.m41 as f32, t.m42 as f32, size.width.round(), size.height.round()));
                            }
                        }
                    }
                    let m = blitz_dom::resolve_2d_transform(s.get_box(), reference)?;
                    // un-bake transform-origin: M = T(o) M0 T(-o)
                    let o = &s.get_box().transform_origin;
                    let ox = o.horizontal.resolve(style::values::computed::CSSPixelLength::new(size.width)).px() as f64;
                    let oy = o.vertical.resolve(style::values::computed::CSSPixelLength::new(size.height)).px() as f64;
                    let [a, b, c, d, e, f] = m.as_coeffs();
                    let e0 = e - ox + a * ox + c * oy;
                    let f0 = f - oy + b * ox + d * oy;
                    Some(TransformState::from_affine(Affine::new([a, b, c, d, e0, f0])).detect_percent(e0 as f32, f0 as f32, size.width.round(), size.height.round()))
                })
                .unwrap_or_else(TransformState::identity);
            self.transforms.insert(n, state);
        }
        drop(inner);
        // GSAP reads a transform tween's start from the element's decomposed transform.
        let by_node: HashMap<u64, NodeId> = self.elements.clone();
        for track in &mut self.anim.clone().borrow_mut().tracks {
            let Target::El(r) = track.target else { continue };
            if !is_transform_prop(&track.prop) {
                continue;
            }
            let Some(state) = by_node.get(&r).and_then(|n| self.transforms.get(n)) else { continue };
            let base = match track.prop.as_str() {
                "x" => state.x.clone(),
                "y" => state.y.clone(),
                "z" => state.z.clone(),
                "xPercent" => CssValue::num(state.x_percent, ""),
                "yPercent" => CssValue::num(state.y_percent, ""),
                "rotation" => CssValue::num(state.rotation, "deg"),
                "rotationX" => CssValue::num(state.rotation_x, "deg"),
                "rotationY" => CssValue::num(state.rotation_y, "deg"),
                "skewX" => CssValue::num(state.skew_x, "deg"),
                "skewY" => CssValue::num(state.skew_y, "deg"),
                "scaleX" => CssValue::num(state.scale_x, ""),
                "scaleY" => CssValue::num(state.scale_y, ""),
                _ => continue,
            };
            track.set_base(base);
        }
    }

    fn set_css(m: &mut blitz_dom::DocumentMutator<'_>, applied: &mut HashMap<(NodeId, String), String>, node: NodeId, prop: &str, value: Option<&str>) {
        let key = (node, prop.to_string());
        if crate::env_os("DEBUG_APPLY").is_some() {
            eprintln!("[apply] {node:?} {prop}: {value:?}");
        }
        match value {
            Some(v) => {
                if applied.get(&key).map(String::as_str) == Some(v) {
                    return;
                }
                m.set_style_property(node, prop, v);
                applied.insert(key, v.to_string());
            }
            None => {
                if applied.remove(&key).is_some() {
                    m.remove_style_property(node, prop);
                }
            }
        }
    }

    /// Bring the document to time `t` (seconds) without painting.
    pub fn seek(&mut self, t: f64) {
        self.seek_time = t;
        // Clock::Frames: the page's clocks advanced to this frame's time first, every timer and requestAnimationFrame
        // callback due by then run in order, before its timelines and CSS animations are seeked
        if let Some(epoch) = self.clock_epoch {
            // as a browser's frame does: animations updated (and their finish events sent) at this frame's time
            // before its requestAnimationFrame callbacks run (motion renders an animation's last value from its frame
            // loop when the native one finishes)
            if self.seek_dispatch {
                self.doc.eval(&format!("if (typeof __raeaFrame === 'function') __raeaFrame({t});"));
            }
            let target = epoch + std::time::Duration::from_secs_f64(t.max(0.0));
            for _ in 0..100_000 {
                let Some(deadline) = self.doc.next_timer_deadline() else { break };
                if deadline > target {
                    break;
                }
                self.doc.advance_clock_to(deadline);
                self.doc.poll(None);
            }
            self.doc.advance_clock_to(target);
        }
        // canvases drawn while the page loaded are replayed first, before this frame's GSAP callbacks and
        // seek handlers run, so their getImageData / createPattern / WebGL queries see them
        #[cfg(feature = "skia")]
        if !self.canvas_pending.is_empty() {
            let load = std::mem::take(&mut self.canvas_pending);
            self.apply_canvases(load);
        }
        let update = self.anim.borrow_mut().frame(self.prev_time, t);
        self.prev_time = t;
        if let Some(ctx) = self.sample.borrow_mut().as_mut() {
            ctx.frame_time = t;
        }

        // group by element
        let mut transform_changes: HashMap<NodeId, Vec<(String, Option<CssValue>)>> = HashMap::new();
        let mut svg_transform_changes: HashMap<NodeId, Vec<(String, Option<CssValue>)>> = HashMap::new();
        // MotionPath values go after the element's own x/y/rotation tracks
        let mut motion_changes: Vec<(NodeId, Vec<(String, Option<CssValue>)>)> = Vec::new();
        let mut svg_origins: HashMap<NodeId, (bool, String)> = HashMap::new();
        let mut dirty_svgs: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
        let mut js_objects: BTreeMap<u64, serde_json::Map<String, Value>> = BTreeMap::new();
        let mut js_elements: BTreeMap<u64, serde_json::Map<String, Value>> = BTreeMap::new();
        {
            let mut inner = self.doc.inner_mut();
            let mut m = inner.mutate();
            let anim_rc = self.anim.clone();
            let anim = anim_rc.borrow();
            for (ti, value) in &update.values {
                let track = &anim.tracks[*ti];
                match track.target {
                    Target::Obj(r) => {
                        if let Some(v) = value {
                            js_objects.entry(r).or_default().insert(track.prop.clone(), v.to_json());
                        }
                    }
                    Target::El(r) => {
                        let Some(&node) = self.elements.get(&r) else { continue };
                        let prop = track.prop.as_str();
                        if let Some(k) = prop.strip_prefix("--uv-motion-").and_then(|k| k.parse::<usize>().ok()) {
                            let Some(p) = value.as_ref().and_then(|v| v.as_f32()) else { continue };
                            let elements = &self.elements;
                            let built = self.motion_paths.entry((node, k)).or_insert_with(|| motion_path(m.doc, node, k, elements));
                            if let Some((path, rotate, start, end)) = built {
                                // start/end slice the path (GSAP's sliceRawPath): progress runs from start
                                // to end along it, wrapping past either end and backwards when end < start
                                let mut q = *start + (*end - *start) * (p as f64).clamp(0.0, 1.0);
                                if !(0.0..=1.0).contains(&q) {
                                    q = q.rem_euclid(1.0);
                                }
                                let (x, y, mut angle) = crate::paths::position(path, q);
                                if end < start {
                                    angle += 180.0;
                                }
                                let mut changes = vec![("x".to_string(), Some(CssValue::num(x as f32, "px"))), ("y".to_string(), Some(CssValue::num(y as f32, "px")))];
                                if let Some(off) = rotate {
                                    changes.push(("rotation".to_string(), Some(CssValue::num((angle + *off) as f32, "deg"))));
                                }
                                motion_changes.push((node, changes));
                            }
                            continue;
                        }
                        if prop == "__inline" {
                            // clearProps "all": GSAP empties the style attribute, authored declarations too
                            match (value.is_some(), self.inline_cleared.contains_key(&node)) {
                                (true, false) => {
                                    let style = compile::attr(m.doc, node, "style").unwrap_or_default();
                                    let decls: Vec<(String, String)> = style
                                        .split(';')
                                        .filter_map(|d| d.split_once(':'))
                                        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                                        .filter(|(k, _)| !k.is_empty() && !self.applied.contains_key(&(node, k.clone())))
                                        .collect();
                                    for (k, _) in &decls {
                                        m.remove_style_property(node, k);
                                    }
                                    self.inline_cleared.insert(node, decls);
                                }
                                (false, true) => {
                                    for (k, v) in self.inline_cleared.remove(&node).unwrap_or_default() {
                                        m.set_style_property(node, &k, &v);
                                    }
                                }
                                _ => {}
                            }
                            continue;
                        }
                        if let Some(svg) = self.svg_targets.get(&node) {
                            // SVG child: everything goes through presentation attributes
                            let root = svg.root;
                            if is_transform_prop(prop) {
                                svg_transform_changes.entry(node).or_default().push((prop.to_string(), value.clone()));
                                continue;
                            }
                            if prop == "transformOrigin" || prop == "svgOrigin" {
                                if let Some(v) = value {
                                    svg_origins.insert(node, (prop == "svgOrigin", v.to_css()));
                                }
                                svg_transform_changes.entry(node).or_default();
                                continue;
                            }
                            let mut writes: Vec<(String, Option<String>)> = Vec::new();
                            if let Some(name) = prop.strip_prefix("attr:") {
                                writes.push((name.to_string(), value.as_ref().map(|v| v.to_css())));
                            } else if prop == "autoAlpha" {
                                let hidden = value.as_ref().and_then(|v| v.as_f32()).is_some_and(|o| o == 0.0);
                                writes.push(("opacity".into(), value.as_ref().map(|v| v.to_css())));
                                writes.push(("visibility".into(), value.as_ref().map(|_| if hidden { "hidden".to_string() } else { "visible".to_string() })));
                            } else if is_dom_prop(prop) {
                                let v = value.as_ref().map(|v| v.to_css()).unwrap_or_else(|| track.inline.clone().unwrap_or_default());
                                js_elements.entry(r).or_default().insert(prop.to_string(), Value::String(v));
                            } else if let Some(k) = prop.strip_prefix("--uv-morph-").and_then(|k| k.parse::<usize>().ok()) {
                                let morph = self.morphs.entry((node, k)).or_insert_with(|| svg_morph(m.doc, node, k, svg.orig.get("d").map(String::as_str)));
                                let d = match (value.as_ref().and_then(|v| v.as_f32()), morph) {
                                    (Some(p), Some(morph)) => Some(morph.at(p as f64)),
                                    _ => None,
                                };
                                writes.push(("d".into(), d));
                            } else if prop == "--uv-draw" {
                                let (array, offset) = match value.as_ref().map(draw_svg_dash) {
                                    Some((a, o)) => (Some(a), Some(o)),
                                    None => (None, None),
                                };
                                writes.push(("stroke-dasharray".into(), array));
                                writes.push(("stroke-dashoffset".into(), offset));
                            } else {
                                writes.push((kebab(prop), value.as_ref().map(|v| v.to_css())));
                            }
                            for (name, v) in writes {
                                let v = v.or_else(|| svg.orig.get(&name).cloned());
                                // user units: SVG attributes take bare numbers for lengths
                                let v = v.map(|v| match v.strip_suffix("px") {
                                    Some(n) if n.trim().parse::<f64>().is_ok() => n.trim().to_string(),
                                    _ => v,
                                });
                                let key = (node, format!("svgattr:{name}"));
                                let cur = self.applied.get(&key).cloned();
                                match v {
                                    Some(v) if cur.as_deref() != Some(v.as_str()) => {
                                        m.set_attribute(node, compile::qual(&name), &v);
                                        self.applied.insert(key, v);
                                        dirty_svgs.insert(root);
                                    }
                                    None if cur.is_some() => {
                                        m.clear_attribute(node, compile::qual(&name));
                                        self.applied.remove(&key);
                                        dirty_svgs.insert(root);
                                    }
                                    _ => {}
                                }
                            }
                            continue;
                        }
                        if is_transform_prop(prop) {
                            transform_changes.entry(node).or_default().push((prop.to_string(), value.clone()));
                        } else if let Some(k) = prop.strip_prefix("--uv-text-").and_then(|k| k.parse::<usize>().ok()) {
                            // TextPlugin: the plugin's string for this progress
                            let text = value.as_ref().and_then(|v| v.as_f32()).and_then(|p| text_plugin_at(m.doc, node, k, p as f64));
                            if let Some((text, svg)) = text {
                                let dom = if svg { "textContent" } else { "innerHTML" };
                                let key = (node, format!("dom:{dom}"));
                                if self.applied.get(&key) != Some(&text) {
                                    self.applied.insert(key, text.clone());
                                    js_elements.entry(r).or_default().insert(dom.to_string(), Value::String(text));
                                }
                            }
                        } else if is_dom_prop(prop) {
                            let v = value.as_ref().map(|v| v.to_css()).unwrap_or_else(|| track.inline.clone().unwrap_or_default());
                            let key = (node, format!("dom:{prop}"));
                            if self.applied.get(&key) != Some(&v) {
                                self.applied.insert(key, v.clone());
                                js_elements.entry(r).or_default().insert(prop.to_string(), Value::String(v));
                            }
                        } else if let Some(name) = prop.strip_prefix("attr:") {
                            if let Some(v) = value {
                                let css = v.to_css();
                                let key = (node, format!("attr:{name}"));
                                if self.applied.get(&key) != Some(&css) {
                                    m.set_attribute(node, compile::qual(name), &css);
                                    self.applied.insert(key, css);
                                }
                            }
                        } else if prop == "autoAlpha" {
                            let css = value.as_ref().map(|v| v.to_css());
                            let hidden = value.as_ref().and_then(|v| v.as_f32()).is_some_and(|o| o == 0.0);
                            Self::set_css(&mut m, &mut self.applied, node, "opacity", css.as_deref());
                            Self::set_css(&mut m, &mut self.applied, node, "visibility", value.as_ref().map(|_| if hidden { "hidden" } else { "inherit" }));
                        } else if prop == "transformOrigin" {
                            let css = value.as_ref().map(|v| v.to_css());
                            Self::set_css(&mut m, &mut self.applied, node, "transform-origin", css.as_deref());
                        } else {
                            let name = kebab(prop);
                            match value {
                                Some(v) => Self::set_css(&mut m, &mut self.applied, node, &name, Some(&v.to_css())),
                                None => match &track.inline {
                                    Some(orig) if !orig.is_empty() => Self::set_css(&mut m, &mut self.applied, node, &name, Some(orig)),
                                    _ => Self::set_css(&mut m, &mut self.applied, node, &name, None),
                                },
                            }
                        }
                    }
                }
            }
            for (node, changes) in svg_transform_changes {
                let Some(svg) = self.svg_targets.get(&node) else { continue };
                let mut state = self.transforms.get(&node).cloned().unwrap_or_else(TransformState::identity);
                let mut any = svg_origins.contains_key(&node);
                for (prop, v) in &changes {
                    if let Some(v) = v {
                        state.set(prop, v);
                        any = true;
                    }
                }
                let origin = match svg_origins.get(&node) {
                    Some((true, abs)) => {
                        let v = parse_numbers(&abs.replace("px", " "));
                        (v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0))
                    }
                    Some((false, rel)) => svg_origin_from(rel, svg.bbox),
                    None => (svg.bbox[0] + svg.css_origin.0, svg.bbox[1] + svg.css_origin.1),
                };
                let value = if any { Some(svg_matrix(&state, origin, svg.bbox)) } else { svg.orig.get("transform").cloned() };
                let key = (node, "svgattr:transform".to_string());
                if self.applied.get(&key) != value.as_ref() {
                    match &value {
                        Some(v) => m.set_attribute(node, compile::qual("transform"), v),
                        None => m.clear_attribute(node, compile::qual("transform")),
                    }
                    match value {
                        Some(v) => self.applied.insert(key, v),
                        None => self.applied.remove(&key),
                    };
                    dirty_svgs.insert(svg.root);
                }
            }
            for (node, changes) in motion_changes {
                transform_changes.entry(node).or_default().extend(changes);
            }
            for (node, changes) in transform_changes {
                let mut state = self.transforms.get(&node).cloned().unwrap_or_else(TransformState::identity);
                let mut any = false;
                for (prop, v) in &changes {
                    if let Some(v) = v {
                        state.set(prop, v);
                        any = true;
                    }
                }
                let css = state.to_css();
                if any {
                    Self::set_css(&mut m, &mut self.applied, node, "transform", Some(&css));
                } else {
                    Self::set_css(&mut m, &mut self.applied, node, "transform", None);
                }
            }
        }

        if !dirty_svgs.is_empty() {
            let mut inner = self.doc.inner_mut();
            for root in dirty_svgs {
                if let Some(n) = inner.get_node_mut(root) {
                    n.clear_layout_cache();
                    n.insert_damage(style::selector_parser::RestyleDamage::from_bits_retain(0b1110_1111));
                }
            }
        }

        if !self.starts_sent {
            // composition id -> start: registered timelines report their local time to callbacks (and
            // The composition format's Lottie adapter seeks to it)
            let inner = self.doc.inner();
            let mut starts = serde_json::Map::new();
            for c in &self.timing.timed {
                if let Some(id) = compile::attr(&inner, c.node, "data-composition-id") {
                    starts.entry(id).or_insert(json!(c.origin));
                }
            }
            drop(inner);
            self.doc.eval(&format!("__hfb.compositionStarts({})", Value::Object(starts)));
            self.starts_sent = true;
        }
        if !js_objects.is_empty() || !update.callbacks.is_empty() || !js_elements.is_empty() {
            let objs: Vec<Value> = js_objects.into_iter().map(|(k, v)| json!([k, v])).collect();
            let els: Vec<Value> = js_elements.into_iter().map(|(k, v)| json!([k, v])).collect();
            let code = format!("__hfb.frame({}, {}, {}, {t})", Value::Array(objs), json!(update.callbacks), Value::Array(els));
            if crate::env_os("DEBUG").is_some() {
                eprintln!("[debug] t={t} js: {}", if code.len() > 400 { &code[..400] } else { &code });
            }
            // A script style write (a callback's gsap.set, el.style.x = ...) re-parses the style attribute,
            // which doesn't carry the values applied here, so it drops them (play-mode's onStart set the
            // transform-origin and the card stayed at its authored opacity 0). Remember the attributes...
            let before: HashMap<NodeId, String> = {
                let inner = self.doc.inner();
                let nodes: std::collections::HashSet<NodeId> = self.applied.keys().map(|k| k.0).collect();
                nodes.into_iter().map(|n| (n, compile::attr(&inner, n, "style").unwrap_or_default())).collect()
            };
            self.doc.eval(&code);
            // ...and re-assert what a changed attribute wiped, except properties the script set itself.
            let mut inner = self.doc.inner_mut();
            let changed: Vec<(NodeId, String, String)> =
                before.into_iter().filter_map(|(n, old)| compile::attr(&inner, n, "style").filter(|new| *new != old).map(|new| (n, old, new))).collect();
            if !changed.is_empty() {
                let decls = |s: &str| -> HashMap<String, String> {
                    s.split(';').filter_map(|d| d.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect()
                };
                let mut m = inner.mutate();
                for (node, old, new) in changed {
                    let (old, new) = (decls(&old), decls(&new));
                    for ((n, prop), value) in self.applied.iter() {
                        if *n == node && old.get(prop) == new.get(prop) {
                            m.set_style_property(node, prop, value);
                        }
                    }
                }
            }
            drop(inner);
            for e in self.doc.take_js_errors() {
                if !self.js_errors.contains(&e) {
                    self.js_errors.push(e);
                }
            }
        }

        if self.seek_dispatch {
            // then Web Animations at this frame's time (the shim's __raeaFrame, jetway #9), before its layout
            let settled = self.clock_epoch.is_none();
            self.doc.eval(&format!("__hfb.seek({t}); if (typeof __raeaFrame === 'function') __raeaFrame({t}, {settled});"));
            // a settled page's own frame loop, after the animations' finish events ran (the eval above ended)
            if settled {
                self.doc.eval("if (typeof __raeaFrameTurn === 'function') __raeaFrameTurn();");
            }
            let mut batch = std::mem::take(&mut self.canvas_pending);
            for msg in self.doc.take_messages() {
                let Ok(v) = serde_json::from_str::<Value>(&msg) else { continue };
                match v.get("type").and_then(Value::as_str) {
                    Some("canvas") | Some("webgl") => batch.push(v),
                    Some("warn") => {
                        let m = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
                        if !self.warnings.contains(&m) {
                            self.warnings.push(m);
                        }
                    }
                    _ => {}
                }
            }
            for e in self.doc.take_js_errors() {
                if !self.js_errors.contains(&e) {
                    self.js_errors.push(e);
                }
            }
            self.apply_canvases(batch);
        }
        {
            let mut inner = self.doc.inner_mut();
            self.timing.apply(&mut inner, t);
        }
        self.apply_video_frames(t);
        let r0 = std::time::Instant::now();
        let mut inner = self.doc.inner_mut();
        inner.resolve(t);
        // IntersectionObserver (jetway #4): checked against this frame's layout (the shim's __raeaIO), its callbacks
        // run before the paint, and what they change laid out at t (a no-op when nothing did)
        if self.seek_dispatch {
            drop(inner);
            self.doc.eval("if (typeof __raeaIO === 'function') __raeaIO();");
            inner = self.doc.inner_mut();
            inner.resolve(t);
        }
        if crate::env_os("DEBUG_TIMING").is_some() {
            eprintln!("[timing]   resolve {:.1}ms", r0.elapsed().as_secs_f64() * 1e3);
        }
    }

    /// The element the shim tagged `data-hfb="r"` (refs can be assigned after load).
    fn resolve_ref(&mut self, r: u64) -> Option<NodeId> {
        if let Some(n) = self.elements.get(&r) {
            return Some(*n);
        }
        let inner = self.doc.inner();
        let key = r.to_string();
        let found = compile::all_elements(&inner).into_iter().find(|&id| compile::attr(&inner, id, "data-hfb").as_deref() == Some(key.as_str()));
        drop(inner);
        if let Some(n) = found {
            self.elements.insert(r, n);
        }
        found
    }

    /// A canvas that isn't in the document (an offscreen buffer): its calls replay into a bitmap kept by
    /// element ref, which drawImage, createPattern and getImageData on other canvases can read.
    #[cfg(feature = "skia")]
    fn apply_detached_canvas(&mut self, msg: &Value) {
        let Some(r) = msg["ref"].as_u64() else { return };
        let (w, h) = (msg["w"].as_i64().unwrap_or(300) as i32, msg["h"].as_i64().unwrap_or(150) as i32);
        if self.detached_canvases.get(&r).is_none_or(|c| c.size() != (w, h)) {
            let Some(c) = crate::canvas::Canvas2D::new(w, h) else { return };
            self.detached_canvases.insert(r, c);
        }
        let ops = msg["ops"].as_array().cloned().unwrap_or_default();
        let (net, html, cache) = (&self.net, &self.opts.html_path, &self.src_images);
        let mut sources: HashMap<u64, (u32, u32, Vec<u8>)> = HashMap::new();
        for v in ops.iter().filter_map(crate::canvas::op_image_source) {
            let Some(sr) = v["ref"].as_u64() else { continue };
            if sr == r || sources.contains_key(&sr) {
                continue;
            }
            if let Some(c) = self.detached_canvases.get_mut(&sr) {
                let (cw, ch) = c.size();
                sources.insert(sr, (cw as u32, ch as u32, c.pixels()));
            } else if let Some(img) = v["src"].as_str().and_then(|s| decode_src(net, html, cache, s)) {
                sources.insert(sr, img);
            }
        }
        // a function of t, not of render history (seek determinism): Chrome draws t = 0 before late fonts load
        let pending = (self.seek_time == 0.0).then(|| self.canvas_font_pending());
        if let Some(c) = self.detached_canvases.get_mut(&r) {
            c.pending_fonts = pending;
            c.apply(&ops, &|sr| sources.get(&sr).cloned());
            c.pending_fonts = None;
            // kept for getImageData on it and for the getImageData of a canvas that draws it
            // (drawImage, createPattern)
            self.canvas_readback.lock().unwrap().insert(r, Some((w, h, c.pixels())));
        }
    }

    /// Replay recorded canvas calls and show each canvas's bitmap as the element's image.
    #[cfg(feature = "skia")]
    fn apply_canvases(&mut self, batch: Vec<Value>) {
        if batch.is_empty() {
            return;
        }
        let mut touched = Vec::new();
        let mut gl_frames: Vec<(NodeId, u32, u32, Vec<u8>)> = Vec::new();
        let r_of = |m: &Value| m["ref"].as_u64().unwrap_or(0);
        // 2D canvases first: a WebGL canvas uploads them as textures (scene captures drawn into offscreen
        // canvases, then texImage2D), and the shim flushes canvases in creation order
        let (canvas2d, webgl): (Vec<Value>, Vec<Value>) = batch.into_iter().partition(|m| m["type"] != "webgl");
        for msg in canvas2d.into_iter().chain(webgl) {
            let Some(node) = msg["ref"].as_u64().and_then(|r| self.resolve_ref(r)) else {
                if msg["type"] == "canvas" {
                    self.apply_detached_canvas(&msg);
                }
                continue;
            };
            let (w, h) = (msg["w"].as_i64().unwrap_or(300) as i32, msg["h"].as_i64().unwrap_or(150) as i32);
            // image sources referenced by these calls (refs can be assigned after load)
            if let Some(ops) = msg["ops"].as_array() {
                for o in ops {
                    for a in o.as_array().into_iter().flatten() {
                        if let Some(r) = a.get("ref").and_then(Value::as_u64) {
                            self.resolve_ref(r);
                        }
                    }
                }
            }
            if msg["type"] == "webgl" {
                #[cfg(feature = "webgl")]
                {
                    let ops = msg["ops"].as_array().cloned().unwrap_or_default();
                    // canvases used as texture sources: other <canvas> elements' bitmaps and offscreen canvases
                    // (straight-alpha RGBA, like decoded images)
                    let mut canvas_srcs: HashMap<u64, (u32, u32, Vec<u8>)> = HashMap::new();
                    for a in ops.iter().filter_map(Value::as_array).flatten() {
                        let Some(r) = a.get("ref").and_then(Value::as_u64) else { continue };
                        if canvas_srcs.contains_key(&r) {
                            continue;
                        }
                        let readback = |r: u64| self.canvas_readback.lock().unwrap().get(&r).cloned().flatten().map(|(cw, ch, px)| (cw as u32, ch as u32, px));
                        let src = match self.elements.get(&r).copied() {
                            #[cfg(feature = "skia")]
                            Some(n) if n != node => match self.canvases.get_mut(&n) {
                                Some(c) => {
                                    let (cw, ch) = c.size();
                                    Some((cw as u32, ch as u32, c.pixels()))
                                }
                                // an offscreen canvas (never attached): its bitmap is a detached canvas's
                                None => readback(r),
                            },
                            Some(_) => None,
                            None => readback(r),
                        };
                        if let Some(src) = src {
                            canvas_srcs.insert(r, src);
                        }
                    }
                    let inner = self.doc.inner();
                    let (elements, net, html, cache) = (&self.elements, &self.net, &self.opts.html_path, &self.src_images);
                    let lookup = |v: &Value| -> Option<(u32, u32, Vec<u8>)> {
                        if let Some(c) = v["ref"].as_u64().and_then(|r| canvas_srcs.get(&r)) {
                            return Some(c.clone());
                        }
                        let in_doc = v["ref"].as_u64().and_then(|r| elements.get(&r)).and_then(|&n| {
                            let data = inner.get_node(n)?.element_data()?.raster_image_data()?;
                            Some((data.width, data.height, data.data.as_ref().as_ref().to_vec()))
                        });
                        in_doc.or_else(|| v["src"].as_str().and_then(|s| decode_src(net, html, cache, s)))
                    };
                    let result = crate::webgl::flush(r_of(&msg), w, h, msg["v"].as_u64().unwrap_or(2) as u8, &msg["attrs"], &ops, &lookup);
                    drop(inner);
                    match result {
                        Ok((fw, fh, px)) => gl_frames.push((node, fw, fh, px)),
                        Err(e) => {
                            if !self.warnings.contains(&e) {
                                self.warnings.push(e);
                            }
                        }
                    }
                }
                continue;
            }
            if self.canvases.get(&node).is_none_or(|c| c.size() != (w, h)) {
                match crate::canvas::Canvas2D::new(w, h) {
                    Some(c) => {
                        self.canvases.insert(node, c);
                    }
                    None => continue,
                }
            }
            let ops = msg["ops"].as_array().cloned().unwrap_or_default();
            // drawImage sources: <img> pixels from the document, or another canvas's bitmap
            let inner = self.doc.inner();
            let (elements, net, html, cache) = (&self.elements, &self.net, &self.opts.html_path, &self.src_images);
            let images: std::collections::HashMap<u64, (u32, u32, Vec<u8>)> = ops
                .iter()
                .filter_map(crate::canvas::op_image_source)
                .filter_map(|v| {
                    let r = v["ref"].as_u64()?;
                    let in_doc = elements.get(&r).and_then(|&n| {
                        let data = inner.get_node(n)?.element_data()?.raster_image_data()?;
                        Some((data.width, data.height, data.data.as_ref().as_ref().to_vec()))
                    });
                    Some((r, in_doc.or_else(|| v["src"].as_str().and_then(|s| decode_src(net, html, cache, s)))?))
                })
                .collect();
            drop(inner);
            let mut others: std::collections::HashMap<u64, (u32, u32, Vec<u8>)> = std::collections::HashMap::new();
            for o in &ops {
                if let Some(r) = crate::canvas::op_image_source(o).and_then(|v| v["ref"].as_u64()) {
                    if images.contains_key(&r) || others.contains_key(&r) {
                        continue;
                    }
                    if let Some(src_node) = self.elements.get(&r).copied() {
                        if src_node != node {
                            if let Some(c) = self.canvases.get_mut(&src_node) {
                                let (cw, ch) = c.size();
                                others.insert(r, (cw as u32, ch as u32, c.pixels()));
                            }
                        }
                    } else if let Some(c) = self.detached_canvases.get_mut(&r) {
                        let (cw, ch) = c.size();
                        others.insert(r, (cw as u32, ch as u32, c.pixels()));
                    }
                }
            }
            let lookup = |r: u64| images.get(&r).or_else(|| others.get(&r)).cloned();
            // a function of t, not of render history (seek determinism): Chrome draws t = 0 before late fonts load
            let pending = (self.seek_time == 0.0).then(|| self.canvas_font_pending());
            if let Some(c) = self.canvases.get_mut(&node) {
                c.pending_fonts = pending;
                c.apply(&ops, &lookup);
                c.pending_fonts = None;
                // keep the bitmap for getImageData, for canvases that have asked for pixels
                if let Some(r) = msg["ref"].as_u64() {
                    let mut rb = self.canvas_readback.lock().unwrap();
                    if rb.contains_key(&r) {
                        rb.insert(r, Some((w, h, c.pixels())));
                    }
                }
            }
            if !touched.contains(&node) {
                touched.push(node);
            }
        }
        let mut inner = self.doc.inner_mut();
        for (node, w, h, pixels) in gl_frames {
            if let Some(el) = inner.get_node_mut(node).and_then(|n| n.element_data_mut()) {
                el.special_data = blitz_dom::node::SpecialElementData::Image(Box::new(blitz_dom::node::ImageData::Raster(
                    blitz_dom::node::RasterImageData::new(w.max(1), h.max(1), Arc::new(pixels)),
                )));
            }
            if let Some(n) = inner.get_node_mut(node) {
                n.insert_damage(style::selector_parser::RestyleDamage::from_bits_retain(0b1110_1111));
            }
        }
        for node in touched {
            let Some(c) = self.canvases.get_mut(&node) else { continue };
            let (w, h) = c.size();
            let pixels = c.pixels();
            if let Some(el) = inner.get_node_mut(node).and_then(|n| n.element_data_mut()) {
                el.special_data = blitz_dom::node::SpecialElementData::Image(Box::new(blitz_dom::node::ImageData::Raster(
                    blitz_dom::node::RasterImageData::new(w.max(1) as u32, h.max(1) as u32, Arc::new(pixels)),
                )));
            }
            if let Some(n) = inner.get_node_mut(node) {
                n.insert_damage(style::selector_parser::RestyleDamage::from_bits_retain(0b1110_1111));
            }
        }
    }

    #[cfg(not(feature = "skia"))]
    fn apply_canvases(&mut self, batch: Vec<Value>) {
        if !batch.is_empty() && !self.warnings.iter().any(|w| w.contains("<canvas>")) {
            self.warnings.push("<canvas> drawing is not rendered: this build has no Skia backend".into());
        }
    }

    fn apply_video_frames(&mut self, t: f64) {
        if self.video_nodes.is_empty() {
            return;
        }
        for (vid, node) in self.video_nodes.clone() {
            // clip-local frame index: a frame directory file or the host's frame provider
            let frame = self.opts.videos.frame_key(&vid, t);
            if self.video_shown.get(&vid) == Some(&frame) {
                continue;
            }
            let image = frame.and_then(|(k, _)| self.opts.videos.frame_rgba_at(&vid, t, k));
            let mut inner = self.doc.inner_mut();
            // The HTML composition format always hides the <video> itself and overlays its frame as an <img> with the
            // video's styles; with no frame (missing source, failed decode) nothing is drawn, not even the
            // video's border or shadow.
            match (image.is_some(), self.video_hidden.contains(&node)) {
                (false, false) => {
                    inner.mutate().set_style_property(node, "visibility", "hidden");
                    self.video_hidden.insert(node);
                }
                (true, true) => {
                    inner.mutate().remove_style_property(node, "visibility");
                    self.video_hidden.remove(&node);
                }
                _ => {}
            }
            if let Some(n) = inner.get_node_mut(node) {
                if let Some(el) = n.element_data_mut() {
                    el.special_data = match image {
                        Some((w, h, data)) => blitz_dom::node::SpecialElementData::Image(Box::new(
                            blitz_dom::node::ImageData::Raster(blitz_dom::node::RasterImageData::new(w, h, Arc::new(data))),
                        )),
                        None => blitz_dom::node::SpecialElementData::None,
                    };
                }
                n.clear_layout_cache();
                n.insert_damage(style::selector_parser::RestyleDamage::from_bits_retain(0b1110_1111));
            }
            drop(inner);
            self.video_shown.insert(vid, frame);
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self.opts.backend {
            Backend::Skia => "skia",
            Backend::Gpu => "gpu",
            Backend::Vello | Backend::Auto => "vello",
        }
    }

    /// The frame's state at `t`: animation tracks, script callbacks, canvases (stage.rs's seek system).
    pub fn seek_frame(&mut self, t: f64) {
        self.seek(t);
        self.frames_rendered += 1;
    }

    /// Paint the document as it stands into `out` (stage.rs's paint system); `t` and `t0` only label timing output.
    pub fn paint(&mut self, out: &mut Vec<u8>, t: f64, t0: std::time::Instant) {
        let timing = crate::env_os("DEBUG_TIMING").is_some();
        let t1 = t0.elapsed();
        let w = (self.width as f32 * self.opts.scale).round() as u32;
        let h = (self.height as f32 * self.opts.scale).round() as u32;
        let scale = self.opts.scale as f64;
        let transparent = self.opts.transparent;
        let backend = self.opts.backend;
        let renderer = self.renderer.get_or_insert_with(|| match backend {
            Backend::Vello | Backend::Gpu | Backend::Auto => Raster::Vello(VelloCpuImageRenderer::new(w, h)),
            #[cfg(feature = "skia")]
            Backend::Skia => Raster::Skia(SkiaImageRenderer::new(w, h)),
            #[cfg(not(feature = "skia"))]
            Backend::Skia => unreachable!("Composition::load rejects the skia backend without the `skia` feature"),
        });
        let mut inner = self.doc.inner_mut();
        out.resize((w * h * 4) as usize, 0);
        // 3D-transformed planes rasterize offscreen with the frame's rasterizer
        blitz_paint::set_offscreen_rasterizer(Some(match renderer {
            Raster::Vello(_) => raster_layer_vello,
            #[cfg(feature = "skia")]
            Raster::Skia(_) => raster_layer_skia,
        }));
        // 3D planes: the flat layer drawn through Skia's concat_44 (Chrome composites layers with
        // perspective-correct bilinear sampling); the CPU warp without Skia
        #[cfg(feature = "skia")]
        blitz_paint::set_layer_warper(Some(warp_layer_skia));
        // inline SVG through Skia (Chrome's SVG rasterizer) when the frame itself is vello
        #[cfg(feature = "skia")]
        blitz_paint::set_svg_rasterizer(match renderer {
            Raster::Vello(_) => Some(raster_svg_skia),
            Raster::Skia(_) => None,
        });
        // the GPU backend paints here; a frame it can't (a backdrop filter, no adapter) falls through to vello_cpu
        if backend == Backend::Gpu && crate::quads::paint_with(w, h, false, out, |scene| paint_frame(scene, &mut inner, scale, w, h, transparent)).is_ok() {
            if timing {
                eprintln!("[timing] t={t:.3} seek+resolve {:.1}ms paint+gpu {:.1}ms", t1.as_secs_f64() * 1e3, (t0.elapsed() - t1).as_secs_f64() * 1e3);
            }
            return;
        }
        match renderer {
            Raster::Vello(r) => {
                // the CPU renderer keeps recorded commands until reset: without this every frame
                // would repaint all the previous ones underneath
                r.reset();
                r.render(|scene| paint_frame(scene, &mut inner, scale, w, h, transparent), out);
            }
            #[cfg(feature = "skia")]
            Raster::Skia(r) => r.render(|scene| paint_frame(scene, &mut inner, scale, w, h, transparent), out),
        }
        if timing {
            eprintln!("[timing] t={t:.3} seek+resolve {:.1}ms paint+raster {:.1}ms", t1.as_secs_f64() * 1e3, (t0.elapsed() - t1).as_secs_f64() * 1e3);
        }
    }

    /// Render frame `t` and report the text on it, one entry per element whose text was painted, in paint
    /// order: its box in composition (CSS) px including transforms, the canvas edges it crosses, whether an
    /// overflow clip hides part of it, its effective opacity, the safe-zone bands it reaches (`safe`:
    /// top, right, bottom, left in CSS px) and the other text boxes it overlaps. Text entirely hidden by
    /// clips or off the canvas is left out.
    /// (A frame is made by Bevy's schedule, stage.rs; `Stage::text_boxes` paints it collecting `runs` and calls this.)
    pub fn text_report(&mut self, t: f64, runs: Vec<blitz_paint::TextRunBox>, safe: Option<[f64; 4]>) -> Value {
        let scale = self.opts.scale as f64;
        let (cw, ch) = (self.width as f64, self.height as f64);
        let canvas = Rect::new(0.0, 0.0, cw, ch);
        let inner = self.doc.inner();
        // (element, full box, part not hidden by overflow clips)
        let mut acc: Vec<(NodeId, Rect, Option<Rect>)> = Vec::new();
        // each element's glyph runs' vertical centres and heights: how many lines it wraps to
        let mut runs_y: std::collections::HashMap<NodeId, Vec<(f64, f64)>> = Default::default();
        for r in runs {
            let owner = match inner.get_node(r.node) {
                Some(n) if n.is_text_node() => n.parent.unwrap_or(r.node),
                _ => r.node,
            };
            let p = r.corners.map(|c| peniko::kurbo::Point::new(c.x / scale, c.y / scale));
            let bbox = Rect::from_points(p[0], p[1]).union_pt(p[2]).union_pt(p[3]);
            let clip = Rect::new(r.clip.x0 / scale, r.clip.y0 / scale, r.clip.x1 / scale, r.clip.y1 / scale);
            let shown = bbox.intersect(clip);
            let shown = (shown.width() > 0.0 && shown.height() > 0.0).then_some(shown);
            runs_y.entry(owner).or_default().push((bbox.center().y, bbox.height()));
            match acc.iter_mut().find(|a| a.0 == owner) {
                Some(a) => {
                    a.1 = a.1.union(bbox);
                    a.2 = match (a.2, shown) {
                        (Some(x), Some(y)) => Some(x.union(y)),
                        (x, y) => x.or(y),
                    };
                }
                None => acc.push((owner, bbox, shown)),
            }
        }
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        let mut boxes: Vec<(Rect, NodeId, Value)> = Vec::new();
        for (node, bbox, shown) in acc {
            // visible part: inside overflow clips and on the canvas
            let Some(visible) = shown.map(|s| s.intersect(canvas)).filter(|v| v.width() > 0.0 && v.height() > 0.0) else { continue };
            let mut opacity = 1.0f64;
            let mut cur = Some(node);
            while let Some(id) = cur {
                let Some(n) = inner.get_node(id) else { break };
                if let Some(st) = n.primary_styles() {
                    opacity *= st.get_effects().opacity as f64;
                }
                cur = n.parent;
            }
            let text: String = inner.get_node(node).map(|n| n.text_content()).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
            let short: String = text.chars().take(40).collect();
            let mut clipped = Vec::new();
            for (edge, out) in [("top", bbox.y0 < -0.5), ("right", bbox.x1 > cw + 0.5), ("bottom", bbox.y1 > ch + 0.5), ("left", bbox.x0 < -0.5)] {
                if out {
                    clipped.push(edge);
                }
            }
            let shown_area = shown.map(|s| s.area()).unwrap_or(0.0);
            let mut zones = Vec::new();
            if let Some([top, right, bottom, left]) = safe {
                for (edge, hit) in [
                    ("top", top > 0.0 && visible.y0 < top),
                    ("right", right > 0.0 && visible.x1 > cw - right),
                    ("bottom", bottom > 0.0 && visible.y1 > ch - bottom),
                    ("left", left > 0.0 && visible.x0 < left),
                ] {
                    if hit {
                        zones.push(edge);
                    }
                }
            }
            // the lines it wraps to: runs whose centres sit more than half a run's height apart
            let lines = {
                let mut ys = runs_y.get(&node).cloned().unwrap_or_default();
                ys.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut n = 0usize;
                let mut last: Option<f64> = None;
                for (y, hgt) in ys {
                    if last.is_none_or(|l| y - l > 0.5 * hgt.max(1.0)) {
                        n += 1;
                        last = Some(y);
                    }
                }
                n.max(1)
            };
            // its component: the nearest ancestor that draws a border, a background or a shadow (a card, a pill,
            // a button), and how far the text spills past that component's inner edge (inside its border), per side;
            // ancestors' transforms aren't applied to the component's box
            let mut component = Value::Null;
            let mut spill = serde_json::Map::new();
            // a transform on the text, its component or anything above puts the painted text and the laid-out
            // component in different coordinates (an animated badge, a tilted label): no spill is measured then
            let transformed = {
                let mut t = false;
                let mut c = Some(node);
                while let Some(id) = c {
                    let Some(n) = inner.get_node(id) else { break };
                    if n.primary_styles().is_some_and(|st| !st.get_box().transform.0.is_empty()) {
                        t = true;
                        break;
                    }
                    c = n.parent;
                }
                t
            };
            let mut cur = inner.get_node(node).and_then(|n| n.parent);
            while let Some(id) = cur {
                let Some(n) = inner.get_node(id) else { break };
                if let Some(st) = n.primary_styles() {
                    // the laid-out border (a style's width with border-style none still reads 3 px)
                    let lb = n.final_layout().border;
                    let bordered = lb.top > 0.0 || lb.right > 0.0 || lb.bottom > 0.0 || lb.left > 0.0;
                    let filled = st.get_background().background_color.resolve_to_absolute(&st.clone_color()).alpha > 0.0;
                    let shadowed = !st.get_effects().box_shadow.0.is_empty();
                    if bordered || filled || shadowed {
                        let l = n.final_layout();
                        let p = n.absolute_position(0.0, 0.0);
                        let (bx, by) = (p.x as f64, p.y as f64);
                        let (bw, bh) = (l.size.width as f64, l.size.height as f64);
                        let ix0 = bx + l.border.left as f64;
                        let iy0 = by + l.border.top as f64;
                        let ix1 = bx + bw - l.border.right as f64;
                        let iy1 = by + bh - l.border.bottom as f64;
                        let draws: Vec<&str> = [bordered.then_some("border"), filled.then_some("background"), shadowed.then_some("shadow")].into_iter().flatten().collect();
                        component = json!({"id": compile::attr(&inner, id, "id"), "class": compile::attr(&inner, id, "class"), "tag": compile::tag(&inner, id),
                            "x": r1(bx), "y": r1(by), "w": r1(bw), "h": r1(bh), "draws": draws});
                        for (side, px) in [("top", iy0 - bbox.y0), ("right", bbox.x1 - ix1), ("bottom", bbox.y1 - iy1), ("left", ix0 - bbox.x0)] {
                            if px > 0.5 && !transformed {
                                spill.insert(side.into(), json!(r1(px)));
                            }
                        }
                        break;
                    }
                }
                cur = n.parent;
            }
            let v = json!({
                "lines": lines,
                "component": component,
                "spills": spill,
                "transformed": transformed,
                "id": compile::attr(&inner, node, "id"),
                "class": compile::attr(&inner, node, "class"),
                "tag": compile::tag(&inner, node),
                "text": short,
                "words": text.split_whitespace().count(),
                "x": r1(bbox.x0), "y": r1(bbox.y0), "w": r1(bbox.width()), "h": r1(bbox.height()),
                "centerOffset": {"x": r1(bbox.center().x - cw / 2.0), "y": r1(bbox.center().y - ch / 2.0)},
                "opacity": (opacity * 1000.0).round() / 1000.0,
                "clipped": clipped,
                "overflowClipped": shown_area < bbox.area() - 1.0,
                // share of the box on screen (inside overflow clips and the canvas)
                "visibleFraction": ((visible.area() / bbox.area().max(1e-9)).min(1.0) * 100.0).round() / 100.0,
                "inSafeZone": zones,
            });
            boxes.push((visible, node, v));
        }
        // text nested in another listed element (a highlighted <span> in its title) is not an overlap
        let contains = |outer: NodeId, inner_id: NodeId| {
            let mut cur = inner.get_node(inner_id).and_then(|n| n.parent);
            while let Some(id) = cur {
                if id == outer {
                    return true;
                }
                cur = inner.get_node(id).and_then(|n| n.parent);
            }
            false
        };
        let rects: Vec<(Rect, NodeId)> = boxes.iter().map(|b| (b.0, b.1)).collect();
        let list: Vec<Value> = boxes
            .into_iter()
            .enumerate()
            .map(|(i, (_, _, mut v))| {
                let (ri, ni) = rects[i];
                let hits: Vec<usize> = rects
                    .iter()
                    .enumerate()
                    .filter(|&(j, &(r, n))| j != i && r.intersect(ri).area() > 1.0 && !contains(n, ni) && !contains(ni, n))
                    .map(|(j, _)| j)
                    .collect();
                v["overlaps"] = json!(hits);
                v
            })
            .collect();
        let safe_zone = safe.map(|[top, right, bottom, left]| json!({"top": top, "right": right, "bottom": bottom, "left": left}));
        let mut report = json!({"t": t, "width": self.width, "height": self.height, "safeZone": safe_zone, "boxes": list});
        // text that doesn't fit its component (src/textfit.rs)
        report["textfit"] = crate::textfit::findings(&report);
        report
    }

    /// Inline <svg>s parse into usvg trees when their boxes are built, which can be before the page's
    /// web fonts arrive (static SVG text then kept the fallback font); rebuild them once loading is done.
    fn reparse_inline_svgs(&mut self) {
        let mut inner = self.doc.inner_mut();
        let roots: Vec<NodeId> = compile::all_elements(&inner)
            .into_iter()
            .filter(|&id| {
                let is_svg = |n: NodeId| inner.get_node(n).and_then(|x| x.element_data()).is_some_and(|e| e.name.ns == blitz_dom::ns!(svg));
                is_svg(id) && !inner.get_node(id).and_then(|x| x.parent).is_some_and(is_svg)
            })
            .collect();
        for root in roots {
            if let Some(n) = inner.get_node_mut(root) {
                n.clear_layout_cache();
                n.insert_damage(style::selector_parser::RestyleDamage::from_bits_retain(0b1110_1111));
            }
        }
    }

    /// @font-face families Chrome has not loaded when the HTML composition format captures the first frame: a face loads
    /// when text needs it, so a family only canvas text uses is still loading at t = 0 (the load-time
    /// draw and the first hf-seek both fall back to the default serif; tests/parity/canvas2d/drawing).
    fn canvas_font_pending(&self) -> std::collections::HashSet<String> {
        let inner = self.doc.inner();
        let mut declared = declared_web_fonts(&inner);
        // families DOM text uses (the first of each text-bearing element's list) are loaded before capture
        let mut used = std::collections::HashSet::new();
        for id in compile::all_elements(&inner) {
            let Some(parent) = inner.get_node(id) else { continue };
            let has_text = parent.children.iter().any(|&c| inner.get_node(c).is_some_and(|n| n.is_text_node() && !n.text_content().trim().is_empty()));
            if !has_text || matches!(compile::tag(&inner, id).as_str(), "script" | "style" | "title") {
                continue;
            }
            if let Some(styles) = parent.primary_styles() {
                use style::values::computed::font::SingleFontFamily;
                if let Some(SingleFontFamily::FamilyName(name)) = styles.get_font().font_family.families.iter().next() {
                    used.insert(name.name.to_string().to_ascii_lowercase());
                }
            }
        }
        declared.retain(|f| !used.contains(f));
        declared
    }

    pub fn output_size(&self) -> (u32, u32) {
        ((self.width as f32 * self.opts.scale).round() as u32, (self.height as f32 * self.opts.scale).round() as u32)
    }

    pub fn drain_warnings(&mut self) -> Vec<String> {
        let mut w = std::mem::take(&mut self.warnings);
        w.extend(self.net.warnings.lock().unwrap().drain(..));
        w
    }
}

/// A `<video>` as a GPU compositor draws it when the page is hosted (`Composition::host_media`): its picture's box
/// in CSS pixels, how the picture fits it, its stacking order among the videos, and why it can't be drawn that way
/// (`not_simple`), in which case the page renders on the CPU path as before.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MediaLayer {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// object-fit: cover, contain, fill, none or scale-down
    pub fit: String,
    /// object-position as fractions of the free space (0.5, 0.5 is centred)
    pub position: (f64, f64),
    /// order among the page's videos (later is on top)
    pub z: usize,
    /// its opacity times its ancestors' (a fade is a host's per-frame opacity, not a reason to refuse)
    pub opacity: f64,
    pub not_simple: Option<String>,
}

impl Composition {
    /// Paint the page without its videos and without the backgrounds behind them, over transparency: a host draws
    /// the videos (decoded on the GPU) and the clear colour this returns, then this page over them. Refused (the
    /// reason) when what's behind a video isn't one flat colour.
    pub fn host_media(&mut self) -> Result<[u8; 4], String> {
        self.opts.transparent = true;
        let mut clear: Option<[u8; 4]> = None;
        let mut flatten = Vec::new();
        {
            let inner = self.doc.inner();
            let mut behind = std::collections::BTreeSet::new();
            for (_, node) in &self.video_nodes {
                let mut cur = inner.get_node(*node).and_then(|n| n.parent);
                while let Some(id) = cur {
                    behind.insert(id);
                    cur = inner.get_node(id).and_then(|n| n.parent);
                }
            }
            for id in behind {
                let Some(n) = inner.get_node(id) else { continue };
                let Some(st) = n.primary_styles() else { continue };
                let bg = st.get_background();
                if bg.background_image.0.iter().any(|i| !matches!(i, style::values::generics::image::GenericImage::None)) {
                    return Err(format!("a background image behind a video ({})", compile::tag(&inner, id)));
                }
                let c = bg.background_color.resolve_to_absolute(&st.clone_color());
                if c.alpha <= 0.0 {
                    continue;
                }
                let srgb = c.to_color_space(style::color::ColorSpace::Srgb);
                let px = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                let rgba = [px(srgb.components.0), px(srgb.components.1), px(srgb.components.2), px(srgb.alpha)];
                if rgba[3] != 255 {
                    return Err("a translucent background behind a video".into());
                }
                match clear {
                    Some(c0) if c0 != rgba => return Err("two different background colours behind the videos".into()),
                    _ => clear = Some(rgba),
                }
                flatten.push(id);
            }
        }
        let mut inner = self.doc.inner_mut();
        for id in flatten {
            inner.mutate().set_style_property(id, "background-color", "transparent");
        }
        Ok(clear.unwrap_or([0, 0, 0, 255]))
    }

    /// The videos at time t (after `seek`), as `MediaLayer`s in stacking order.
    pub fn media_layers(&mut self, t: f64) -> Vec<MediaLayer> {
        self.seek(t);
        let inner = self.doc.inner();
        let order = compile::all_elements(&inner);
        let rank: HashMap<NodeId, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let videos: std::collections::HashSet<NodeId> = self.video_nodes.iter().map(|(_, n)| *n).collect();
        let abs = |id: NodeId| -> Option<(f64, f64, f64, f64)> {
            let n = inner.get_node(id)?;
            let l = n.final_layout();
            let (mut x, mut y) = (l.location.x as f64, l.location.y as f64);
            let mut cur = n.layout_parent.get();
            while let Some(p) = cur.and_then(|p| inner.get_node(p)) {
                x += p.final_layout().location.x as f64;
                y += p.final_layout().location.y as f64;
                cur = p.layout_parent.get();
            }
            Some((x, y, l.size.width as f64, l.size.height as f64))
        };
        let mut out = Vec::new();
        let mut list: Vec<&(String, NodeId)> = self.video_nodes.iter().collect();
        list.sort_by_key(|(_, n)| rank.get(n).copied().unwrap_or(usize::MAX));
        for (z, (vid, node)) in list.into_iter().enumerate() {
            let Some(n) = inner.get_node(*node) else { continue };
            let Some(st) = n.primary_styles() else { continue };
            let l = n.final_layout();
            let Some((bx, by, bw, bh)) = abs(*node) else { continue };
            // the picture goes in the content box
            let (x, y) = (bx + (l.border.left + l.padding.left) as f64, by + (l.border.top + l.padding.top) as f64);
            let w = bw - (l.border.left + l.border.right + l.padding.left + l.padding.right) as f64;
            let h = bh - (l.border.top + l.border.bottom + l.padding.top + l.padding.bottom) as f64;
            let fit = format!("{:?}", st.clone_object_fit()).to_lowercase().replace('_', "-");
            let pos = st.clone_object_position();
            let frac = |v: &style::values::computed::LengthPercentage| v.resolve(style::values::computed::CSSPixelLength::new(100.0)).px() as f64 / 100.0;
            let position = (frac(&pos.horizontal), frac(&pos.vertical));
            let mut why: Option<String> = None;
            let mut opacity = 1.0f64;
            let mut note = |w: &str| {
                if why.is_none() {
                    why = Some(w.to_string());
                }
            };
            // the video and everything around it: no effect a flat picture in a box can't show
            let mut cur = Some(*node);
            while let Some(id) = cur {
                let Some(a) = inner.get_node(id) else { break };
                if let Some(s) = a.primary_styles() {
                    let fx = s.get_effects();
                    opacity *= fx.opacity as f64;
                    if !fx.filter.0.is_empty() {
                        note("a filter");
                    }
                    if fx.mix_blend_mode != style::computed_values::mix_blend_mode::T::Normal {
                        note("a blend mode");
                    }
                    if !s.get_box().transform.0.is_empty() {
                        note("a transform");
                    }
                    if !matches!(s.get_svg().clip_path, style::values::generics::basic_shape::GenericClipPath::None) {
                        note("a clip-path");
                    }
                    if id == *node {
                        let b = s.get_border();
                        let r = |c: &style::values::computed::BorderCornerRadius| c.0.width.0.resolve(style::values::computed::CSSPixelLength::new(100.0)).px() > 0.0;
                        if r(&b.border_top_left_radius) || r(&b.border_top_right_radius) || r(&b.border_bottom_left_radius) || r(&b.border_bottom_right_radius) {
                            note("rounded corners");
                        }
                    }
                }
                cur = a.parent;
            }
            // nothing that paints before it in the page may sit under it (the host draws videos below the page)
            let me = rank.get(node).copied().unwrap_or(0);
            let mut ancestors = std::collections::HashSet::new();
            let mut cur = n.parent;
            while let Some(id) = cur {
                ancestors.insert(id);
                cur = inner.get_node(id).and_then(|a| a.parent);
            }
            for &other in &order[..me] {
                if ancestors.contains(&other) || videos.contains(&other) {
                    continue;
                }
                let Some(o) = inner.get_node(other) else { continue };
                let Some(s) = o.primary_styles() else { continue };
                if s.get_inherited_box().visibility != style::computed_values::visibility::T::Visible {
                    continue;
                }
                let paints = s.get_background().background_color.resolve_to_absolute(&s.clone_color()).alpha > 0.0
                    || !o.text_content().trim().is_empty();
                if !paints {
                    continue;
                }
                if let Some((ox, oy, ow, oh)) = abs(other) {
                    if ox < x + w && ox + ow > x && oy < y + h && oy + oh > y && ow > 0.0 && oh > 0.0 {
                        note("page content under the video");
                        break;
                    }
                }
            }
            out.push(MediaLayer { id: vid.clone(), x, y, w, h, fit, position, z, opacity, not_simple: why });
        }
        out
    }

    /// What the page shows at time t, as a number that changes when it does (inline styles, classes and text after
    /// the seek): a host repaints its overlay only when this moves.
    pub fn page_key(&mut self, t: f64) -> u64 {
        use std::hash::{Hash, Hasher};
        self.seek(t);
        let inner = self.doc.inner();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // the timeline and scripts change the page through its elements' style blocks (set_style_property), not their
        // style attributes: hash the blocks as CSS
        let lock = inner.guard().read();
        let mut css = String::new();
        for id in compile::all_elements(&inner) {
            let Some(n) = inner.get_node(id) else { continue };
            id.hash(&mut h);
            if let Some(el) = n.element_data() {
                for a in el.attrs() {
                    if matches!(&*a.name.local, "class" | "src" | "hidden") {
                        a.value.hash(&mut h);
                    }
                }
                if let Some(block) = &el.style_attribute {
                    css.clear();
                    let _ = block.read_with(&lock).to_css(&mut css);
                    css.hash(&mut h);
                }
            }
            for c in &n.children {
                if let Some(t) = inner.get_node(*c).and_then(|c| c.text_data()) {
                    t.content.hash(&mut h);
                }
            }
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GSAP 3.14.2 CSSPlugin: `r.xPercent = x && Math.round(offsetWidth / 2) === Math.round(-x) ? -50 : 0`,
    /// `r.x = x - offsetWidth * xPercent / 100` (same for y).
    #[test]
    fn gsap_detects_centring_translate_as_percent() {
        // translate(-50%, -50%) of a 541.55 x 77 box, offsetWidth 542
        let s = TransformState::identity().detect_percent(-270.773, -38.5, 542.0, 77.0);
        assert_eq!((s.x_percent, s.y_percent), (-50.0, -50.0));
        assert_eq!(s.x.to_css(), "0.227px");
        // a plain offset stays px
        let s = TransformState::identity().detect_percent(-120.0, 0.0, 542.0, 77.0);
        assert_eq!(s.x_percent, 0.0);
    }

    /// DrawSVGPlugin end to end: the shim's measuring and value parsing (`__uvDrawSVGValue`) and Rust's
    /// render step (`draw_svg_dash`) against stroke-dasharray/-dashoffset that GSAP 3.14.2's real plugin
    /// set in Chrome on the same elements (tests/gsap-plugin-goldens.json, bench/gen-plugin-goldens.cjs;
    /// test environment only).
    #[test]
    fn draw_svg_matches_gsap_plugin() {
        let golden: Value = serde_json::from_str(include_str!("../tests/gsap-plugin-goldens.json")).unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gsap-plugins-golden");
        let mut comp = Composition::load(Options {
            html_path: dir.join("drawsvg.html"),
            project_dir: dir.clone(),
            fps: (30, 1),
            scale: 1.0,
            size: None,
            variables: None,
            allow_network: false,
            transparent: false,
            videos: Arc::new(Default::default()),
            backend: Backend::Vello,
            profile: Profile::default(),
            html_source: None,
            clock: Clock::default(),
        })
        .unwrap();
        let cases = golden["draw"].as_array().unwrap();
        let js = format!(
            "var c = {}; __blitz_send_message(JSON.stringify(c.map(function (k) {{ return __uvDrawSVGValue(document.getElementById(k.id), k.value); }})));",
            serde_json::to_string(&cases.iter().map(|c| json!({"id": c["id"], "value": c["value"]})).collect::<Vec<_>>()).unwrap()
        );
        comp.doc.eval(&js);
        let errors = comp.doc.take_js_errors();
        assert!(errors.is_empty(), "{errors:?}");
        let msgs = comp.doc.take_messages();
        let values: Vec<String> = serde_json::from_str(msgs.last().expect("no message")).unwrap();
        let nums = |s: &str| -> Vec<f64> { s.split([',', ' ']).filter_map(|t| t.trim().trim_end_matches("px").parse().ok()).collect() };
        let mut bad = Vec::new();
        for (case, triple) in cases.iter().zip(&values) {
            let (array, offset) = draw_svg_dash(&crate::value::parse_value("--uv-draw", triple));
            let (want_array, want_offset) = (case["dasharray"].as_str().unwrap(), case["dashoffset"].as_str().unwrap());
            let ok = if want_array.is_empty() {
                // the plugin clears the dash for a full draw; ours keeps a dash covering the whole stroke
                let t = nums(triple);
                (t[0] - t[2]).abs() < 1e-3 && t[1].abs() < 1e-3
            } else {
                let (a, w) = (nums(&array), nums(want_array));
                array == "none" && want_array == "none"
                    || a.len() == w.len() && a.iter().zip(&w).all(|(x, y)| (x - y).abs() < 0.01) && (nums(&offset)[0] - nums(want_offset)[0]).abs() < 0.01
            };
            if !ok {
                bad.push(format!("#{} {}: ours {array} / {offset} (from {triple}), gsap {want_array} / {want_offset}", case["id"], case["value"]));
            }
        }
        assert!(bad.is_empty(), "{} of {} mismatch:\n{}", bad.len(), cases.len(), bad.join("\n"));
    }
}
