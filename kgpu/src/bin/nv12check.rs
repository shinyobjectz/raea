//! In-engine NV12 against the native reference: RGBA.raw (frames read back as RGBA) and NV12.raw (the same frames
//! through the engine's GPU NV12 pass), W H, frame count. Every NV12 frame must equal nv12_reference(RGBA frame).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (rgba, nv12) = (std::fs::read(&a[1]).unwrap(), std::fs::read(&a[2]).unwrap());
    let (w, h): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    let (fr, fn12) = (w * h * 4, w * h * 3 / 2);
    let n = rgba.len() / fr;
    assert_eq!(nv12.len() / fn12, n, "frame counts differ");
    let mut same = 0;
    for k in 0..n {
        let mut words: Vec<u32> = bytemuck::cast_slice(&rgba[k * fr..(k + 1) * fr]).to_vec();
        words.extend(std::iter::repeat_n(0, w)); // the tag row (its texel is checked by the engine, not here)
        let want: Vec<u8> = bytemuck::cast_slice(&raea_kgpu::nv12_reference(&words, w, h)).to_vec();
        if want[..fn12] == nv12[k * fn12..(k + 1) * fn12] {
            same += 1;
        } else {
            let d = (0..fn12).find(|&i| want[i] != nv12[k * fn12 + i]).unwrap();
            println!("frame {k}: first difference at byte {d}");
        }
    }
    println!("{same}/{n} frames: engine NV12 == native reference");
}
