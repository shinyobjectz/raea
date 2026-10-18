//! void's playtest probe: the no-V8 goal's gates measured inside void's own window, not in `raea-motion play`'s
//! offscreen harness. Off unless RAEA_VOID_PROBE=1; then a small HTTP server on 127.0.0.1:RAEA_VOID_PROBE_PORT (7458)
//! answers the harness (void/bench/gates.py), and three hooks feed it:
//!   - the stage thread (stage_native.rs): `drained` after it takes its commands, `stepped` for each frame stepped (the
//!     keys it carried: a game's input log), `painted` after each frame painted into the chrome's texture (the work: step
//!     and paint);
//!   - the chrome's renderer (vendor/dioxus-native ON_RENDER): each window frame's render, which ends at the present.
//! Input to present: a key change pressed through stage_native::key by this probe's own input thread (as play's --input
//! thread presses them), stamped there; it is timed to the end of the first window frame rendered after the stage
//! painted the frame that carried it. The path from AppKit to stage_native::key is not in it (a test window doesn't take
//! the keyboard).
//!
//! GET /reset                      clear the measurements
//! GET /report                     the measurements since the last reset, as JSON
//! GET /input?keys=A,B+C,&seconds=S  random key changes 50-250 ms apart for S seconds (sets split by commas, keys by +)
//! GET /snapshot?path=P[&frame=N]  the stage's page read back to P (with frame: paused and sought there first)
//! GET /inputlog?path=P            the keys each frame carried since the object opened, as `render --input` takes them

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_VOID_PROBE").as_deref() == Ok("1"))
}

#[derive(Default)]
struct M {
    t0: Option<Instant>,
    fps: f64,
    /// stage: work (step + paint, ms), gap between paints (ms), frames stepped per paint
    work: Vec<f64>,
    gaps: Vec<f64>,
    stepped: u64,
    last_paint: Option<Instant>,
    /// window: each render's time (ms) and the gap between renders' ends (ms)
    render: Vec<f64>,
    rgaps: Vec<f64>,
    last_render: Option<Instant>,
    /// input: a change sent (its stamp), taken by the stage, painted (stamp, paint time), presented
    sent: Option<Instant>,
    applied: Option<Instant>,
    painted: Option<(Instant, Instant)>,
    latency: Vec<f64>,
    /// input to the stage's paint of the frame that carries it (the window's present not in it)
    to_paint: Vec<f64>,
    changes: u64,
    /// each second: (s, rss MB, footprint MB)
    mem: Vec<(f64, f64, f64)>,
    /// the keys each frame carried
    log: BTreeMap<i64, Vec<String>>,
}

static S: Mutex<Option<M>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut M) -> T) -> Option<T> {
    if !on() {
        return None;
    }
    let mut g = S.lock().ok()?;
    Some(f(g.get_or_insert_with(M::default)))
}

/// The stage took its commands: a key change sent before now is in the next frame stepped.
pub fn drained() {
    with(|m| {
        if let Some(t) = m.sent.take() {
            m.applied = Some(t);
        }
    });
}

/// A frame stepped with these keys (a frame back at 0 starts the log again: the object opened, a timeline looped).
pub fn stepped(frame: i64, keys: &[String]) {
    with(|m| {
        if frame == 0 {
            m.log.clear();
        }
        m.log.insert(frame, keys.to_vec());
        m.stepped += 1;
    });
}

/// A frame painted into the chrome's texture: the work since `t0` (the loop's steps and the paint), at the
/// composition's rate `fps`.
pub fn painted(t0: Instant, fps: f64) {
    let now = Instant::now();
    with(|m| {
        m.t0.get_or_insert(now);
        m.fps = fps;
        m.work.push((now - t0).as_secs_f64() * 1e3);
        if let Some(l) = m.last_paint {
            m.gaps.push((now - l).as_secs_f64() * 1e3);
        }
        m.last_paint = Some(now);
        if let Some(t) = m.applied.take() {
            m.to_paint.push((now - t).as_secs_f64() * 1e3);
            m.painted = Some((t, now));
        }
    });
}

/// A window frame rendered from `t0` to its present's end (the chrome's renderer).
pub fn rendered(t0: Instant, t1: Instant) {
    with(|m| {
        m.render.push((t1 - t0).as_secs_f64() * 1e3);
        if let Some(l) = m.last_render {
            m.rgaps.push((t1 - l).as_secs_f64() * 1e3);
        }
        m.last_render = Some(t1);
        // the first window frame begun after the stage painted the change carries it
        if let Some((sent, at)) = m.painted {
            if t0 >= at {
                m.latency.push((t1 - sent).as_secs_f64() * 1e3);
                m.painted = None;
            }
        }
    });
}

/// Starts the probe's server and its memory sampler, when RAEA_VOID_PROBE=1.
pub fn start() {
    if !on() {
        return;
    }
    let _ = dioxus_native::ON_RENDER.set(rendered);
    std::thread::Builder::new()
        .name("void probe mem".into())
        .spawn(|| {
            let t = Instant::now();
            loop {
                let (rss, fp) = (raea_native::trace::rss_mb(), raea_native::trace::footprint_mb());
                with(|m| m.mem.push((t.elapsed().as_secs_f64(), rss, fp)));
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .ok();
    let port: u16 = std::env::var("RAEA_VOID_PROBE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7458);
    std::thread::Builder::new()
        .name("void probe".into())
        .spawn(move || {
            let server = match tiny_http::Server::http(("127.0.0.1", port)) {
                Ok(s) => s,
                Err(e) => return crate::mcp::log(format!("probe: can't listen on {port}: {e}")),
            };
            crate::mcp::log(format!("probe: http://127.0.0.1:{port}"));
            for req in server.incoming_requests() {
                std::thread::spawn(move || {
                    let url = req.url().to_string();
                    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
                    let q: BTreeMap<String, String> = query.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), unescape(v))).collect();
                    let body = match answer(path, &q) {
                        Ok(v) => v,
                        Err(e) => serde_json::json!({"error": e}),
                    };
                    let _ = req.respond(tiny_http::Response::from_string(body.to_string()));
                });
            }
        })
        .ok();
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("3f"), 16).unwrap_or(b'?'));
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn display_hz() -> f64 {
    unsafe {
        use objc2::runtime::AnyObject;
        let screen: *mut AnyObject = objc2::msg_send![objc2::class!(NSScreen), mainScreen];
        if screen.is_null() {
            return 60.0;
        }
        let n: isize = objc2::msg_send![&*screen, maximumFramesPerSecond];
        if n > 0 { n as f64 } else { 60.0 }
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    (s[((s.len() - 1) as f64 * p).round() as usize] * 1000.0).round() / 1000.0
}

fn answer(path: &str, q: &BTreeMap<String, String>) -> Result<serde_json::Value, String> {
    use serde_json::json;
    match path {
        "/reset" => {
            with(|m| {
                let log = std::mem::take(&mut m.log);
                *m = M { log, ..M::default() };
            });
            Ok(json!({"ok": true}))
        }
        "/report" => {
            let hz = display_hz();
            let st = crate::stage_native::status();
            Ok(with(|m| {
                let secs = m.t0.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
                // a frame is due every refresh, or every composition frame when the composition is slower
                let budget = 1000.0 / if m.fps > 0.0 { hz.min(m.fps) } else { hz };
                let lat_frames = if m.fps > 0.0 { pct(&m.latency, 0.99) / (1000.0 / m.fps) } else { 0.0 };
                json!({
                    "seconds": (secs * 10.0).round() / 10.0,
                    "display_hz": hz,
                    "composition_fps": m.fps,
                    "budget_ms": budget,
                    "page": [st.page.0, st.page.1],
                    "stage_frame": st.frame,
                    "stage_error": st.error,
                    "stage": {"painted": m.work.len(), "stepped": m.stepped, "painted_fps": if secs > 0.0 { m.work.len() as f64 / secs } else { 0.0 },
                        "work_ms": {"p50": pct(&m.work, 0.5), "p99": pct(&m.work, 0.99), "max": pct(&m.work, 1.0)},
                        "gap_ms": {"p50": pct(&m.gaps, 0.5), "p99": pct(&m.gaps, 0.99), "max": pct(&m.gaps, 1.0)},
                        "missed": m.gaps.iter().filter(|g| **g > budget * 1.5).count()},
                    "window": {"rendered": m.render.len(), "rendered_fps": if secs > 0.0 { m.render.len() as f64 / secs } else { 0.0 },
                        "render_ms": {"p50": pct(&m.render, 0.5), "p99": pct(&m.render, 0.99), "max": pct(&m.render, 1.0)},
                        "gap_ms": {"p50": pct(&m.rgaps, 0.5), "p99": pct(&m.rgaps, 0.99), "max": pct(&m.rgaps, 1.0)},
                        "missed": m.rgaps.iter().filter(|g| **g > budget * 1.5).count()},
                    "input_to_present_ms": {"sent": m.changes, "timed": m.latency.len(), "p50": pct(&m.latency, 0.5), "p99": pct(&m.latency, 0.99), "max": pct(&m.latency, 1.0), "p99_frames": lat_frames,
                        "to_paint_p50": pct(&m.to_paint, 0.5), "to_paint_p99": pct(&m.to_paint, 0.99), "to_paint_max": pct(&m.to_paint, 1.0), "to_paint_timed": m.to_paint.len()},
                    "mem_each_second": m.mem.iter().map(|(t, r, f)| json!([(t * 10.0).round() / 10.0, (r * 10.0).round() / 10.0, (f * 10.0).round() / 10.0])).collect::<Vec<_>>(),
                })
            })
            .unwrap_or_default())
        }
        "/input" => {
            let sets: Vec<Vec<String>> = q.get("keys").map(|v| v.split(',').map(|s| s.split('+').filter(|k| !k.is_empty()).map(str::to_string).collect()).collect()).unwrap_or_else(|| vec![vec![], vec!["ArrowUp".into()], vec!["ArrowDown".into()]]);
            let secs: f64 = q.get("seconds").and_then(|s| s.parse().ok()).unwrap_or(10.0);
            std::thread::spawn(move || {
                let until = Instant::now() + Duration::from_secs_f64(secs);
                let (mut x, mut k, mut down): (u64, usize, Vec<String>) = (0x2545f4914f6cdd1d, 0, Vec::new());
                while Instant::now() < until {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    std::thread::sleep(Duration::from_millis(50 + x % 200));
                    k += 1;
                    let set = &sets[k % sets.len()];
                    // stamped before the keys go, published after: a stage that takes them before it sees the stamp
                    // counts them a frame late (never early)
                    let at = Instant::now();
                    for key in down.iter().filter(|d| !set.contains(d)) {
                        crate::stage_native::key(key, false);
                    }
                    for key in set.iter().filter(|s| !down.contains(s)) {
                        crate::stage_native::key(key, true);
                    }
                    with(|m| {
                        m.sent = Some(at);
                        m.changes += 1;
                    });
                    down = set.clone();
                }
                for key in &down {
                    crate::stage_native::key(key, false);
                }
            });
            Ok(json!({"ok": true}))
        }
        "/snapshot" => {
            let p = q.get("path").ok_or("path is required")?;
            let frame = q.get("frame").and_then(|f| f.parse::<i64>().ok());
            if let Some(f) = frame {
                crate::stage_native::control(crate::stage_native::Control::Pause);
                crate::stage_native::control(crate::stage_native::Control::Seek(f));
            }
            let (f, w, h) = crate::stage_native::snapshot(std::path::Path::new(p))?;
            Ok(json!({"frame": f, "w": w, "h": h, "path": p}))
        }
        "/inputlog" => {
            let p = q.get("path").ok_or("path is required")?;
            let log = with(|m| m.log.iter().map(|(f, k)| (f.to_string(), json!({"keys": k}))).collect::<serde_json::Map<_, _>>()).unwrap_or_default();
            let n = log.len();
            std::fs::write(p, serde_json::Value::Object(log).to_string()).map_err(|e| e.to_string())?;
            Ok(json!({"frames": n, "path": p}))
        }
        other => Err(format!("no {other}")),
    }
}
