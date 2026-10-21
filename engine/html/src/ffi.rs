//! The C ABI (header: include/raea.h) over api.rs, for Swift / Objective-C hosts.
//!
//! Conventions: paths and JSON are NUL-terminated UTF-8. Returned `char *` strings are owned by
//! the caller and freed with `raea_free_string`. A failing call returns NULL / a negative status
//! and leaves a message for `raea_last_error()` on the calling thread. No call panics across the
//! boundary: panics are caught and reported as errors.
//!
//! Threading: every call may be made from any thread, the main thread included (they block; run
//! long ones off the main thread). Compositions never cross threads: probe / lint / snapshot run
//! on an internal 64 MB-stack thread, and a render handle owns `workers` threads, each with its
//! own document and JS runtime. A handle is internally locked, but its calls should come from one
//! thread at a time, and `raea_render_end` must not race the others on the same handle.

use crate::api::{self, FrameError, RenderOptions, RenderSession};
use crate::media::FrameProvider;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

pub const RAEA_OK: i32 = 0;
pub const RAEA_DONE: i32 = -1;
pub const RAEA_ERR: i32 = -2;
pub const RAEA_ERR_BUFFER_TOO_SMALL: i32 = -3;
pub const RAEA_ERR_INVALID_ARGUMENT: i32 = -4;
pub const RAEA_ERR_STATE: i32 = -5;

pub const RAEA_PIXEL_RGBA8: u32 = 0;
pub const RAEA_PIXEL_BGRA8: u32 = 1;

pub const RAEA_FRAME_OK: i32 = 0;
pub const RAEA_FRAME_NEED_BUFFER: i32 = 1;
pub const RAEA_FRAME_NONE: i32 = -1;

/// One decoded `<video>` frame the host writes for the frame provider.
#[repr(C)]
pub struct RaeaVideoFrame {
    /// [in] buffer to write into (NULL when `capacity` is 0)
    pub data: *mut u8,
    /// [in] bytes available at `data`
    pub capacity: usize,
    /// [out] frame size in pixels
    pub width: u32,
    pub height: u32,
    /// [out] bytes between rows (0 = width * 4)
    pub row_bytes: usize,
    /// [out] RAEA_PIXEL_RGBA8 or RAEA_PIXEL_BGRA8
    pub pixel_format: u32,
}

pub type RaeaVideoFrameFn = unsafe extern "C" fn(
    user_data: *mut c_void,
    video_id: *const c_char,
    frame_index: u64,
    media_time: f64,
    frame: *mut RaeaVideoFrame,
) -> i32;

/// Opaque render handle.
pub struct RaeaRender {
    session: Mutex<RenderSession>,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(msg: String) {
    let c = CString::new(msg.replace('\0', " ")).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

/// Run `f`, turning errors and panics into `fail` + the thread's last error.
fn guard<T>(fail: T, f: impl FnOnce() -> Result<T, String>) -> T {
    clear_error();
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            set_error(e);
            fail
        }
        Err(p) => {
            set_error(api::panic_message(p));
            fail
        }
    }
}

unsafe fn c_str<'a>(p: *const c_char, what: &str) -> Result<Option<&'a str>, String> {
    if p.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(p) }.to_str().map(Some).map_err(|_| format!("{what} is not valid UTF-8"))
}

unsafe fn c_path(p: *const c_char) -> Result<PathBuf, String> {
    match unsafe { c_str(p, "path") }? {
        Some(s) if !s.is_empty() => Ok(PathBuf::from(s)),
        _ => Err("path is NULL or empty".into()),
    }
}

unsafe fn c_opts(p: *const c_char) -> Result<RenderOptions, String> {
    RenderOptions::from_json(unsafe { c_str(p, "opts_json") }?)
}

fn into_c_string(s: String) -> *mut c_char {
    CString::new(s.replace('\0', " ")).map(CString::into_raw).unwrap_or(null_mut())
}

/// Library version, e.g. "0.1.0". Static; do not free.
#[unsafe(no_mangle)]
pub extern "C" fn raea_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// The last error on this thread, or NULL. Valid until the next raea_* call on this thread.
#[unsafe(no_mangle)]
pub extern "C" fn raea_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()))
}

/// Free a string returned by raea_probe / raea_lint / raea_render_info. NULL is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// `raea probe` as JSON. NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_probe(path: *const c_char, opts_json: *const c_char) -> *mut c_char {
    guard(null_mut(), || {
        let (path, o) = unsafe { (c_path(path)?, c_opts(opts_json)?) };
        let v = api::probe_path(&path, &o)?;
        Ok(into_c_string(v.to_string()))
    })
}

/// `raea lint --json` as JSON (`{ok, errors, warnings, unsupported}`). NULL when the
/// composition cannot be loaded at all.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_lint(path: *const c_char, opts_json: *const c_char) -> *mut c_char {
    guard(null_mut(), || {
        let (path, o) = unsafe { (c_path(path)?, c_opts(opts_json)?) };
        let v = api::lint_path(&path, &o)?;
        Ok(into_c_string(v.to_string()))
    })
}

/// Render time `t` (seconds) into `out` (packed rows, width * height * 4 bytes, sizes from
/// raea_probe). RAEA_OK, RAEA_ERR_BUFFER_TOO_SMALL, or RAEA_ERR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_snapshot(path: *const c_char, t: f64, opts_json: *const c_char, out: *mut u8, out_len: usize) -> i32 {
    guard(RAEA_ERR, || {
        let (path, o) = unsafe { (c_path(path)?, c_opts(opts_json)?) };
        if out.is_null() {
            return Ok(RAEA_ERR_INVALID_ARGUMENT);
        }
        let (w, h, px) = api::snapshot_path(&path, t, &o)?;
        if out_len < px.len() {
            set_error(format!("buffer holds {out_len} bytes; a {w}x{h} frame needs {}", px.len()));
            return Ok(RAEA_ERR_BUFFER_TOO_SMALL);
        }
        unsafe { std::ptr::copy_nonoverlapping(px.as_ptr(), out, px.len()) };
        Ok(RAEA_OK)
    })
}

/// Load a composition for rendering (probe runs here; workers start on the first frame).
/// NULL on error. Free with raea_render_end.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_begin(path: *const c_char, opts_json: *const c_char) -> *mut RaeaRender {
    guard(null_mut(), || {
        let (path, o) = unsafe { (c_path(path)?, c_opts(opts_json)?) };
        let session = RenderSession::begin(&path, o)?;
        Ok(Box::into_raw(Box::new(RaeaRender { session: Mutex::new(session) })))
    })
}

fn lock(r: &RaeaRender) -> std::sync::MutexGuard<'_, RenderSession> {
    r.session.lock().unwrap_or_else(|p| p.into_inner())
}

/// Probe JSON for the handle plus firstFrame, endFrame, frameCount, workers, bytesPerFrame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_info(r: *const RaeaRender) -> *mut c_char {
    guard(null_mut(), || {
        let r = unsafe { r.as_ref() }.ok_or("handle is NULL")?;
        Ok(into_c_string(lock(r).info().to_string()))
    })
}

struct HostCallback {
    cb: RaeaVideoFrameFn,
    user_data: *mut c_void,
}
// The host promises the callback may be called from any thread, concurrently.
unsafe impl Send for HostCallback {}
unsafe impl Sync for HostCallback {}

impl HostCallback {
    // a method, so closures capture the whole (Send + Sync) struct rather than its raw pointer
    unsafe fn call(&self, id: &CStr, k: u64, media_time: f64, f: &mut RaeaVideoFrame) -> i32 {
        unsafe { (self.cb)(self.user_data, id.as_ptr(), k, media_time, f) }
    }
}

/// Packed RGBA8 from a host frame (any row stride, RGBA or BGRA).
fn pack(mut buf: Vec<u8>, w: u32, h: u32, row: usize, format: u32) -> Vec<u8> {
    let packed = w as usize * 4;
    if row != packed {
        let mut out = Vec::with_capacity(packed * h as usize);
        for y in 0..h as usize {
            out.extend_from_slice(&buf[y * row..y * row + packed]);
        }
        buf = out;
    }
    if format == RAEA_PIXEL_BGRA8 {
        api::swap_red_blue(&mut buf);
    }
    buf
}

/// Supply `<video>` frames from the host (e.g. AVAssetReader) instead of `videoFrames`.
/// Must be called before the first raea_render_next_frame. `cb` NULL removes the provider.
/// RAEA_OK, RAEA_ERR_STATE (rendering already started) or RAEA_ERR_INVALID_ARGUMENT.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_set_video_frame_provider(r: *mut RaeaRender, cb: Option<RaeaVideoFrameFn>, user_data: *mut c_void) -> i32 {
    guard(RAEA_ERR, || {
        let Some(r) = (unsafe { r.as_ref() }) else {
            set_error("handle is NULL".into());
            return Ok(RAEA_ERR_INVALID_ARGUMENT);
        };
        let mut session = lock(r);
        let provider = cb.map(|cb| {
            let host = HostCallback { cb, user_data };
            let fps = session.fps();
            let clips: HashMap<String, (f64, f64)> = session.videos().iter().map(|v| (v.id.clone(), (v.media_start, v.playback_rate))).collect();
            // bytes the host asked for last time, per video (the first frame may need a retry)
            let sizes: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());
            FrameProvider(Arc::new(move |id: &str, k: usize| {
                let cid = CString::new(id).ok()?;
                let (media_start, rate) = clips.get(id).copied().unwrap_or((0.0, 1.0));
                let media_time = media_start + k as f64 / fps * rate;
                let mut cap = sizes.lock().ok()?.get(id).copied().unwrap_or(0);
                for _ in 0..3 {
                    let mut buf = vec![0u8; cap];
                    let mut f = RaeaVideoFrame {
                        data: if cap > 0 { buf.as_mut_ptr() } else { null_mut() },
                        capacity: cap,
                        width: 0,
                        height: 0,
                        row_bytes: 0,
                        pixel_format: RAEA_PIXEL_RGBA8,
                    };
                    let status = unsafe { host.call(&cid, k as u64, media_time, &mut f) };
                    let (w, h) = (f.width as usize, f.height as usize);
                    let row = if f.row_bytes == 0 { w * 4 } else { f.row_bytes };
                    let need = row * h;
                    if w == 0 || h == 0 || row < w * 4 {
                        return None;
                    }
                    match status {
                        RAEA_FRAME_OK if need <= cap => {
                            buf.truncate(need);
                            return Some((f.width, f.height, pack(buf, f.width, f.height, row, f.pixel_format)));
                        }
                        RAEA_FRAME_NEED_BUFFER | RAEA_FRAME_OK if need > cap => {
                            cap = need;
                            sizes.lock().ok()?.insert(id.to_string(), need);
                        }
                        _ => return None,
                    }
                }
                None
            }))
        });
        match session.set_provider(provider) {
            Ok(()) => Ok(RAEA_OK),
            Err(e) => {
                set_error(e);
                Ok(RAEA_ERR_STATE)
            }
        }
    })
}

fn next_frame(r: *mut RaeaRender, buf: *mut u8, buf_len: usize, row_bytes: usize) -> i64 {
    guard(RAEA_ERR as i64, || {
        let Some(r) = (unsafe { r.as_ref() }) else {
            set_error("handle is NULL".into());
            return Ok(RAEA_ERR_INVALID_ARGUMENT as i64);
        };
        let mut session = lock(r);
        if buf.is_null() {
            set_error("buffer is NULL".into());
            return Ok(RAEA_ERR_INVALID_ARGUMENT as i64);
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(buf, buf_len) };
        match session.next_frame(dst, row_bytes) {
            Ok(Some(i)) => Ok(i as i64),
            Ok(None) => Ok(RAEA_DONE as i64),
            Err(FrameError::BufferTooSmall(n)) => {
                set_error(format!("buffer holds {buf_len} bytes; this frame needs {n}"));
                Ok(RAEA_ERR_BUFFER_TOO_SMALL as i64)
            }
            Err(FrameError::Failed(e)) => Err(e),
        }
    })
}

/// Next frame, packed (width * 4 bytes per row) into `rgba` (`bytesPerFrame` bytes). Returns the
/// frame index (>= 0), RAEA_DONE after the last frame, or a negative error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_next_frame(r: *mut RaeaRender, rgba: *mut u8, buf_len: usize) -> i64 {
    next_frame(r, rgba, buf_len, 0)
}

/// Like raea_render_next_frame, rows `row_bytes` apart (a locked CVPixelBuffer's base address and
/// bytesPerRow). Needs row_bytes * (height - 1) + width * 4 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_next_frame_strided(r: *mut RaeaRender, dst: *mut u8, buf_len: usize, row_bytes: usize) -> i64 {
    next_frame(r, dst, buf_len, row_bytes)
}

/// Render the frame at `t` seconds, in any order (scrubbing, previews), into `dst` with rows
/// `row_bytes` apart (0 = packed). A separate worker keeps the composition loaded, so after the
/// first call (which loads it) a frame costs one seek + paint. Blocks; call it off the main
/// thread and coalesce playhead changes (render only the latest requested time). Independent of
/// raea_render_next_frame on the same handle. Set a frame provider before the first call.
/// RAEA_OK, RAEA_ERR_BUFFER_TOO_SMALL, RAEA_ERR_INVALID_ARGUMENT or RAEA_ERR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_frame_at(r: *mut RaeaRender, t: f64, dst: *mut u8, buf_len: usize, row_bytes: usize) -> i32 {
    guard(RAEA_ERR, || {
        let Some(r) = (unsafe { r.as_ref() }) else {
            set_error("handle is NULL".into());
            return Ok(RAEA_ERR_INVALID_ARGUMENT);
        };
        if dst.is_null() || !t.is_finite() {
            set_error("buffer is NULL or t is not finite".into());
            return Ok(RAEA_ERR_INVALID_ARGUMENT);
        }
        let mut session = lock(r);
        let buf = unsafe { std::slice::from_raw_parts_mut(dst, buf_len) };
        match session.frame_at(t, buf, row_bytes) {
            Ok(()) => Ok(RAEA_OK),
            Err(FrameError::BufferTooSmall(n)) => {
                set_error(format!("buffer holds {buf_len} bytes; this frame needs {n}"));
                Ok(RAEA_ERR_BUFFER_TOO_SMALL)
            }
            Err(FrameError::Failed(e)) => Err(e),
        }
    })
}

/// The text-layout report at `t` seconds as JSON (`raea snapshot --boxes`): every visible
/// text element's box in composition px (transforms included), the canvas edges it crosses, overflow
/// clipping, opacity, the safe-zone bands it reaches and the text boxes it overlaps. `safe_zone`:
/// "top,right,bottom,left" in composition px, or NULL for 220,140,420,0 at 1080x1920 and none
/// otherwise. Renders `t` on the raea_render_frame_at worker (loading it on first use); blocks. Free
/// with raea_free_string; NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_boxes_at(r: *mut RaeaRender, t: f64, safe_zone: *const c_char) -> *mut c_char {
    guard(null_mut(), || {
        let Some(r) = (unsafe { r.as_ref() }) else {
            return Err("handle is NULL".into());
        };
        if !t.is_finite() {
            return Err("t is not finite".into());
        }
        let safe = match unsafe { c_str(safe_zone, "safe_zone") }? {
            None => None,
            Some(z) => {
                let v: Vec<f64> = z.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("bad safe_zone {z:?} (top,right,bottom,left)"))?;
                let [top, right, bottom, left] = v[..] else { return Err(format!("safe_zone {z:?} needs top,right,bottom,left")) };
                Some([top, right, bottom, left])
            }
        };
        let json = lock(r).boxes_at(t, safe)?;
        Ok(into_c_string(json))
    })
}

/// Stop the workers (each finishes the frame it is on) and free the handle. NULL is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_render_end(r: *mut RaeaRender) {
    if r.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(r) })));
}

// === html nodes (node.rs): one page as a layer for an embedding engine ===

/// Opaque html node handle. Used by one thread (the one that opened it).
pub struct RaeaNode {
    node: crate::node::HtmlNode,
}

/// Open an html node: `html` is the page markup, `base_url` a file:// URL (or empty/NULL) that relative
/// URLs resolve against, `opts_json` see node.rs `NodeOptions`. Every resource loads here; the network
/// is sealed afterwards. NULL on error (raea_last_error). Free with raea_node_close.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_node_open(html: *const c_char, base_url: *const c_char, opts_json: *const c_char) -> *mut RaeaNode {
    guard(null_mut(), || {
        let html = unsafe { c_str(html, "html") }?.ok_or("html is NULL")?;
        let base = unsafe { c_str(base_url, "base_url") }?.unwrap_or("");
        let opts = crate::node::NodeOptions::from_json(unsafe { c_str(opts_json, "opts_json") }?)?;
        let node = crate::node::HtmlNode::open(html, base, opts)?;
        Ok(Box::into_raw(Box::new(RaeaNode { node })))
    })
}

/// Render the node at `t` seconds into `rgba`: premultiplied RGBA8, width*scale x height*scale,
/// `stride` bytes per row (>= width*scale*4; 0 means tightly packed), `buf_len` bytes available.
/// The same bytes for any order of t. RAEA_OK, or RAEA_ERR_BUFFER_TOO_SMALL / RAEA_ERR_INVALID_ARGUMENT / RAEA_ERR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_node_render(n: *mut RaeaNode, t: f64, rgba: *mut u8, buf_len: usize, stride: usize) -> i32 {
    guard(-2, || {
        let n = unsafe { n.as_mut() }.ok_or("node is NULL")?;
        if rgba.is_null() || !t.is_finite() {
            set_error("rgba is NULL or t is not finite".into());
            return Ok(-4);
        }
        let (w, h) = n.node.pixel_size();
        let stride = if stride == 0 { w as usize * 4 } else { stride };
        let need = stride * (h as usize).saturating_sub(1) + w as usize * 4;
        if stride < w as usize * 4 || buf_len < need {
            set_error(format!("buffer too small: {w}x{h} at stride {stride} needs {need} bytes, got {buf_len}"));
            return Ok(-3);
        }
        let dst = unsafe { std::slice::from_raw_parts_mut(rgba, buf_len) };
        n.node.render_into(t, dst, stride)?;
        Ok(0)
    })
}

/// Text boxes at `t` as JSON (snapshot --boxes). Free with raea_free_string; NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_node_boxes(n: *mut RaeaNode, t: f64) -> *mut c_char {
    guard(null_mut(), || {
        let n = unsafe { n.as_mut() }.ok_or("node is NULL")?;
        Ok(into_c_string(n.node.boxes(t).to_string()))
    })
}

/// The node's facts as JSON: width, height, scale, duration (0 if static), profile, fonts, fetched
/// (url, bytes), failed, lateFetches, warnings, jsErrors, inputsHash. Free with raea_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_node_info(n: *mut RaeaNode) -> *mut c_char {
    guard(null_mut(), || {
        let n = unsafe { n.as_mut() }.ok_or("node is NULL")?;
        Ok(into_c_string(n.node.info().to_string()))
    })
}

/// Free a node. NULL is ignored. Call from the thread that opened it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_node_close(n: *mut RaeaNode) {
    if !n.is_null() {
        drop(unsafe { Box::from_raw(n) });
    }
}
