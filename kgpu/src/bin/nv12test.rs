//! The NV12 kernel (kernels/nv12.k) on a real frame: GPU (an sRGB texture read through a Unorm view, as the engine's
//! render target will be) = the CPU interpreter of the same K = the native reference, bit for bit.

use std::collections::HashMap;
use std::time::Instant;

use raea_kgpu::{Cpu, Gpu, nv12_reference, parse};

fn main() -> Result<(), String> {
    let gpu = Gpu::new()?;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/fixtures");
    let fr = raea_media::backend("ffmpeg")?.open_video(&fixtures.join("take-h264.mp4"))?.frame_at(5.0)?;
    let (w, h) = (fr.width as usize, fr.height as usize);
    // the frame plus the tag row
    let mut rgba = fr.rgba.clone();
    rgba.extend(std::iter::repeat_n(0u8, w * 4));
    let tag = (h * w + 8) * 4;
    rgba[tag..tag + 4].copy_from_slice(&[12, 34, 56, 255]);
    let words: Vec<u32> = bytemuck::cast_slice(&rgba).to_vec();

    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w as u32, height: h as u32 + 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w as u32 * 4), rows_per_image: None },
        wgpu::Extent3d { width: w as u32, height: h as u32 + 1, depth_or_array_layers: 1 },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });

    let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels/nv12.k")).map_err(|e| e.to_string())?;
    let k = parse(&src)?;
    let sc: HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
    let n = w * h / 4 + w * h / 8 + 1;
    let c = gpu.compile_tex(&k, &[], &["T"], &sc, &["px"])?;
    let _ = gpu.run_tex(&c, &[], &[&view], n)?;
    let mut best = f64::INFINITY;
    let mut best_compute = f64::INFINITY;
    let mut g = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let (o, times) = gpu.run_tex(&c, &[], &[&view], n)?;
        best = best.min(t.elapsed().as_secs_f64());
        best_compute = best_compute.min(times.compute);
        g = o.into_iter().next().unwrap();
    }
    let t = Instant::now();
    let native = nv12_reference(&words, w, h);
    let native_ms = t.elapsed().as_secs_f64() * 1e3;
    let cpu = Cpu { buffers: [("T", &words[..])].into(), scalars: sc.clone() };
    let t = Instant::now();
    let interp = cpu.run(&k, n, &["px"])?.into_iter().next().unwrap();
    let interp_ms = t.elapsed().as_secs_f64() * 1e3;
    let first = |a: &[u32], b: &[u32]| (0..a.len().min(b.len())).find(|&i| a[i] != b[i]);
    println!(
        "{}",
        serde_json::json!({"frame": [w, h], "words": n, "bytes": n * 4, "gpu_eq_interpreter": g == interp, "gpu_eq_native": g == native,
            "first_diff_gpu_native": first(&g, &native), "tag_word": format!("{:08x}", g[n - 1]),
            "gpu_ms_incl_readback": best * 1e3, "gpu_compute_ms": best_compute * 1e3, "native_single_thread_ms": native_ms, "interpreter_ms": interp_ms})
    );
    Ok(())
}
