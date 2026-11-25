use super::*;

/// A 64x64 VP9 WebM with alpha, a red square on nothing (#113), made by this machine's ffmpeg; None without ffmpeg or
/// libvpx (the test then has nothing to read).
fn alpha_webm(dir: &Path) -> Option<PathBuf> {
    let p = dir.join("square.webm");
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=red@0.0:s=64x64:d=0.4,format=rgba,geq=r=255:g=0:b=0:a='if(lt(abs(X-32),16)*lt(abs(Y-32),16),255,0)'"])
        .args(["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-auto-alt-ref", "0"])
        .arg(&p)
        .status()
        .is_ok_and(|s| s.success());
    made.then_some(p)
}

#[test]
fn vp9_webm_keeps_its_alpha() {
    let dir = std::env::temp_dir().join(format!("raea-ffmpeg-alpha-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let Some(webm) = alpha_webm(&dir) else { return eprintln!("skipped: no ffmpeg with libvpx-vp9 here") };
    // as made, and copied by ffmpeg's muxer, which writes the tag as ALPHA_MODE (#224)
    let copy = dir.join("copy.webm");
    let copied = Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&webm).args(["-c", "copy"]).arg(&copy).status().is_ok_and(|s| s.success());
    assert!(copied, "ffmpeg copies the WebM");
    for file in [&webm, &copy] {
        let mut d = Ffmpeg.open_video(file).expect("the ffmpeg backend opens a VP9 WebM");
        for t in [0.0, 0.2] {
            let f = d.frame_at(t).expect("a frame");
            let at = |x: usize, y: usize| f.rgba[(y * f.width as usize + x) * 4 + 3];
            assert_eq!((at(2, 2), at(32, 32)), (0, 255), "{} at {t} s: outside the square is transparent, inside opaque", file.display());
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
