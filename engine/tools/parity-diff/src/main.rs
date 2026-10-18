//! parity-diff REF.png OURS.png [--heatmap OUT.png]
//!
//! Prints JSON: the share of pixels whose largest channel difference is within 2/255 (`within2`),
//! the maximum and mean channel difference, and the pixel count. With `--heatmap`, writes a
//! visualization: black = equal, red intensity = difference (amplified 8x).

use image::{ImageBuffer, Rgba, RgbaImage};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: parity-diff REF.png OURS.png [--heatmap OUT.png]");
        std::process::exit(2);
    }
    let load = |p: &str| -> RgbaImage {
        image::open(p).unwrap_or_else(|e| {
            eprintln!("{p}: {e}");
            std::process::exit(2)
        })
        .to_rgba8()
    };
    let a = load(&args[0]);
    let mut b = load(&args[1]);
    if a.dimensions() != b.dimensions() {
        b = image::imageops::resize(&b, a.width(), a.height(), image::imageops::FilterType::Triangle);
    }
    let heatmap = args.iter().position(|x| x == "--heatmap").and_then(|i| args.get(i + 1)).cloned();
    let (w, h) = a.dimensions();
    let mut within = 0u64;
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut heat: Option<RgbaImage> = heatmap.as_ref().map(|_| ImageBuffer::new(w, h));
    for (x, y, pa) in a.enumerate_pixels() {
        let pb = b.get_pixel(x, y);
        // compare composited over white, like the frames a viewer sees
        let over = |p: &Rgba<u8>, c: usize| -> i32 {
            let alpha = p[3] as i32;
            (p[c] as i32 * alpha + 255 * (255 - alpha)) / 255
        };
        let d = (0..3).map(|c| (over(pa, c) - over(pb, c)).unsigned_abs() as u8).max().unwrap_or(0);
        if d <= 2 {
            within += 1;
        }
        max = max.max(d);
        sum += d as u64;
        if let Some(hm) = heat.as_mut() {
            let v = (d as u32 * 8).min(255) as u8;
            hm.put_pixel(x, y, Rgba([v, 0, 0, 255]));
        }
    }
    let n = (w as u64 * h as u64).max(1);
    if let (Some(hm), Some(path)) = (heat, heatmap) {
        let _ = hm.save(&path);
    }
    println!(
        "{{\"within2\":{:.6},\"maxdiff\":{},\"meandiff\":{:.4},\"pixels\":{}}}",
        within as f64 / n as f64,
        max,
        sum as f64 / n as f64,
        n
    );
}
