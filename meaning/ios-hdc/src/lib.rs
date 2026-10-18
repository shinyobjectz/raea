//! The iPhone side of `raea-meaning hdc-time`: the same k.rs kernels and the same timing code, compiled for iOS.
#![allow(dead_code)]
#[path = "../../src/k.rs"]
mod k;
#[path = "../../src/hdc_time.rs"]
mod hdc_time;

/// Time the layers on a frame column (`path`: little-endian f32 rows of `dim`). Returns a malloc'd JSON C string.
#[unsafe(no_mangle)]
pub extern "C" fn raea_hdc_time(path: *const std::ffi::c_char, dim: usize) -> *mut std::ffi::c_char {
    let p = unsafe { std::ffi::CStr::from_ptr(path) }.to_string_lossy().into_owned();
    let out = match std::fs::read(&p) {
        Ok(b) => {
            let rows: Vec<f32> = b.chunks_exact(4).map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]])).collect();
            hdc_time::run(dim, &rows, &p)
        }
        Err(e) => format!("{{\"error\": {:?}}}", e.to_string()),
    };
    std::ffi::CString::new(out).unwrap().into_raw()
}

#[path = "../../src/record.rs"]
mod record;

/// Encode a clip's records on the phone with the same record.rs the Mac runs (`raea-meaning encode`):
/// `vec` n x 768 SigLIP embeddings (the fused Core ML model's `vector`), `seen` n x 128 words of SEEN bits (the signs of
/// its `projection`), `yam` yam_rows x 1024 YAMNet embeddings (Core ML, one per 0.48 s), `fps` the moments' rate.
/// Writes moments.hv, windows.hv and every slot's filler (m.SLOT.hv, w.SLOT.hv) into `out_dir`; returns JSON.
#[unsafe(no_mangle)]
pub extern "C" fn raea_encode(vec: *const f32, n: usize, seen: *const u64, yam: *const f32, yam_rows: usize, fps: f64,
                              out_dir: *const std::ffi::c_char) -> *mut std::ffi::c_char {
    let dir = unsafe { std::ffi::CStr::from_ptr(out_dir) }.to_string_lossy().into_owned();
    let t0 = std::time::Instant::now();
    let mut col = k::Col::new(768);
    let v = unsafe { std::slice::from_raw_parts(vec, n * 768) };
    for r in v.chunks_exact(768) {
        col.push(r);
    }
    let words = unsafe { std::slice::from_raw_parts(seen, n * 128) };
    let seen_bits: Vec<k::Hv> = words.chunks_exact(128).map(|w| k::Hv(w.to_vec())).collect();
    let y = if yam_rows > 0 { Some(unsafe { std::slice::from_raw_parts(yam, yam_rows * 1024) }) } else { None };
    let d = k::neighbour_distance(&col);
    let cuts = k::spikes(&d, 12, 4.0, 0.05, (0.25 * fps).round().max(1.0) as usize);
    let space = record::Space::new(8192);
    let e = space.encode(&record::Sources { siglip: &col, fps, yamnet: y, logmel: None, cuts: &cuts, tracks: vec![], seen_bits: Some(seen_bits) }, true);
    let mut out = vec![];
    let mut w = |name: String, rows: &[k::Hv]| {
        let _ = record::write_hv(&format!("{dir}/{name}"), rows);
        out.push(name);
    };
    w("moments.hv".into(), &e.moments);
    w("windows.hv".into(), &e.windows);
    for (s, rows) in &e.moment_fillers {
        w(format!("m.{s}.hv"), rows);
    }
    for (s, rows) in &e.window_fillers {
        w(format!("w.{s}.hv"), rows);
    }
    let json = format!("{{\"moments\": {}, \"windows\": {}, \"cuts\": {}, \"files\": {:?}, \"encode_ms\": {:.1}}}",
                       e.moments.len(), e.windows.len(), cuts.len(), out, t0.elapsed().as_secs_f64() * 1000.0);
    std::ffi::CString::new(json).unwrap().into_raw()
}
