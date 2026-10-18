//! The C interface (the roadmap A6): what an app calls to read footage with raea's perception. Each call takes UTF-8
//! paths and returns a JSON string the caller frees with raea_perception_free; on failure the JSON is {"error": "..."}.
//! Header: perception/include/raea_perception.h.

use std::ffi::{CStr, CString, c_char};
use std::path::Path;

fn arg(p: *const c_char) -> Result<String, String> {
    if p.is_null() {
        return Err("a null path".into());
    }
    unsafe { CStr::from_ptr(p) }.to_str().map(String::from).map_err(|e| e.to_string())
}

fn out(r: Result<serde_json::Value, String>) -> *mut c_char {
    let v = r.unwrap_or_else(|e| serde_json::json!({"error": e}));
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}

/// Ingest a clip once (facts, sound, performance, SigLIP columns) into `store` (null: the default store), keyed by its
/// content: {"dir", "hash", "steps"}.
#[unsafe(no_mangle)]
pub extern "C" fn raea_perception_ingest(clip: *const c_char, store: *const c_char) -> *mut c_char {
    out((|| {
        let clip = arg(clip)?;
        let store = if store.is_null() { crate::ingest::store() } else { arg(store)?.into() };
        let (dir, m) = crate::ingest::ingest(Path::new(&clip), &store, "apple", &crate::vision::AppleVision::default())?;
        Ok(serde_json::json!({"dir": dir, "hash": m.hash, "steps": m.steps}))
    })())
}

/// A clip's facts into `dir` (facts.json and a still per shot), returned as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn raea_perception_analyze(clip: *const c_char, dir: *const c_char) -> *mut c_char {
    out((|| {
        let f = crate::ingest::video(Path::new(&arg(clip)?), Path::new(&arg(dir)?), "apple", &crate::vision::AppleVision::default())?;
        serde_json::to_value(&f).map_err(|e| e.to_string())
    })())
}

/// Frees a string this library returned.
#[unsafe(no_mangle)]
pub extern "C" fn raea_perception_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_clip_is_an_error_not_a_crash() {
        let clip = CString::new("/nonexistent/clip.mp4").unwrap();
        let r = raea_perception_analyze(clip.as_ptr(), clip.as_ptr());
        let s = unsafe { CStr::from_ptr(r) }.to_str().unwrap().to_string();
        raea_perception_free(r);
        assert!(s.contains("\"error\""), "{s}");
    }

    #[test]
    fn a_null_path_is_an_error() {
        let r = raea_perception_ingest(std::ptr::null(), std::ptr::null());
        let s = unsafe { CStr::from_ptr(r) }.to_str().unwrap().to_string();
        raea_perception_free(r);
        assert!(s.contains("null path"), "{s}");
    }
}
