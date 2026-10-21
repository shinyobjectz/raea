//! Live play looping back to frame 0 as void's stage loops a timeline: the drift after the loop against a fresh start.
//!   RAEA_AUDIO_MUTE=1 cargo run --release --example loop_drift -- TRACK.wav [frames] [fps] [loops]
use std::time::{Duration, Instant};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = std::path::PathBuf::from(&a[1]);
    let frames: i64 = a.get(2).and_then(|v| v.parse().ok()).unwrap_or(90);
    let fps: f64 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(30.0);
    let loops: usize = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(2);
    let mut m = raea_mixer::live::Live::open().unwrap();
    let tone = raea_mixer::ToneCue::default();
    let (mut base, mut since) = (0i64, Instant::now());
    let mut frame = -1i64;
    let mut seen = 0usize;
    let mut pass = 0usize;
    let show = |m: &raea_mixer::live::Live, seen: &mut usize| -> String {
        let d: Vec<String> = m.drift_ms[*seen..].iter().map(|x| format!("{x:.2}")).collect();
        *seen = m.drift_ms.len();
        d.join(" ")
    };
    while pass <= loops {
        let due = base + (since.elapsed().as_secs_f64() * fps).floor() as i64;
        while frame < due {
            frame += 1;
            let wrap = frame >= frames;
            if wrap {
                since += Duration::from_secs_f64((frame - base) as f64 / fps);
                (base, frame) = (0, 0);
                println!("pass {pass} end: drift {}", show(&m, &mut seen));
                pass += 1;
            }
            let lag = since.elapsed().as_secs_f64() - (frame - base) as f64 / fps;
            let s = raea_mixer::Sound { key: "master".into(), file: file.as_path().into(), at: frame as f64 / fps, volume: 1.0, rate: 1.0, fade_in: -1.0, fade_out: 0.0, bus: None };
            m.sync(frame, fps, &[s], &tone, lag);
            if frame == 20 {
                println!("pass {pass} to frame 20: drift {}", show(&m, &mut seen));
            }
            if wrap {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("{}", m.report());
}
