//! Whether Vello drew a scene at all. Past one of its bump buffers, Vello's fine stage returns before writing a texel
//! (vello_shaders fine.wgsl: `ptcl[0] == ~0u`) and `render_to_texture` still returns Ok, so the target keeps what it
//! held: a pooled layer texture its last frame's picture, a new one nothing (#23). A sentinel texel written at (0,0)
//! before the draw and read back after says which: the fine stage writes every texel when it runs, and never this one,
//! since its colour is straight (an alpha of 0 comes out with a colour of about 0, never magenta).
//! The read back isn't waited for (a wait on every draw cost a fifth of a render's speed): its answer comes in the
//! wait for the frame's own pixels. A skipped draw puts Vello's own bump sizes on for the rest of the process and is
//! counted (raea_gpu_device::check::skipped_draw), and the frame's check (`skip_checked`) draws the frames since again
//! at those sizes, or refuses one skipped at them too, so the render fails naming it.
//! RAEA_PAGE_DRAWN_CHECK=0 draws without the check (for measuring what it costs; a skipped draw goes unseen again).
//! And whether it drew every image: one that finds no room in Vello's image atlas is drawn at no size, again with no
//! error. A layer with a filter or a perspective transform is such an image, the size of what it draws, so a page of
//! many lost its nearest ones. The resolve counts them (vendor/vello_encoding `take_images_left_out`) and the frame is
//! refused (raea_gpu_device::check::refuse), so the render fails naming it.

use std::sync::atomic::{AtomicBool, Ordering};

/// The sentinel, as Rgba8Unorm bytes: magenta at an alpha of 0.
const SENTINEL: [u8; 4] = [255, 0, 255, 0];

/// On once a scene overflowed raea's smaller sizes: every draw since has Vello's own (lib.rs `redraw_full` keeps it).
pub(crate) static STAY_FULL: AtomicBool = AtomicBool::new(false);

/// What the texture's usage needs for the check: the sentinel written in and read back.
pub const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::COPY_SRC.union(wgpu::TextureUsages::COPY_DST);

/// The device the draws are on, polled without waiting for read backs that are done (`settle`).
static DEVICE: std::sync::OnceLock<wgpu::Device> = std::sync::OnceLock::new();

/// The read backs that are done answered: raea_gpu_device::check calls it before each frame's check (lib.rs).
pub fn settle() {
    if let Some(d) = DEVICE.get() {
        let _ = d.poll(wgpu::PollType::Poll);
    }
}

/// The last draw's sentinel read back, as submitted (`wait_drawn` waits for it).
static LAST: std::sync::Mutex<Option<wgpu::SubmissionIndex>> = std::sync::Mutex::new(None);

/// Every draw's read back answered, waiting for the last one: the fed render's check (export fed.rs), made before its
/// frame is handed to the encoder, since once encoded there's no frame left to draw again. It waits only for the page
/// draws the frame's composite waits for anyway.
pub fn wait_drawn() {
    let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let (Some(d), Some(i)) = (DEVICE.get(), last) {
        let _ = d.poll(wgpu::PollType::Wait { submission_index: Some(i), timeout: None });
    }
}

/// `scene` drawn by Vello into `target` with `params`, its sentinel's read back on its way (`what` names it).
pub fn render(renderer: &mut vello::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, scene: &vello::Scene, target: &wgpu::Texture, params: &vello::RenderParams, what: &str) -> Result<(), String> {
    let check = std::env::var_os("RAEA_PAGE_DRAWN_CHECK").is_none_or(|v| v != "0");
    let texel = wgpu::TexelCopyTextureInfo { texture: target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All };
    let one = wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
    let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: None, rows_per_image: None };
    if check {
        queue.write_texture(texel, &SENTINEL, layout, one);
    }
    // (the count is this draw's: a resolve that only measures the scene, RAEA_VELLO_BUMP_USE's, counts its own before it)
    vello::low_level::vello_encoding::take_images_left_out();
    renderer.render_to_texture(device, queue, scene, &target.create_view(&Default::default()), params).map_err(|e| format!("vello, {what}: {e}"))?;
    let left_out = vello::low_level::vello_encoding::take_images_left_out();
    if left_out > 0 {
        let side = vello::low_level::vello_encoding::max_atlas_size();
        let why = format!("{what} ({}x{}) was drawn without {left_out} of its images: Vello's image atlas ({side} px a side, this GPU's largest texture) has no room for them. Each element with a filter or a perspective transform is an image the size of what it draws, so fewer of them, or smaller ones, draw it", params.width, params.height);
        bevy::log::error!("raea-page: {why}");
        raea_gpu_device::check::refuse(why);
    }
    if !check {
        return Ok(());
    }
    DEVICE.get_or_init(|| device.clone());
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("raea-page drawn?"), size: 4, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("raea-page drawn?") });
    enc.copy_texture_to_buffer(texel, wgpu::TexelCopyBufferInfo { buffer: &buf, layout }, one);
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(queue.submit([enc.finish()]));
    let (read, what, (w, h)) = (buf.clone(), what.to_string(), (params.width, params.height));
    buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let skipped = r.is_ok() && read.slice(..).get_mapped_range().is_ok_and(|d| d[..4] == SENTINEL);
        read.unmap();
        if skipped {
            if !STAY_FULL.swap(true, Ordering::Relaxed) {
                vello::low_level::vello_encoding::set_bump_full(true);
                eprintln!("raea-page: {what} ({w}x{h}) is past raea's Vello buffers and wasn't drawn: it's drawn again, and every page from now on, at Vello's own sizes");
            }
            raea_gpu_device::check::skipped_draw(format!("{what} ({w}x{h}) wasn't drawn: its scene is past Vello's bump buffers even at Vello's own sizes (fewer full-frame layers under one filter, or a smaller render, draws it)"));
        }
    });
    Ok(())
}
