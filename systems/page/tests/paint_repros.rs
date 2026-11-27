//! 0.1.11's paint reports, each the issue's own repro as HTML, painted by blitz-paint into a recording scene and read
//! back: what is filled, in what colour or brush, and under which clips (no GPU, so every runner checks them).
use anyrender::recording::RenderCommand;
use anyrender::{Paint, Scene};
use bevy::math::UVec2;
use vello::kurbo::{Point, Rect, Shape};
use raea_sys_page::{Page, config};

fn painted(html: &str, w: u32, h: u32) -> Scene {
    let mut doc = blitz_html::HtmlDocument::from_html(html, config(Page { size: UVec2::new(w, h), scale: 1.0 })).into_inner();
    doc.resolve(0.0);
    let mut scene = Scene::new();
    blitz_paint::paint_scene(&mut scene, &mut doc, 1.0, w, h, 0, 0);
    scene
}

/// Each fill: its brush, its box on the page, and the boxes of the layers it's clipped by.
fn fills(scene: &Scene) -> Vec<(Paint, Rect, Vec<Rect>)> {
    let (mut clips, mut out) = (Vec::new(), Vec::new());
    for c in &scene.commands {
        match c {
            RenderCommand::PushLayer(l) => clips.push(l.transform.transform_rect_bbox(l.clip.bounding_box())),
            RenderCommand::PushClipLayer(l) => clips.push(l.transform.transform_rect_bbox(l.clip.bounding_box())),
            RenderCommand::PopLayer => drop(clips.pop()),
            RenderCommand::Fill(f) => out.push((f.brush.clone(), f.transform.transform_rect_bbox(f.shape.bounding_box()), clips.clone())),
            _ => {}
        }
    }
    out
}

fn rgb(p: &Paint) -> Option<[u8; 3]> {
    match p {
        Paint::Solid(c) => {
            let c = c.to_rgba8();
            (c.a > 0).then_some([c.r, c.g, c.b])
        }
        _ => None,
    }
}

/// What of `color` is drawn: the fills in it, each cut by its clips (None when a clip leaves none of it).
fn drawn(scene: &Scene, color: [u8; 3]) -> Vec<Rect> {
    fills(scene)
        .into_iter()
        .filter(|(p, ..)| rgb(p) == Some(color))
        .filter_map(|(_, r, clips)| {
            let cut = clips.iter().fold(r, |a, c| a.intersect(*c));
            (cut.width() > 0.0 && cut.height() > 0.0).then_some(cut)
        })
        .collect()
}

/// raea #250: `clip-path: inset(0 0 0 100%)` clips the element away (it drew whole); 99% leaves a sliver
#[test]
fn a_clip_path_inset_that_clips_everything_draws_nothing() {
    let page = |inset: &str| {
        format!(
            r#"<body style="margin:0;background:#fff"><div style="padding:40px"><div style="position:relative;width:400px;height:300px;background:#0f0">
            <div style="position:absolute;inset:0;background:#f00;clip-path:inset({inset})"></div></div></div></body>"#
        )
    };
    for inset in ["0 0 0 100%", "100% 0 0 0", "0 60% 0 50%", "0 0 0 120%"] {
        let s = painted(&page(inset), 700, 400);
        assert_eq!(drawn(&s, [255, 0, 0]), vec![], "inset({inset}) drew red");
        assert_eq!(drawn(&s, [0, 255, 0]).len(), 1, "inset({inset}) lost the green box");
    }
    let sliver = drawn(&painted(&page("0 0 0 99%"), 700, 400), [255, 0, 0]);
    assert_eq!(sliver.len(), 1);
    assert!((sliver[0].width() - 4.0).abs() < 0.01 && (sliver[0].height() - 300.0).abs() < 0.01, "{sliver:?}");
}

/// raea #251: an <svg> with `overflow: visible` draws past its viewport (a 1x1 svg drew nothing); without it, the
/// UA sheet still clips it to its box, as Chrome does
#[test]
fn an_svg_with_overflow_visible_draws_past_its_viewport() {
    let page = |overflow: &str| {
        format!(
            r##"<body style="margin:0;background:#fff"><svg xmlns="http://www.w3.org/2000/svg" style="position:absolute;left:0;top:0;{overflow}" width="1" height="1">
            <circle cx="200" cy="200" r="80" fill="#ff6600"/><polygon points="0,-40 32,24 -28,20" transform="translate(400 200) rotate(30)" fill="#3366ff"/></svg></body>"##
        )
    };
    let s = painted(&page("overflow:visible"), 700, 400);
    let circle = drawn(&s, [255, 102, 0]);
    assert_eq!(circle.len(), 1, "the circle: {circle:?}");
    assert!(circle[0].contains(Point::new(200.0, 200.0)) && circle[0].width() > 150.0, "{circle:?}");
    let triangle = drawn(&s, [51, 102, 255]);
    assert!(triangle.len() == 1 && triangle[0].contains(Point::new(400.0, 200.0)), "the triangle: {triangle:?}");
    // the UA's clip, and an author's overflow: hidden, keep it in its 1x1 box
    for overflow in ["", "overflow:hidden"] {
        let s = painted(&page(overflow), 700, 400);
        assert_eq!(drawn(&s, [255, 102, 0]), vec![], "{overflow:?} drew the circle past the box");
    }
}

/// raea #248: a background-image on an inline element is drawn on each of its line fragments (it drew nothing, its
/// background-color did), laid out across the fragments side by side, so `background-size: 10% 100%` covers part of the
/// first line's fragment and nothing of the others'
#[test]
fn an_inline_elements_background_image_fills_its_line_fragments() {
    let page = |size: &str| {
        format!(
            r#"<body style="margin:0;background:#fff"><div style="padding:40px;font-size:40px;line-height:50px"><div style="width:300px">Before <span id="m" style="background-image:linear-gradient(#FFD43B, #FFD43B);background-repeat:no-repeat;background-size:{size}">this marked phrase wraps over lines</span> after.</div></div></body>"#
        )
    };
    let gradients = |s: &Scene| -> Vec<Rect> {
        fills(s)
            .into_iter()
            .filter(|(p, ..)| matches!(p, Paint::Gradient(_)))
            .filter_map(|(_, r, clips)| {
                let cut = clips.iter().fold(r, |a, c| a.intersect(*c));
                (cut.width() > 1.0 && cut.height() > 1.0).then_some(cut)
            })
            .collect()
    };
    let full = gradients(&painted(&page("100% 100%"), 700, 400));
    assert!(full.len() >= 2, "a fill on each line's fragment: {full:?}");
    let mut tops: Vec<i64> = full.iter().map(|r| r.y0.round() as i64).collect();
    tops.dedup();
    assert!(tops.len() >= 2, "on more than one line: {full:?}");
    // 10% of the fragments side by side: on the first line alone
    let half = gradients(&painted(&page("10% 100%"), 700, 400));
    assert!(!half.is_empty() && half.iter().all(|r| r.y0 < full.iter().map(|f| f.y0).fold(f64::MAX, f64::min) + 1.0), "{half:?} vs {full:?}");
}
