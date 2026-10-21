//! QuadsWindowRenderer, checked: what it presents to a window's surface is what the quads compositor paints for the
//! same scene, byte for byte. The window is an NSView in no window (no screen is taken); AppKit wants the main
//! thread, so this is an example, not a test: `cargo run --release --example window_renderer`.

use std::sync::Arc;

use anyrender::{ImageRenderer, PaintScene};
use peniko::kurbo::{Affine, Rect};
use peniko::{Color, Fill};

const W: u32 = 320;
const H: u32 = 240;

#[cfg(not(target_vendor = "apple"))]
fn main() {}

#[cfg(target_vendor = "apple")]
fn main() {
    use anyrender::WindowRenderer;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use wgpu::rwh::{AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle};

    struct View(std::ptr::NonNull<std::ffi::c_void>);
    unsafe impl Send for View {}
    unsafe impl Sync for View {}
    impl HasWindowHandle for View {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(self.0))) })
        }
    }
    impl HasDisplayHandle for View {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())) })
        }
    }
    let view: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view = Arc::new(View(std::ptr::NonNull::new(view as *mut std::ffi::c_void).unwrap()));

    let draw = |s: &mut dyn FnMut(Rect, Color)| {
        s(Rect::new(0.0, 0.0, W as f64, H as f64), Color::from_rgb8(20, 24, 31));
        s(Rect::new(20.0, 20.0, 180.0, 90.0), Color::from_rgb8(230, 80, 40));
        s(Rect::new(100.5, 60.25, 300.0, 200.0), Color::from_rgba8(40, 160, 255, 160));
    };
    let mut r = raea::quads::QuadsWindowRenderer::without_vsync();
    raea::quads::read_next_present();
    r.resume(view, W, H, || {});
    assert!(r.complete_resume(), "the surface wasn't made");
    r.render(|sc| draw(&mut |rect, c| sc.fill(Fill::NonZero, Affine::IDENTITY, c, None, &rect)));
    let (tw, th, shown) = raea::quads::take_present().expect("nothing presented");
    assert_eq!((tw, th), (W, H));
    let mut painted = Vec::new();
    raea::quads::paint_with(W, H, true, &mut painted, |sc| draw(&mut |rect, c| sc.fill(Fill::NonZero, Affine::IDENTITY, c, None, &rect))).unwrap();
    // the drawable holds the page's colour bytes as they are (opaque here, so premultiplied = straight)
    let off = shown.chunks_exact(4).zip(painted.chunks_exact(4)).filter(|(a, b)| a[..3] != b[..3]).count();
    assert_eq!(off, 0, "{off} pixels differ between the window and the compositor's paint");
    println!("window renderer: {W}x{H} presented, identical to the compositor's paint");

    // see-through: only what's painted is opaque; the rest of the drawable is clear, for a view under the window
    let view2: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view2 = Arc::new(View(std::ptr::NonNull::new(view2 as *mut std::ffi::c_void).unwrap()));
    let mut t = raea::quads::QuadsWindowRenderer::without_vsync().transparent();
    raea::quads::read_next_present();
    t.resume(view2, W, H, || {});
    assert!(t.complete_resume(), "the transparent surface wasn't made");
    let card = Rect::new(40.0, 30.0, 200.0, 120.0);
    t.render(|sc| sc.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(230, 80, 40), None, &card));
    let (_, _, px) = raea::quads::take_present().expect("nothing presented");
    let at = |x: u32, y: u32| &px[((y * W + x) * 4) as usize..((y * W + x) * 4 + 4) as usize];
    assert_eq!(at(10, 10), &[0, 0, 0, 0], "outside the card should be clear");
    assert_eq!(at(100, 70), &[230, 80, 40, 255], "inside the card should be the card");
    println!("window renderer, transparent: clear outside what's painted, the paint inside");

    // a frame the compositor can't do (a blend over the page, as a backdrop filter is): painted on the CPU, presented
    let view3: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view3 = Arc::new(View(std::ptr::NonNull::new(view3 as *mut std::ffi::c_void).unwrap()));
    let mut c = raea::quads::QuadsWindowRenderer::without_vsync();
    raea::quads::read_next_present();
    c.resume(view3, W, H, || {});
    let scene = |p: &mut anyrender::Scene| {
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(20, 120, 200), None, &Rect::new(0.0, 0.0, W as f64, H as f64));
        p.push_layer(Fill::NonZero, peniko::Mix::Difference, 1.0, Affine::IDENTITY, &Rect::new(60.0, 40.0, 260.0, 200.0), None, None);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(240, 200, 40), None, &Rect::new(60.0, 40.0, 260.0, 200.0));
        p.pop_layer();
    };
    c.render(|sc| scene(sc));
    let (_, _, shown) = raea::quads::take_present().expect("the CPU frame wasn't presented");
    let mut r = anyrender_vello_cpu::VelloCpuImageRenderer::new(W, H);
    let mut cpu = Vec::new();
    let mut rec = anyrender::Scene::new();
    scene(&mut rec);
    r.render_to_vec(|p| {
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, W as f64, H as f64));
        p.append_scene(rec.clone(), Affine::IDENTITY)
    }, &mut cpu);
    let off = shown.chunks_exact(4).zip(cpu.chunks_exact(4)).filter(|(a, b)| a[..3] != b[..3]).count();
    assert_eq!(off, 0, "{off} pixels differ between the presented CPU frame and vello_cpu's paint");
    println!("window renderer, CPU frame: presented, identical to vello_cpu's paint");

    // a backdrop blur (void's glass sheets): on the GPU in a window, close to vello_cpu's (a different blur, so not
    // byte for byte); both written beside each other for a look
    let view4: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view4 = Arc::new(View(std::ptr::NonNull::new(view4 as *mut std::ffi::c_void).unwrap()));
    let mut g = raea::quads::QuadsWindowRenderer::without_vsync();
    raea::quads::read_next_present();
    g.resume(view4, W, H, || {});
    let glass = |p: &mut anyrender::Scene| {
        for k in 0..16 {
            let c = if k % 2 == 0 { Color::from_rgb8(230, 60, 40) } else { Color::from_rgb8(30, 90, 220) };
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(k as f64 * 20.0, 0.0, k as f64 * 20.0 + 20.0, H as f64));
        }
        let sheet = peniko::kurbo::RoundedRect::new(50.0, 50.0, 270.0, 190.0, 16.0);
        p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &sheet, None, Some(Arc::new(anyrender::Filter::single(anyrender::filters::FilterEffect::blur(12.0)))));
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(255, 255, 255, 60), None, &sheet);
        p.pop_layer();
    };
    let t0 = std::time::Instant::now();
    g.render(|sc| glass(sc));
    let gpu_ms = t0.elapsed().as_secs_f64() * 1e3;
    let (_, _, shown) = raea::quads::take_present().expect("the backdrop frame wasn't presented");
    let mut r = anyrender_vello_cpu::VelloCpuImageRenderer::new(W, H);
    let mut cpu = Vec::new();
    let mut rec = anyrender::Scene::new();
    glass(&mut rec);
    let t1 = std::time::Instant::now();
    r.render_to_vec(|p| {
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, W as f64, H as f64));
        p.append_scene(rec.clone(), Affine::IDENTITY)
    }, &mut cpu);
    let cpu_ms = t1.elapsed().as_secs_f64() * 1e3;
    // vello_cpu leaves the backdrop unblurred (anyrender_vello_cpu ignores backdrop filters), so it's the reference only outside
    // the sheet; inside, the stripes' contrast along a row must mostly be gone
    let px_at = |v: &[u8], x: u32, y: u32| [v[((y * W + x) * 4) as usize] as f64, v[((y * W + x) * 4 + 2) as usize] as f64];
    let outside_same = (0..W).all(|x| (0..H).filter(|y| !(48..=192).contains(y) || !(48..=272).contains(&x)).all(|y| px_at(&shown, x, y) == px_at(&cpu, x, y)));
    let spread = |v: &[u8]| {
        let reds: Vec<f64> = (80..240).map(|x| px_at(v, x, 120)[0]).collect();
        reds.iter().cloned().fold(f64::MIN, f64::max) - reds.iter().cloned().fold(f64::MAX, f64::min)
    };
    let (blurred, sharp) = (spread(&shown), spread(&cpu));
    // a second frame, its pipelines made
    let t2 = std::time::Instant::now();
    g.render(|sc| glass(sc));
    let again_ms = t2.elapsed().as_secs_f64() * 1e3;
    if let Ok(dir) = std::env::var("RAEA_WINDOW_CHECK_OUT") {
        let _ = image::save_buffer(format!("{dir}/backdrop-gpu.png"), &shown, W, H, image::ColorType::Rgba8);
        let _ = image::save_buffer(format!("{dir}/backdrop-cpu.png"), &cpu, W, H, image::ColorType::Rgba8);
    }
    println!("window renderer, backdrop blur on the GPU: outside the sheet identical {outside_same}; red across a row under it {blurred:.0} (unblurred {sharp:.0}); frame {gpu_ms:.1} ms first, {again_ms:.1} ms after (vello_cpu {cpu_ms:.1} ms, unblurred)");
    assert!(outside_same, "the page outside the sheet changed");
    assert!(blurred < sharp * 0.35, "under the sheet isn't blurred: {blurred:.0} vs {sharp:.0}");

    // the glass sheets' own filter, `saturate(160%) blur(28px)`: a colour step with the blur stays on the GPU (it sent
    // the frame to the CPU, which ignores backdrop filters: tinted, not blurred), blurred as before, more saturated
    let sat_glass = |p: &mut anyrender::Scene| {
        for k in 0..16 {
            let c = if k % 2 == 0 { Color::from_rgb8(230, 60, 40) } else { Color::from_rgb8(30, 90, 220) };
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(k as f64 * 20.0, 0.0, k as f64 * 20.0 + 20.0, H as f64));
        }
        let sheet = peniko::kurbo::RoundedRect::new(50.0, 50.0, 270.0, 190.0, 16.0);
        let f = anyrender::Filter::linear_list([anyrender::filters::FilterEffect::saturate(1.6), anyrender::filters::FilterEffect::blur(12.0)].into_iter());
        p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &sheet, None, Some(Arc::new(f)));
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(255, 255, 255, 60), None, &sheet);
        p.pop_layer();
    };
    raea::quads::read_next_present();
    g.render(|sc| sat_glass(sc));
    let (_, _, sat) = raea::quads::take_present().expect("the saturate + blur frame wasn't presented on the GPU");
    let chroma = |v: &[u8]| (100..220).map(|x| { let i = ((120 * W + x) * 4) as usize; let c = [v[i] as f64, v[i + 1] as f64, v[i + 2] as f64]; c.iter().cloned().fold(f64::MIN, f64::max) - c.iter().cloned().fold(f64::MAX, f64::min) }).sum::<f64>() / 120.0;
    let (sat_spread, c_plain, c_sat) = (spread(&sat), chroma(&shown), chroma(&sat));
    println!("window renderer, saturate(160%) blur on the GPU: red across a row under it {sat_spread:.0}; chroma under it {c_sat:.0} (blur alone {c_plain:.0})");
    assert!(sat_spread < sharp * 0.35, "under the saturated sheet isn't blurred: {sat_spread:.0} vs {sharp:.0}");
    assert!(c_sat > c_plain * 1.2, "under the sheet isn't more saturated: {c_sat:.0} vs {c_plain:.0}");

    // a texture the GPU holds (the stage composited into the chrome) in a frame that goes to the CPU (a blend): the
    // texture still shows, drawn on the GPU under the CPU's frame
    let (_, _, device, queue) = raea::quads::handles().unwrap();
    let tex = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
    let green: Vec<u8> = (0..64 * 64).flat_map(|_| [20u8, 200, 60, 255]).collect();
    queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, &green, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(64 * 4), rows_per_image: None }, wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 });
    let id = 4242u64;
    raea::quads::set_texture(id, tex.create_view(&Default::default()), 64, 64).unwrap();
    let view5: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view5 = Arc::new(View(std::ptr::NonNull::new(view5 as *mut std::ffi::c_void).unwrap()));
    let mut t5 = raea::quads::QuadsWindowRenderer::without_vsync();
    raea::quads::read_next_present();
    t5.resume(view5, W, H, || {});
    t5.render(|sc| {
        let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(id), sampler: peniko::ImageSampler::default() };
        sc.fill(Fill::NonZero, Affine::translate((20.0, 20.0)), anyrender::Paint::Resource(brush), None, &Rect::new(0.0, 0.0, 64.0, 64.0));
        sc.push_layer(Fill::NonZero, peniko::Mix::Difference, 1.0, Affine::IDENTITY, &Rect::new(150.0, 40.0, 260.0, 200.0), None, None);
        sc.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(240, 200, 40), None, &Rect::new(150.0, 40.0, 260.0, 200.0));
        sc.pop_layer();
    });
    let (_, _, px5) = raea::quads::take_present().expect("the frame wasn't presented");
    let at5 = |x: u32, y: u32| px5[((y * W + x) * 4) as usize..((y * W + x) * 4 + 3) as usize].to_vec();
    assert_eq!(at5(50, 50), vec![20, 200, 60], "the texture didn't show in a CPU frame");
    println!("window renderer, a GPU texture in a CPU frame: shown");

    // a texture drawn before it's set (the stage's first frames) shows once it is; and inside two rounded clips (a
    // stretch vello_cpu draws), it shows over that stretch
    let late = 4343u64;
    let view6: *mut AnyObject = unsafe { msg_send![class!(NSView), new] };
    let view6 = Arc::new(View(std::ptr::NonNull::new(view6 as *mut std::ffi::c_void).unwrap()));
    let mut t6 = raea::quads::QuadsWindowRenderer::without_vsync();
    t6.resume(view6, W, H, || {});
    let frame6 = |sc: &mut anyrender::Scene| {
        let rr = |x: f64| peniko::kurbo::RoundedRect::new(x, x, W as f64 - x, H as f64 - x, 14.0);
        sc.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(250, 250, 250), None, &Rect::new(0.0, 0.0, W as f64, H as f64));
        sc.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rr(10.0));
        sc.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &rr(20.0));
        let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(late), sampler: peniko::ImageSampler::default() };
        sc.fill(Fill::NonZero, Affine::translate((40.0, 40.0)), anyrender::Paint::Resource(brush), None, &Rect::new(0.0, 0.0, 64.0, 64.0));
        sc.pop_layer();
        sc.pop_layer();
    };
    t6.render(|sc| frame6(sc));
    let tex2 = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
    queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &tex2, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, &green, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(64 * 4), rows_per_image: None }, wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 });
    raea::quads::set_texture(late, tex2.create_view(&Default::default()), 64, 64).unwrap();
    raea::quads::read_next_present();
    t6.render(|sc| frame6(sc));
    let (_, _, px6) = raea::quads::take_present().expect("the frame wasn't presented");
    let at6 = |x: u32, y: u32| px6[((y * W + x) * 4) as usize..((y * W + x) * 4 + 3) as usize].to_vec();
    assert_eq!(at6(70, 70), vec![20, 200, 60], "a texture set after its first frame, inside two rounded clips, didn't show");
    println!("window renderer, a texture set late inside nested rounded clips: shown");
}
