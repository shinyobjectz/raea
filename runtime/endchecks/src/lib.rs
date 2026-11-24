//! A render's end checks as its frames come (docs/stage-guards.feature SG4a): each frame's parts read into what the
//! clipping check (raea-sys-contact; RAEA_CLIP_CHECK=0 skips it), the staging (raea-sys-stage; RAEA_STAGE_CHECK=0) and
//! the manifest's word on the parts (raea_sys_stage::Mounts) keep of them, and let go, so a long render never holds its
//! frames' parts for its checks. raea-runtime-export hands each frame in as it's drawn, and a still's at its end.

use raea_core_facts::Finding;
use raea_core_scene::Item;
use serde_json::Value;

pub struct EndChecks {
    clip: Option<raea_sys_contact::Clipping>,
    stage: Option<raea_sys_stage::Staging>,
    mounts: raea_sys_stage::Mounts,
    any: bool,
    parts: usize,
}

impl EndChecks {
    pub fn new(w: u32, h: u32, fps: f64) -> EndChecks {
        let on = |k: &str| std::env::var(k).as_deref() != Ok("0");
        let (clip, stage) = (on("RAEA_CLIP_CHECK").then(|| raea_sys_contact::Clipping::new(w, h, fps)), on("RAEA_STAGE_CHECK").then(|| raea_sys_stage::Staging::new(w, h, fps)));
        EndChecks { clip, stage, mounts: raea_sys_stage::Mounts::new(w, h), any: false, parts: 0 }
    }

    /// The next frame (its number, `frame`): its parts, and its page where the manifest has it.
    pub fn push(&mut self, frame: i64, its: &[Item], page: Option<&raea_sys_stage::Page>) {
        (self.any, self.parts) = (self.any || !its.is_empty(), self.parts + its.len());
        if let Some(c) = &mut self.clip {
            c.push(frame, its);
        }
        if let Some(s) = &mut self.stage {
            s.push(frame, its, page);
        }
        self.mounts.push(frame, its);
    }

    /// The findings, each said on stderr, and what the manifest says of the parts (none when no frame had any); `mark`
    /// after each step (raea-render's mem_mark: the footprint, with RAEA_MEM_MARKS).
    pub fn told(self, mark: impl Fn(&str)) -> (Vec<Finding>, Option<Value>) {
        if !self.any {
            return (Vec::new(), None);
        }
        mark(&format!("end checks: each frame's parts checked as they came, {} in all", self.parts));
        let seen = self.mounts.value();
        mark("end checks: what's on screen");
        let mut found = self.clip.map_or_else(Vec::new, raea_sys_contact::Clipping::told);
        mark("end checks: clipping");
        found.extend(self.stage.map_or_else(Vec::new, raea_sys_stage::Staging::told));
        mark("end checks: staging");
        // the clipping check's solids, kept for the process, aren't needed again by a render
        raea_sys_contact::forget();
        mark("end checks: done, their solids let go");
        (found, Some(seen))
    }
}
