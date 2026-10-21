//! kernels/composite.k then kernels/nv12buf.k on real frames: the take with a punch-in (slot 0), the rotated b-roll
//! in a box at 230/256 opacity (slot 1), a caption-like premultiplied overlay, a coloured background through the
//! texture input. The b-roll is masked (on = 2): a soft-edged disc of coverage in M, byte 1. GPU = CPU interpreter =
//! native for both kernels, bit for bit.

use std::collections::HashMap;
use std::time::Instant;

use raea_kgpu::{Cpu, Gpu, Slot, composite_cpu_masked, composite_reference_masked, nv12_cpu, nv12_reference, parse};

fn cover(sw: i32, sh: i32, bx: i32, by: i32, ex: i32, ey: i32, zoom: f64, al: i32) -> Slot {
    let (bw, bh) = ((ex - bx) as f64, (ey - by) as f64);
    let (sw_f, sh_f) = (sw as f64, sh as f64);
    let (cw, ch) = if sw_f / sh_f > bw / bh { (sh_f * bw / bh, sh_f) } else { (sw_f, sw_f * bh / bw) };
    let (cw, ch) = (cw / zoom, ch / zoom);
    let f = |v: f64| (v * 65536.0).round() as i32;
    Slot { on: 1, bx, by, ex, ey, sw, sh, cx: f((sw_f - cw) / 2.0), cy: f((sh_f - ch) / 2.0), dx: f(cw / bw), dy: f(ch / bh), al }
}

fn words(b: &[u8]) -> Vec<u32> {
    let mut v = b.to_vec();
    v.resize(b.len().div_ceil(4) * 4, 0);
    bytemuck::cast_slice::<u8, u32>(&v).to_vec()
}

fn main() -> Result<(), String> {
    let gpu = Gpu::new()?;
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/fixtures");
    let ff = raea_media::backend("ffmpeg")?;
    let take = ff.open_video(&fx.join("take-h264.mp4"))?.frame_nv12_at(5.0)?;
    let broll = ff.open_video(&fx.join("broll-rot.mov"))?.frame_nv12_at(2.0)?;
    let (w, h) = (1080usize, 1920usize);
    let slots = [
        cover(take.width as i32, take.height as i32, 0, 0, w as i32, h as i32, 1.18, 256),
        Slot { on: 2, ..cover(broll.width as i32, broll.height as i32, 100, 1000, 980, 1600, 1.0, 230) },
        Slot::off(),
        Slot::off(),
    ];
    // the b-roll's mask: a disc of radius 260 about (540, 1300), its edge soft over 12 px, in byte 1 of each word
    let mask: Vec<u32> = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            let d = ((x - 540.0).powi(2) + (y - 1300.0).powi(2)).sqrt();
            let cov = ((266.0 - d) / 12.0).clamp(0.0, 1.0);
            ((cov * 255.0).round() as u32) << 8
        })
        .collect();
    let dummy = vec![0u8; 24];
    let planes: [&[u8]; 4] = [&take.rgba, &broll.rgba, &dummy, &dummy];
    // a caption bar: translucent black under white stripes, premultiplied
    let mut overlay = vec![0u32; w * h];
    for y in 1300..1420 {
        for x in 60..1020 {
            overlay[y * w + x] = if (1330..1390).contains(&y) && x % 7 < 3 { 0xffff_ffff } else { 140 << 24 };
        }
    }
    let bg_word = 0xff20_1010u32;
    // the background texture: w x (h + 1), the last row holding a frame tag at x = 8
    let mut bg = vec![bg_word; w * (h + 1)];
    bg[h * w + 8] = 0xff38_220c;
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
        bytemuck::cast_slice(&bg),
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w as u32 * 4), rows_per_image: None },
        wgpu::Extent3d { width: w as u32, height: h as u32 + 1, depth_or_array_layers: 1 },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels");
    let comp = parse(&std::fs::read_to_string(root.join("composite.k")).map_err(|e| e.to_string())?)?;
    let sc: HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
    let c_words: Vec<u32> = slots.iter().flat_map(|s| s.words()).map(|v| v as u32).collect();
    let pw: Vec<Vec<u32>> = planes.iter().map(|p| words(p)).collect();
    let inputs = ["P0", "P1", "P2", "P3", "O", "C", "M"];
    let ck = gpu.compile_tex(&comp, &inputs, &["T"], &sc, &["px"])?;
    let bufs: Vec<&[u32]> = vec![&pw[0], &pw[1], &pw[2], &pw[3], &overlay, &c_words, &mask];
    let _ = gpu.run_tex(&ck, &bufs, &[&view], w * h)?;
    let t = Instant::now();
    let (g, times) = gpu.run_tex(&ck, &bufs, &[&view], w * h)?;
    let gpu_total = t.elapsed().as_secs_f64() * 1e3;
    let g = g.into_iter().next().unwrap();
    let t = Instant::now();
    let native = composite_reference_masked(&bg[..w * h], w, h, &planes, &slots, &overlay, Some(&mask));
    let native_ms = t.elapsed().as_secs_f64() * 1e3;
    // the CPU render's composite (docs/cpu.feature C2): equal to the reference, on every core
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let _ = composite_cpu_masked(&bg[..w * h], w, h, &planes, &slots, &overlay, cores, Some(&mask));
    let t = Instant::now();
    let fast = composite_cpu_masked(&bg[..w * h], w, h, &planes, &slots, &overlay, cores, Some(&mask));
    let cpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let fast1 = composite_cpu_masked(&bg[..w * h], w, h, &planes, &slots, &overlay, 1, Some(&mask));
    let cpu1_ms = t.elapsed().as_secs_f64() * 1e3;
    let cpu_eq_native = fast == native && fast1 == native;
    // the interpreter on every 97th pixel (it is slow): the same K, element by element
    let cpu_bufs: HashMap<&str, &[u32]> = [("P0", &pw[0][..]), ("P1", &pw[1][..]), ("P2", &pw[2][..]), ("P3", &pw[3][..]), ("O", &overlay[..]), ("C", &c_words[..]), ("M", &mask[..]), ("T", &bg[..])].into();
    let cpu = Cpu { buffers: cpu_bufs, scalars: sc.clone() };
    let all = cpu.run(&comp, w * h, &["px"])?.into_iter().next().unwrap();
    let interp_eq = all == g;
    // composite_tex.k: the same planes as R8 (luma) and RG8 (chroma) textures, as VideoToolbox's buffers wrap
    let plane_tex = |data: &[u8], tw: u32, th: u32, format: wgpu::TextureFormat, bpp: u32| {
        let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(tw * bpp), rows_per_image: None },
            wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
        );
        t.create_view(&Default::default())
    };
    let frames = [(&take, 0usize), (&broll, 1)];
    let mut ytex = Vec::new();
    let mut utex = Vec::new();
    // the interpreter holds a texture as packed words: R8 reads as (y, 0, 0, 255), RG8 as (u, v, 0, 255)
    let mut ywords: Vec<Vec<u32>> = Vec::new();
    let mut uwords: Vec<Vec<u32>> = Vec::new();
    let mut tsc = sc.clone();
    let names_y = ["Y0", "Y1", "Y2", "Y3"];
    let names_u = ["U0", "U1", "U2", "U3"];
    let widths_y = ["Y0_w", "Y1_w", "Y2_w", "Y3_w"];
    let widths_u = ["U0_w", "U1_w", "U2_w", "U3_w"];
    for s in 0..4 {
        match frames.iter().find(|f| f.1 == s) {
            Some((f, _)) => {
                let (fw, fh) = (f.width as usize, f.height as usize);
                ytex.push(plane_tex(&f.rgba[..fw * fh], fw as u32, fh as u32, wgpu::TextureFormat::R8Unorm, 1));
                utex.push(plane_tex(&f.rgba[fw * fh..], fw as u32 / 2, fh as u32 / 2, wgpu::TextureFormat::Rg8Unorm, 2));
                ywords.push(f.rgba[..fw * fh].iter().map(|&y| y as u32 | 0xff00_0000).collect());
                uwords.push(f.rgba[fw * fh..].chunks(2).map(|c| c[0] as u32 | (c[1] as u32) << 8 | 0xff00_0000).collect());
                tsc.insert(widths_y[s], fw as i32);
                tsc.insert(widths_u[s], fw as i32 / 2);
            }
            None => {
                ytex.push(plane_tex(&[0], 1, 1, wgpu::TextureFormat::R8Unorm, 1));
                utex.push(plane_tex(&[0, 0], 1, 1, wgpu::TextureFormat::Rg8Unorm, 2));
                ywords.push(vec![0xff00_0000]);
                uwords.push(vec![0xff00_0000]);
                tsc.insert(widths_y[s], 1);
                tsc.insert(widths_u[s], 1);
            }
        }
    }
    let tk = parse(&std::fs::read_to_string(root.join("composite_tex.k")).map_err(|e| e.to_string())?)?;
    let texnames = ["T", "Y0", "Y1", "Y2", "Y3", "U0", "U1", "U2", "U3"];
    let tck = gpu.compile_tex(&tk, &["O", "C", "M"], &texnames, &sc, &["px"])?;
    let views: Vec<&wgpu::TextureView> = std::iter::once(&view).chain(ytex.iter()).chain(utex.iter()).collect();
    let _ = gpu.run_tex(&tck, &[&overlay, &c_words, &mask], &views, w * h)?;
    let (gt, ttimes) = gpu.run_tex(&tck, &[&overlay, &c_words, &mask], &views, w * h)?;
    let gt = gt.into_iter().next().unwrap();
    let mut tbufs: HashMap<&str, &[u32]> = [("O", &overlay[..]), ("C", &c_words[..]), ("M", &mask[..]), ("T", &bg[..])].into();
    for s in 0..4 {
        tbufs.insert(names_y[s], &ywords[s][..]);
        tbufs.insert(names_u[s], &uwords[s][..]);
    }
    let tcpu = Cpu { buffers: tbufs, scalars: tsc.clone() };
    let tall = tcpu.run(&tk, w * h, &["px"])?.into_iter().next().unwrap();
    let tex_eq_buffers = gt == g;
    let tex_interp_eq = tall == gt;
    // then NV12 over the composite buffer (the tag from the texture)
    let nk = parse(&std::fs::read_to_string(root.join("nv12buf.k")).map_err(|e| e.to_string())?)?;
    let nck = gpu.compile_tex(&nk, &["F"], &["T"], &sc, &["px"])?;
    let n = w * h / 4 + w * h / 8 + 1;
    let (nv, _) = gpu.run_tex(&nck, &[&g], &[&view], n)?;
    let mut with_tag = g.clone();
    with_tag.extend(bg[w * h..].iter());
    let nv_ref = nv12_reference(&with_tag, w, h);
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let t = Instant::now();
    let nv_cpu = nv12_cpu(&with_tag, w, h, cores);
    let nv12_cpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let _ = nv12_reference(&with_tag, w, h);
    let nv12_ref_ms = t.elapsed().as_secs_f64() * 1e3;
    let rgba: Vec<u8> = bytemuck::cast_slice(&g).to_vec();
    image::save_buffer(root.join("../composite-test.png"), &rgba, w as u32, h as u32, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"composite": {"gpu_eq_native": g == native, "gpu_eq_interpreter": interp_eq, "gpu_compute_ms": times.compute * 1e3,
            "gpu_total_ms": gpu_total, "native_single_thread_ms": native_ms, "cpu_eq_native": cpu_eq_native, "cpu_ms": cpu_ms, "cpu_one_thread_ms": cpu1_ms, "cores": cores,
            "first_diff_native": (0..g.len()).find(|&i| g[i] != native[i])},
            "composite_tex": {"gpu_eq_composite_gpu": tex_eq_buffers, "gpu_eq_interpreter": tex_interp_eq, "gpu_compute_ms": ttimes.compute * 1e3,
                "first_diff": (0..g.len()).find(|&i| g[i] != gt[i])},
            "nv12buf": {"gpu_eq_native": nv[0] == nv_ref, "cpu_eq_native": nv_cpu == nv_ref, "cpu_ms": nv12_cpu_ms, "native_ms": nv12_ref_ms, "tag": format!("{:08x}", nv[0][n - 1])}})
    );
    Ok(())
}
