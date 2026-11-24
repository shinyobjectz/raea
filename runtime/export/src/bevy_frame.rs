//! A frame's pixels from the Bevy page: the snapshot, the 3D under the page, and the fed GPU path.

use super::*;

/// A frame black on two draws whose page shows `paint`: one the GPU lost, a warning over its run of frames (#96)
pub(crate) fn lost(f: i64, paint: Option<String>) { if let Some(p) = paint { raea_core_facts::finding::said_frame(raea_core_facts::Finding::new("black-frame", raea_core_facts::Level::Warn, format!("drew all black on two draws, though the page shows {p}"), "raea render").fix("the GPU dropped this frame's drawing: fewer or smaller filter layers at this size, or render it again"), f); } }

/// One frame on the bevy_ui page (src/page.rs, P26): the frame's sound, mix and marker read from React's tree as
/// snapshot_retained reads them, the page made to match the tree, drawn and read back. 3D isn't on this page yet.
pub(crate) fn snapshot_bevy(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx) -> R<Frame> {
    let f = cx.now;
    let (px, mut pics) = bevy_page_frame(world, page, cx)?;
    let (px, twice) = raea_gpu_device::check::black_checked(f, px, raea_sys_page::redraw_full, || {
        // the frame drawn again is the same frame: its manifest entry replaces the first draw's, as the fed render's
        // redraw does (a held first frame that's all black was listed twice, each later frame one place late)
        if let Some(m) = cx.manifest.as_mut() {
            m.pop();
        }
        let (px, again) = bevy_page_frame(world, page, cx)?;
        pics = again;
        Ok(px)
    })?;
    if twice {
        raea_gpu_device::check::black_twice(f, page.canary())?;
        lost(f, page.visible_paint());
    }
    let stored = store_page(&px, page.w, page.h, cx.work)?;
    // each <Screen>'s picture: stored as a page is (a fully clear one too, so the screen shows it clear)
    let mut screens = Vec::new();
    for (name, w, h, rgba) in pics {
        if let Some(hash) = store_page(&rgba, w, h, cx.work)?.or_else(|| store_clear(w, h, cx.work).ok()) {
            screens.push((name, hash));
        }
    }
    Ok(Frame { screens, mix: cx.mix, page: stored, sounds: cx.sounds.clone(), scene: cx.scene.clone(), clear: cx.clear.clone(), same: false })
}

/// A live scene's error with the frame it came on (docs/stage-guards.feature SG4), unless it names one already.
fn on_frame(r: Result<(), String>, now: i64) -> Result<(), String> {
    r.map_err(|m| if m.starts_with("frame ") { m } else { format!("frame {now}: {m}") })
}

/// One frame on the bevy_ui page over its 3D drawn live (cx.scene3d, the engine render_fed opens: handles resolved from
/// the edit's folder, world calls answered): each <Screen>'s picture handed to the engine, the 3D drawn as the fed render
/// draws it and its target read back, and the page laid over it. Straight RGBA, opaque (with --alpha, transparent where nothing is drawn: #114).
/// The 3D's target (premultiplied: Bevy blends into a transparent clear) as a straight picture. Light in the clear
/// (colour above its alpha) can't be straight: its alpha is raised to its brightest channel, which is exact over black
/// and darkens a lit page under it only by that light's own share.
pub(crate) fn scene_straight(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        let a = p[0].max(p[1]).max(p[2]).max(p[3]) as u32;
        if p[3] == 255 || a == 0 {
            continue;
        }
        for c in &mut p[..3] {
            *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
        }
        p[3] = a as u8;
    }
}

pub(crate) fn bevy_over_3d(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx, fps: f64) -> R<Vec<u8>> {
    // a game's: this frame's 3D, just stepped, set on its engine (opened at the first frame with any), as the Blitz
    // fed render's snapshot_retained does
    if let Some(ls) = cx.live3d.as_mut() {
        on_frame(live_scene(world, ls, &mut cx.scene3d, (page.w, page.h), fps, cx.dir), cx.now)?;
    }
    let (px, pics) = bevy_page_frame(world, page, cx)?;
    // with --alpha (#114) what the page and its 3D leave empty stays transparent, as a page alone does; else over black
    let alpha = crate::ALPHA.load(std::sync::atomic::Ordering::Relaxed);
    let opaque = |mut px: Vec<u8>| {
        for p in px.chunks_exact_mut(4).filter(|_| !alpha) {
            p[3] = 255;
        }
        px
    };
    let Some(e) = cx.scene3d.as_mut() else {
        // no 3D yet: the page alone, over black as the fed render's page buffer starts
        return Ok(opaque(px));
    };
    for (name, w, h, rgba) in pics {
        e.set_screen_picture(&name, w, h, rgba);
    }
    // a camera inside a page element (docs/sdk.feature S3a): its view drawn into that element's box of the frame
    let view = if page.has_slot() { None } else { page.view_box() };
    e.set_view_box(view);
    let mats = page.set_ui_materials(e.world_mut())?;
    // one draw a frame, as the fed render draws it (drawn again, a frame's auto exposure and temporal passes move on);
    // <Pathtracer samples={N}> (or RAEA_LIGHT_SPP=N) draws it N times, the last kept, so a path-traced reference converges (docs/realism.feature LT1),
    // its accumulation started afresh so the frame before isn't in it
    let spp = raea_sys_rt::pathtracer::samples(e.world_mut());
    if spp > 1 {
        raea_render::restart_pathtracer(e.world_mut());
    }
    for _ in 1..spp {
        e.draw(cx.now);
    }
    e.draw(cx.now);
    raea_sys_frame::gpu::said(e.world_mut(), cx.now)?;
    let px = if mats { page.read_ui_materials(e.world()).and_then(|_| page.frame())? } else { px };
    let three = e.read_target()?;
    let (tw, th) = e.size();
    // layered: the 3D (clear where it has nothing) is its slot's picture, in the page's paint order; the page drawn
    // again with it. The fed render's slot draws the 3D's texture as premultiplied (quads' texture paint), so light a
    // pass leaves in the clear (the atmosphere's sky, bloom's glow) adds over the page under it; the picture here is
    // straight, so it's made straight (scene_straight)
    if page.has_slot() {
        let mut three = three;
        scene_straight(&mut three);
        page.set_scene(tw, th, three);
        return Ok(opaque(page.frame()?));
    }
    // the camera's view as its element's picture, cropped from the frame where it was drawn; the page alone around it
    if view.is_some() != page.in_box {
        // the camera's box came or went this frame: the page synced again as that draws it (holders kept or not)
        page.in_box = view.is_some();
        page_with_holders(world, page, cx)?;
    }
    if let Some([x, y, w, h]) = view {
        let mut crop = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let at = ((row * tw + x) * 4) as usize;
            crop.extend_from_slice(&three[at..at + (w * 4) as usize]);
        }
        scene_straight(&mut crop);
        page.set_view(w, h, crop);
        return Ok(opaque(page.frame()?));
    }
    // else the page over the 3D, the 3D over black (the fed render's page buffer): the 3D is premultiplied (Bevy blends
    // into it), so over black it's its own colour, as a window shows it whatever alpha an opaque, masked or
    // alpha-to-coverage fragment wrote (3d/transparency_3d). The
    // page goes over as the fed render's compositor lays a page's fills and layers (Skia's SrcOver on the u8 grid:
    // dst * (256 - srcA) >> 8, plus the source), so a near-black 3D under a faded layer floors as it did there
    // with --alpha the 3D is over nothing: its alpha (its clear's, transparent) is laid under the page's the same way,
    // and the premultiplied result made straight for the PNG
    let mut out = three;
    for (d, s) in out.chunks_exact_mut(4).zip(px.chunks_exact(4)) {
        let k = 256 - s[3] as u32;
        for c in 0..3 {
            let t = d[c] as u32;
            d[c] = (s[c] as u32 + ((t * k) >> 8)).min(255) as u8;
        }
        d[3] = if alpha { (s[3] as u32 + ((d[3] as u32 * k) >> 8)).min(255) as u8 } else { 255 };
    }
    if alpha { scene_straight(&mut out); }
    Ok(out)
}

/// A fed render's frame buffer for the bevy_ui page (what Blitz's quads::page_buffer is for its page): w x h RGBA8 on
/// the shared device, which raea-render's fed render composites and encodes.
pub(crate) fn bevy_fed_buffer(w: u32, h: u32) -> R<wgpu::Buffer> {
    let g = raea_runtime_view::gpuview::paint_device().ok_or("a fed render needs the shared GPU device")?;
    Ok(g.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bevy_ui page buffer"),
        size: (w as u64 * h as u64 * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    }))
}

/// One frame of a fed render on the bevy_ui page: the page laid over the frame's 3D (drawn live, bevy_over_3d), or
/// the page alone, written into the frame buffer raea-render takes; the frame's sound, mix and screens as Blitz's fed frame carries them.
pub(crate) fn bevy_fed(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx, buf: &wgpu::Buffer, fps: f64) -> R<Frame> {
    let g = raea_runtime_view::gpuview::paint_device().ok_or("a fed render needs the shared GPU device")?;
    // on the GPU (B1d), unless the frames are dumped as files or RAEA_FEED_CPU=1 asks for the readback it replaced
    if std::env::var_os("RAEA_PAGE_DUMP").is_none() && std::env::var("RAEA_FEED_CPU").as_deref() != Ok("1") {
        bevy_fed_gpu(world, page, cx, buf, fps, &g.device, &g.queue)?;
    } else {
        let px = if cx.scene3d.is_some() || cx.live3d.is_some() { bevy_over_3d(world, page, cx, fps)? } else { bevy_page_frame(world, page, cx)?.0 };
        g.queue.write_buffer(buf, 0, &px);
    }
    Ok(Frame { screens: Vec::new(), mix: cx.mix, page: None, sounds: cx.sounds.clone(), scene: Vec::new(), clear: None, same: false })
}

thread_local! {
    /// The pass a fed render's frames go through into its buffer (raea-page's feed.rs), made at the first frame.
    static FEED: std::cell::RefCell<Option<raea_page::feed::Feed>> = const { std::cell::RefCell::new(None) };
}

/// bevy_fed's frame with nothing read back (docs/blitz-bevy.feature B1d): the page drawn, then copied into the frame
/// buffer by raea-page's feed pass, laid over the 3D's target there with bevy_over_3d's arithmetic, or the 3D's
/// target put in the page's layered slot as live play puts it. Each <Screen>'s picture goes to the 3D's screen on the
/// GPU too; read back only on a frame where a screen has no image in the 3D yet (its first).
pub(crate) fn bevy_fed_gpu(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx, buf: &wgpu::Buffer, fps: f64, device: &wgpu::Device, queue: &wgpu::Queue) -> R<()> {
    if let Some(ls) = cx.live3d.as_mut() {
        on_frame(live_scene(world, ls, &mut cx.scene3d, (page.w, page.h), fps, cx.dir), cx.now)?;
    }
    bevy_page_sync(world, page, cx)?;
    page.draw();
    if let Some(m) = cx.manifest.as_mut() {
        m.push(page.manifest_frame(cx.now));
    }
    let mut under = None;
    // a camera inside a page element (docs/sdk.feature S3a): its view drawn into that element's box of the frame, and
    // that box of the 3D's target the element's picture, as live play draws it; the page synced again, its holders
    // kept, when the box comes or goes
    let view = if cx.scene3d.is_some() && !page.has_slot() { page.view_box() } else { None };
    if view.is_some() != page.in_box {
        page.in_box = view.is_some();
        page_with_holders(world, page, cx)?;
        page.draw();
    }
    if let Some(e) = cx.scene3d.as_mut() {
        e.set_view_box(view);
        let screens = page.screen_textures();
        if !screens.iter().all(|(name, t)| e.copy_screen(name, t)) {
            for (name, w, h, rgba) in page.frame_all()?.1 {
                e.set_screen_picture(&name, w, h, rgba);
            }
        }
        let mats = page.set_ui_materials(e.world_mut())?;
        e.draw(cx.now);
        raea_sys_frame::gpu::said(e.world_mut(), cx.now)?;
        if mats {
            page.place_ui_materials(e.world());
            page.draw();
        }
        let target = e.target_texture().ok_or("the 3D drew no target")?;
        if page.has_slot() {
            let (tw, th) = e.size();
            page.set_scene_texture(&target, tw, th);
            page.draw();
        } else if let Some(b) = view {
            page.set_view_texture(&target, b);
            page.draw();
        } else {
            under = Some(target);
        }
    }
    let tex = page.texture().ok_or("the page's picture isn't on the GPU yet")?;
    let opaque = cx.scene3d.is_some() || cx.live3d.is_some();
    FEED.with(|f| f.borrow_mut().get_or_insert_with(|| raea_page::feed::Feed::new(device)).run(device, queue, &tex, under.as_ref(), opaque, page.w, page.h, buf));
    Ok(())
}

/// The frame's page on bevy_ui, drawn and read back (premultiplied RGBA), with each <Screen>'s picture; the frame's
/// sound, mix, marker and 3D rows collected on the way.
pub(crate) fn bevy_page_frame(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx) -> R<(Vec<u8>, Vec<(String, u32, u32, Vec<u8>)>)> {
    bevy_page_sync(world, page, cx)?;
    let (px, pics) = page.frame_all()?;
    page_dump(cx.now, &px, page.w, page.h, Some(page.boxes()))?;
    if let Some(m) = cx.manifest.as_mut() {
        m.push(page.manifest_frame(cx.now));
    }
    Ok((px, pics))
}

/// A composition's layout read answered by the frame's page (`page.layout`, docs/layout-reads.feature LR1): the page made
/// to match the React World as it is mid-settle, at frame `f`'s time, laid out, and every element's boxes by React id.
/// Nothing is drawn; the frame's own sync redoes what this did once the frame settles.
pub(crate) fn layout_answer(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx, f: i64, fps: f64, request: &str) -> R<String> {
    page.at_frame(f, fps);
    page_with_holders(world, page, cx)?;
    page.lay_out();
    Ok(page.layout_snapshot(request))
}

/// The frame's page made to match the React tree, not drawn yet; the frame's sound, mix, marker and 3D rows collected
/// on the way.
pub(crate) fn bevy_page_sync(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx) -> R<()> {
    cx.sounds.clear();
    cx.scene.clear();
    cx.marker = None;
    cx.mix = None;
    let mut q = world.query::<(&ReactKind, Option<&ReactProps>, Option<&raea_runtime_react::ReactNode>)>();
    let items: Vec<(String, Map<String, Value>, Option<u64>)> = q.iter(world).map(|(k, p, n)| (k.0.clone(), p.map(|p| p.0.clone()).unwrap_or_default(), n.map(|n| n.0))).collect();
    for (kind, mut props, node) in items {
        if props.contains_key("data-3d") {
            continue;
        }
        if let Some(f) = num(&props, "data-raea-frame") {
            cx.marker = Some(f as i64);
            continue;
        }
        if props.get("data-mix").and_then(|v| v.as_str()) == Some("true") {
            cx.mix = Some((num(&props, "data-lufs").unwrap_or(-14.0), num(&props, "data-ceiling").unwrap_or(-1.5)));
            continue;
        }
        if let (Some(line), true) = (props.get("data-speech").and_then(|v| v.as_str()), cx.manifest.is_some()) {
            cx.said.entry(line.to_string()).or_insert((cx.now, cx.now)).1 = cx.now;
        }
        if (kind == "audio" || kind == "video") && generated(&mut props, cx) {
            push_sound(cx, &props, &node.map(|n| format!("n{n}")).unwrap_or_default());
        }
    }
    // 3D: rows for raea-render, drawn under the page; the elements holding it see through, the nearest one's colour its clear
    cx.scene = scene_of(world);
    page_with_holders(world, page, cx)
}

/// The page synced with the elements holding the 3D seen through (unless it isn't under the page), the nearest one's
/// colour the 3D's clear.
pub(crate) fn page_with_holders(world: &mut World, page: &mut raea_runtime_page::BevyPage, cx: &mut Ctx) -> R<()> {
    cx.clear = None;
    let mut held = Vec::new();
    // (a layered Canvas3D drawn live is in the page's paint order: its holders stay as they are, as the fed render's;
    // so do a camera's in a box of the page, whose view is that box's picture)
    if !((cx.scene3d.is_some() || cx.live3d.is_some()) && scene_slot(world).is_some()) && !page.in_box {
        holder_entities(world, &mut held);
    }
    let mut holders = HashMap::new();
    for e in held {
        let (Some(n), Some(p)) = (world.get::<raea_runtime_react::ReactNode>(e), world.get::<ReactProps>(e)) else { continue };
        let mut p = p.0.clone();
        let (_, clear) = see_through(&mut p);
        if cx.clear.is_none() {
            cx.clear = clear;
        }
        holders.insert(n.0, p);
    }
    page.sync(world, &holders, cx.brand.as_deref())
}

/// RAEA_PAGE_DUMP=DIR: each frame's page as drawn (DIR/FRAME.png, straight RGBA) and, on the bevy_ui page, each
/// element's box by its path among the page's elements (DIR/FRAME.json): what the page parity corpus compares with
/// Chrome's (sdk/raea/bench/page-parity).
pub(crate) fn page_dump(frame: i64, px: &[u8], w: u32, h: u32, boxes: Option<Vec<(String, [f32; 4])>>) -> R<()> {
    let Some(dir) = std::env::var_os("RAEA_PAGE_DUMP").map(PathBuf::from) else { return Ok(()) };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut straight = px.to_vec();
    for p in straight.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let png = dir.join(format!("{frame}.png"));
    image::save_buffer(&png, &straight, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", png.display()))?;
    if let Some(b) = boxes {
        let m: Map<String, Value> = b.into_iter().map(|(k, r)| (k, json!(r))).collect();
        std::fs::write(dir.join(format!("{frame}.json")), Value::Object(m).to_string()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
