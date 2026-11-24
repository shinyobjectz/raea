//! A render's frames as their parts, and the checks a render runs on them at its end: raea-sys-contact's clipping and
//! raea-sys-stage's staging (docs/stage-guards.feature SG10-SG14). `raea scene` (raea-app-scene) steps them without
//! drawing.

use super::{Frame, flag};

/// A frame's 3D walked into its parts (raea-runtime-scene).
pub fn items(f: &Frame) -> Vec<raea_core_scene::Item> {
    raea_runtime_scene::items_of(&f.scene)
}

/// Each frame's items, walked once (a frame's tree was walked again for each kind of part: 11 times a live frame)
pub fn all_items(frames: &[(i64, Frame)]) -> Vec<Vec<raea_core_scene::Item>> {
    frames.iter().map(|(_, f)| items(f)).collect()
}

/// The end checks (raea-runtime-endchecks) on a render's frames, each said on stderr: the parts that clip where the
/// camera sees them and the staging. `page` is the manifest's frames (each one's words in the page, which the staging
/// holds off the parts: SG11), None where they aren't known.
pub(crate) fn told(frames: &[(i64, Frame)], page: Option<&[serde_json::Value]>, w: u32, h: u32, fps: f64) -> Vec<raea_core_facts::Finding> {
    let nums = frames.iter().map(|f| f.0).collect();
    checked(nums, frames.iter().map(|f| raea_runtime_scene::items_of(&f.1.scene)), page, w, h, fps)
}

/// told for frames nothing reads the 3D of after: each frame's scene let go as its parts are checked (SG4a)
pub(crate) fn told_taking(frames: &mut [(i64, Frame)], page: Option<&[serde_json::Value]>, w: u32, h: u32, fps: f64) -> Vec<raea_core_facts::Finding> {
    let nums = frames.iter().map(|f| f.0).collect();
    checked(nums, frames.iter_mut().map(|(_, f)| raea_runtime_scene::items_of(&std::mem::take(&mut f.scene))), page, w, h, fps)
}

fn checked(nums: Vec<i64>, items: impl Iterator<Item = Vec<raea_core_scene::Item>>, page: Option<&[serde_json::Value]>, w: u32, h: u32, fps: f64) -> Vec<raea_core_facts::Finding> {
    let pages = page.map(|p| raea_sys_stage::pages(p, &nums, w, h));
    let mut e = raea_runtime_endchecks::EndChecks::new(w, h, fps);
    for (k, its) in items.enumerate() {
        e.push(nums[k], &its, pages.as_ref().and_then(|p| p.get(k)));
    }
    ended(e)
}

/// The end checks' findings, a memory mark after each step; what they say of the parts, in SEEN.
pub(crate) fn ended(e: raea_runtime_endchecks::EndChecks) -> Vec<raea_core_facts::Finding> {
    let (found, seen) = e.told(raea_render::engine::mem_mark);
    if seen.is_some() {
        *SEEN.lock().unwrap() = seen;
    }
    found
}

/// What the manifest says of a render's parts (raea_sys_stage::seen; write_manifest takes it): for stills each
/// frame's parts on screen, for a longer render where they remount.
pub(crate) static SEEN: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

/// `--frame N`, 0 when it isn't given (scene lint and render --review).
pub fn frame_of(args: &[String]) -> Result<i64, String> {
    match flag(args, "--frame") {
        Some(f) => f.parse().map_err(|_| "--frame takes a frame number".to_string()),
        None => Ok(0),
    }
}
