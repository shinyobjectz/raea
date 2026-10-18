//! The probe app's entry: `raea_motion_run(bundle)` runs a program bundled by `raea-motion bundle` on JavaScriptCore,
//! the system's engine (jitless: iOS gives an app no JIT, and the app sets JSC_useJIT=false besides), with
//! @raea/motion's native calls, and answers `{"code": N, "printed": "...", "ms": T}` (free it with raea_motion_free).
//! No JS engine is linked into the app: `nm` on the built binary finds no v8 symbol (react/ios/build.sh checks).

use std::ffi::{CStr, CString, c_char};

/// # Safety
/// `bundle` is a NUL-terminated path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_motion_run(bundle: *const c_char) -> *mut c_char {
    let path = unsafe { CStr::from_ptr(bundle).to_string_lossy().to_string() };
    raea_react::native::register();
    // the sandbox's WASI tools when the app carries them (Bundle/wsx/*.cwasm, compiled for Pulley on the Mac)
    let wsx_dir = std::path::Path::new(&path).parent().map(|d| d.join("wsx"));
    if let Some(d) = wsx_dir.filter(|d| d.is_dir()) {
        static WSX: std::sync::Once = std::sync::Once::new();
        WSX.call_once(|| match wsx::Sandbox::new(&d) {
            Ok(sb) => wsx::register(sb, std::env::temp_dir()),
            Err(e) => eprintln!("wsx: {e}"),
        });
    }
    raea_react::native::ECHO.store(false, std::sync::atomic::Ordering::Relaxed);
    let t = std::time::Instant::now();
    let v = match raea_react::motion::run_bundle(std::path::Path::new(&path), 900.0) {
        Ok((code, printed)) => serde_json::json!({"code": code, "printed": printed, "ms": t.elapsed().as_secs_f64() * 1e3}),
        Err(e) => serde_json::json!({"code": -1, "error": e}),
    };
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}

/// # Safety
/// `p` came from raea_motion_run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_motion_free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

/// A composition bundled by `raea-motion bundle EDIT.tsx --composition-bundle --portable DIR` rendered by the engine
/// here (React in Bevy, Blitz, native decode, rdb's render) into `out` (.mp4); media resolves in `dir`.
/// Answers `{"code": 0, "said": "...", "ms": T}` or `{"code": -1, "error": "..."}`.
///
/// # Safety
/// The three are NUL-terminated paths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_motion_render(bundle: *const c_char, dir: *const c_char, out: *const c_char) -> *mut c_char {
    let s = |p: *const c_char| unsafe { CStr::from_ptr(p).to_string_lossy().to_string() };
    let (b, d, o) = (s(bundle), s(dir), s(out));
    let t = std::time::Instant::now();
    let v = match raea_react::motion::render_bundle(std::path::Path::new(&b), std::path::Path::new(&d), std::path::Path::new(&o), None) {
        Ok(said) => serde_json::json!({"code": 0, "said": said, "ms": t.elapsed().as_secs_f64() * 1e3}),
        Err(e) => serde_json::json!({"code": -1, "error": e}),
    };
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}

/// One frame of a composition bundle drawn by the same path as `raea-motion render --bundle B --frame N -o OUT.png`
/// on the Mac (React on JavaScriptCore, Blitz, rdb), so the two PNGs can be compared byte for byte.
/// Answers `{"code": 0, "ms": T}` or `{"code": -1, "error": "..."}`.
///
/// # Safety
/// The three are NUL-terminated paths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raea_motion_frame(bundle: *const c_char, dir: *const c_char, out: *const c_char, frame: i64) -> *mut c_char {
    let s = |p: *const c_char| unsafe { CStr::from_ptr(p).to_string_lossy().to_string() };
    let (b, d, o) = (s(bundle), s(dir), s(out));
    let t = std::time::Instant::now();
    let v = match raea_react::motion::render_bundle(std::path::Path::new(&b), std::path::Path::new(&d), std::path::Path::new(&o), Some(vec![frame])) {
        Ok(_) => serde_json::json!({"code": 0, "ms": t.elapsed().as_secs_f64() * 1e3}),
        Err(e) => serde_json::json!({"code": -1, "error": e}),
    };
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}
