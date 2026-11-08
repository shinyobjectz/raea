//! The React tree walked into the page's elements: kinds, media URLs, video frames and props.

use super::*;

/// The React tree under `e` (whose page id is `parent`, its path at `at`) as the page's elements, in order, into `out`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk(react: &World, e: Entity, parent: u64, at: At, dir: &Path, holders: &HashMap<u64, Map<String, Value>>, out: &mut Walk, videos: &mut Vec<VideoAt>, screens: &mut Vec<ScreenTree>) {
    let Some(kids) = react.get::<Children>(e) else { return };
    let mut list = out.list(parent);
    let mut n = 0usize;
    for k in kids.iter() {
        let Some(node) = react.get::<ReactNode>(k).map(|n| n.0) else { continue };
        let id = node + REACT_IDS;
        let kind = react.get::<ReactKind>(k).map(|k| k.0.as_str()).unwrap_or("div");
        if kind == "#text" || kind == "#span" {
            let raw = react.get::<ReactText>(k).map(|t| t.0.as_str());
            out.element(id, parent, at, Tail::T, kind, None, raw, || Want { id, parent, kind: "text".into(), props: Map::new(), text: Some(raw.map(|t| media_urls(t, dir)).unwrap_or_default()), path: String::new() });
            list.push(id);
            continue;
        }
        let props = holders.get(&node).or_else(|| react.get::<ReactProps>(k).map(|p| &p.0));
        let has = |key: &str| props.is_some_and(|p| p.contains_key(key));
        // a layered Canvas3D's element: the 3D's picture in the page's paint order (BevyPage::set_scene), its parts
        // not on the page
        // a texture element: a texture shared on this thread (share_texture), copied in on the GPU after the sync
        if let Some(tex) = props.and_then(|p| p.get("data-raea-texture")).and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))) {
            let me = Tail::N(n).path(out.base(at));
            n += 1;
            let mut page = page_props(kind, props, dir);
            page.retain(|k, _| k == "style" || k == "className");
            out.put(Want { id, parent, kind: "video".into(), props: page, text: None, path: me }, None);
            list.push(id);
            TEXTURE_SLOTS.with(|c| c.borrow_mut().push((tex, id)));
            continue;
        }
        if has("data-raea-scene") {
            let me = Tail::N(n).path(out.base(at));
            n += 1;
            let mut page = page_props(kind, props, dir);
            page.retain(|k, _| k == "style" || k == "className");
            if let Some(Value::Object(st)) = page.get_mut("style") {
                st.insert("objectFit".into(), Value::from("fill"));
            }
            out.put(Want { id, parent, kind: "video".into(), props: page, text: None, path: me }, None);
            list.push(id);
            SCENE_SLOT.with(|c| c.set(Some(id)));
            let base = out.base(at).to_string();
            walk(react, k, id, At::Str(&base), dir, holders, &mut Walk::default(), &mut Vec::new(), screens);
            continue;
        }
        if undrawn(kind) || has("data-3d") || has("data-raea-frame") || props.and_then(|p| p.get("data-mix")).and_then(Value::as_str) == Some("true") {
            // a camera in the page itself, not in a 3D part (docs/sdk.feature S3a): its view is its element's box
            let camera = props.and_then(|p| p.get("data-3d")).and_then(Value::as_str).is_some_and(|k| matches!(k, "PerspectiveCamera" | "OrthographicCamera"));
            if camera && IN_3D.with(|c| c.get()) == 0 && VIEW_HOLDER.with(|c| c.get()).is_none() {
                VIEW_HOLDER.with(|c| c.set(Some(parent)));
            }
            // not on the page, but a <Screen>'s picture inside it (a Canvas3D's) still is, in a root of its own; and
            // a layered Canvas3D's slot (its <img data-raea-scene>) is, here in the page's order
            let was = SCENE_SLOT.with(|c| c.get());
            SCENE_SLOT.with(|c| c.set(None));
            let mut none = Walk::default();
            let base = out.base(at).to_string();
            IN_3D.with(|c| c.set(c.get() + 1));
            walk(react, k, id, At::Str(&base), dir, holders, &mut none, &mut Vec::new(), screens);
            IN_3D.with(|c| c.set(c.get() - 1));
            match SCENE_SLOT.with(|c| c.get()) {
                Some(slot) => {
                    none.want.truncate(none.len);
                    if let Some(mut w) = none.want.into_iter().find(|w| w.id == slot) {
                        let me = Tail::N(n).path(&base);
                        n += 1;
                        if has("data-3d") && !undrawn(kind) {
                            // the Canvas3D's own box stays on the page with its slot in it: a layered Canvas3D's
                            // holder keeps its background (the Blitz page doesn't clear it), which covers the page under
                            w.path = format!("{me}.0");
                            out.put(Want { id, parent, kind: kind.to_string(), props: page_props(kind, props, dir), text: None, path: me }, None);
                            list.push(id);
                            w.parent = id;
                            let mut l = out.list(id);
                            l.push(slot);
                            out.order.insert(id, l);
                        } else {
                            w.parent = parent;
                            w.path = me;
                            list.push(slot);
                        }
                        out.put(w, None);
                    }
                }
                None => SCENE_SLOT.with(|c| c.set(was)),
            }
            continue;
        }
        // a <Screen>'s picture (not an app's: a document of its own, runtime/page/src/screen.rs): its own root, out of the page
        let screen = props.and_then(|p| p.get("data-raea-screen")).and_then(Value::as_str).filter(|_| !has("data-raea-screen-app"));
        let size = |k: &str| props.and_then(|p| p.get(k)).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0) as u32;
        if let (Some(name), Some(app)) = (props.and_then(|p| p.get("data-raea-screen")).and_then(Value::as_str), props.and_then(|p| p.get("data-raea-screen-app")).and_then(Value::as_str)) {
            APP_SCREENS.with(|c| c.borrow_mut().push((name.to_string(), size("data-raea-screen-w"), size("data-raea-screen-h"), app.to_string())));
        }
        if let Some(name) = screen.filter(|_| size("data-raea-screen-w") > 0 && size("data-raea-screen-h") > 0) {
            let mut page = page_props(kind, props, dir);
            if let Some(Value::Object(st)) = page.get_mut("style") {
                st.insert("left".into(), Value::from(0));
                st.insert("top".into(), Value::from(0));
            }
            let mut tree = Walk::default();
            let mut tvideos = Vec::new();
            tree.put(Want { id, parent: BODY, kind: kind.to_string(), props: page, text: None, path: "screen".into() }, None);
            walk(react, k, id, At::Str("screen"), dir, holders, &mut tree, &mut tvideos, screens);
            tree.want.truncate(tree.len);
            screens.push(ScreenTree { name: name.to_string(), w: size("data-raea-screen-w"), h: size("data-raea-screen-h"), top: id, want: tree.want, order: tree.order, videos: tvideos });
            continue;
        }
        let tail = Tail::N(n);
        n += 1;
        let src = (kind == "video").then(|| props.and_then(|p| p.get("src")).and_then(Value::as_str)).flatten();
        let me = if let Some(src) = src {
            let at_s = props.and_then(|p| p.get("data-source-time")).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
            let file = raea_runtime_react::scene::media_path(dir, src);
            let fit = video_fit(props, &file, at_s);
            videos.push(VideoAt { id, file, at: at_s, fit });
            let mut page = page_props(kind, props, dir);
            // a framed video shows its crop, whose aspect is its box's, stretched to the box
            if fit.is_some() {
                if let Value::Object(st) = page.entry("style").or_insert_with(|| Value::Object(Map::new())) {
                    st.insert("objectFit".into(), Value::from("fill"));
                }
            }
            // what the render's manifest says the video shows (BlitzPage::manifest)
            page.insert("data-raea-src".into(), Value::from(src));
            page.insert("data-raea-at".into(), Value::from((at_s * 1000.0).round() / 1000.0));
            let path = tail.path(out.base(at));
            let i = out.len;
            out.put(Want { id, parent, kind: kind.to_string(), props: page, text: None, path }, None);
            i
        } else {
            out.element(id, parent, at, tail, kind, props, None, || Want { id, parent, kind: kind.to_string(), props: page_props(kind, props, dir), text: None, path: String::new() })
        };
        list.push(id);
        // an element's own string: a text child of its own
        if let Some(t) = react.get::<ReactText>(k) {
            let tid = id | TEXT_BIT;
            out.element(tid, id, At::In(me), Tail::DotT, "#own", None, Some(&t.0), || Want { id: tid, parent: id, kind: "text".into(), props: Map::new(), text: Some(media_urls(&t.0, dir)), path: String::new() });
            let mut l = out.list(id);
            l.push(tid);
            out.order.insert(id, l);
        } else {
            walk(react, k, id, At::In(me), dir, holders, out, videos, screens);
        }
    }
    out.order.insert(parent, list);
}

/// A stylesheet's `url()`s that name the composition's files as what raea-runtime-js's @font-face and backgrounds read
/// (raea-runtime-bundle's css_file_urls: a file URL on Windows, #28), relative ones from the edit's folder.
pub(crate) fn media_urls(text: &str, dir: &Path) -> String {
    raea_runtime_react::scene::css_file_urls(text, dir, dir, |_| ())
}

/// A <Video>'s crop (raea-page's `fit_rect`) at source time `at`, from its framing="face" and punchIn (motion.feature,
/// "How a Video fills its box"): the largest face's eyes 38% down the box and the face centred across, at cover times
/// punchIn; punchIn alone zooms on the middle. An objectPosition or transform in its style is the author's framing,
/// and wins. A take framing="face" can't frame (no raea-perceive, no face in it) refuses the render, never centred.
pub(crate) fn video_fit(props: Option<&Map<String, Value>>, file: &Path, at: f64) -> Option<VideoFit> {
    let p = props?;
    let face = p.get("data-framing").and_then(Value::as_str) == Some("face");
    let punch = raea_runtime_react::scene::num(p, "data-punch-in").filter(|s| *s > 0.0);
    if !face && punch.is_none() {
        return None;
    }
    let style = p.get("style").and_then(Value::as_object);
    if style.is_some_and(|s| ["objectPosition", "transform"].iter().any(|k| s.get(*k).is_some_and(|v| !v.is_null()))) {
        return None;
    }
    let scale = punch.unwrap_or(1.0) as f32;
    if !face {
        return Some(VideoFit { at: (0.5, 0.5), to: (0.5, 0.5), scale });
    }
    match crate::faces::track(file) {
        Ok(t) => t.eyes_at(at).map(|eyes| VideoFit { at: eyes, to: (0.5, 0.38), scale }),
        Err(e) => {
            REFUSED.with(|c| c.borrow_mut().push(e));
            None
        }
    }
}

/// An element's props as the page takes them: React's own (className, style...) without the renderer's bookkeeping,
/// a 3D part's description or handlers; an image's source resolved from the composition's folder; a video's source
/// left to the renderer (its frames are set each frame).
pub(crate) fn page_props(kind: &str, props: Option<&Map<String, Value>>, dir: &Path) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(p) = props else { return out };
    for (k, v) in p {
        // (a <Sequence>'s data-raea-start kept: its CSS animations run from there, raea-runtime-js's src/anim.rs; a bevy_ui
        // <Text>'s data-raea-bevy-text kept: Blitz measures its block as bevy_text does, vendor/blitz-dom layout/inline.rs;
        // a sliced ImageNode's data-raea-bevy-slice kept: Blitz scales and tiles its border-image as bevy_ui does,
        // vendor/blitz-paint render/border_image.rs; a GlobalZIndex node's data-raea-bevy-global-z kept: Blitz ranks it in the
        // root's stacking context, vendor/blitz-dom layout/paint_tree.rs; a bevy_ui root's data-raea-blend kept: its page
        // blends in linear light, systems/page paint; a bevy_ui scroller's data-raea-bevy-scroll and an IgnoreScroll node's
        // data-raea-bevy-ignore-scroll kept: Blitz floors its offset and keeps the node in place, vendor/blitz-dom resolve.rs;
        // the source stamps data-raea-loc and data-raea-use kept for the render manifest, engine/bundle stamp.rs)
        const KEPT: [&str; 12] = ["data-raea-start", "data-raea-bevy-debug", "data-raea-bevy-text", "data-raea-bevy-slice", "data-raea-bevy-global-z", "data-raea-blend", "data-raea-bevy-scroll", "data-raea-bevy-ignore-scroll", "data-raea-bevy-scrollbar", "data-raea-bevy-thumb", "data-raea-loc", "data-raea-use"];
        if k == "children" || k == "key" || (k.starts_with("data-raea") && !KEPT.contains(&k.as_str())) || k == "data-props" || k == "data-shape" || k == "data-anim" || v.is_null() {
            continue;
        }
        if k.starts_with("on") && k.chars().nth(2).is_some_and(|c| c.is_ascii_uppercase()) {
            continue;
        }
        if kind == "video" && (k == "src" || k.starts_with("data-")) {
            continue;
        }
        if k == "style" {
            if let Value::Object(st) = v {
                // a url("x.png") in a value (a background, a border-image, a mask) found as an img's src is
                let kept = st.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.as_str().filter(|s| s.contains("url(")).map_or_else(|| v.clone(), |s| Value::String(css_urls(dir, s)))));
                out.insert(k.clone(), Value::Object(kept.collect()));
            }
            continue;
        }
        if kind == "img" && k == "src" {
            if let Some(src) = v.as_str() {
                let path = if src.contains("://") || src.starts_with("data:") { src.to_string() } else {
                    let p = raea_runtime_react::scene::media_path(dir, src);
                    if !p.exists() {
                        REFUSED.with(|c| c.borrow_mut().push(format!("missing image: {}", p.display())));
                    }
                    raea_io_home::src(&p)
                };
                out.insert(k.clone(), Value::String(path));
                continue;
            }
        }
        out.insert(k.clone(), v.clone());
    }
    out
}

/// A CSS value with each `url(..)` that names a media file as that file, as an img's src is (a background or a
/// border-image of staticFile("x.png")); one that isn't there refuses the render.
fn css_urls(dir: &Path, s: &str) -> String {
    raea_runtime_react::scene::css_file_urls(s, dir, dir, |f| REFUSED.with(|c| c.borrow_mut().push(format!("missing image: {}", f.display()))))
}

#[cfg(test)]
mod css_url_tests {
    #[test]
    fn a_media_url_becomes_its_path_and_others_stay() {
        let dir = std::env::temp_dir();
        let out = super::css_urls(&dir, r#"url("a.png") , url(https://x.io/b.png), url('data:image/png;base64,AA')"#);
        let file = raea_io_home::src(&raea_runtime_react::scene::media_path(&dir, "a.png"));
        assert_eq!(out, format!(r#"url("{file}") , url(https://x.io/b.png), url('data:image/png;base64,AA')"#));
        assert_eq!(super::css_urls(&dir, "linear-gradient(red, blue)"), "linear-gradient(red, blue)");
    }
}
