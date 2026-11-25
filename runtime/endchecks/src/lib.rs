//! A render's end checks as its frames come (docs/stage-guards.feature SG4a): each frame's parts read into what the
//! clipping check (raea-sys-contact; RAEA_CLIP_CHECK=0 skips it), the staging (raea-sys-stage; RAEA_STAGE_CHECK=0) and
//! the manifest's word on the parts (raea_sys_stage::Mounts) keep of them, and let go, so a long render never holds its
//! frames' parts for its checks. raea-runtime-export hands each frame in as it's drawn, and a still's at its end.
//! With them, each part held to its planned detail (raea-runtime-lod-plan; RAEA_LOD_CHECK=0 skips it): its own `lod`, else
//! its name's on the plan above the composition (its plan.rs: `lod` on the brief's make objects, MK22).

use raea_core_facts::Finding;
use raea_core_scene::Item;
use serde_json::Value;

pub struct EndChecks {
    clip: Option<raea_sys_contact::Clipping>,
    stage: Option<raea_sys_stage::Staging>,
    lod: Option<raea_runtime_lod_plan::Lods>,
    mounts: raea_sys_stage::Mounts,
    any: bool,
    parts: usize,
}

impl EndChecks {
    pub fn new(w: u32, h: u32, fps: f64) -> EndChecks {
        let on = |k: &str| std::env::var(k).as_deref() != Ok("0");
        let (clip, stage) = (on("RAEA_CLIP_CHECK").then(|| raea_sys_contact::Clipping::new(w, h, fps)), on("RAEA_STAGE_CHECK").then(|| raea_sys_stage::Staging::new(w, h, fps)));
        let lod = on("RAEA_LOD_CHECK").then(|| raea_runtime_lod_plan::Lods::new(w, h, fps, raea_runtime_lod_plan::plan::here(), false));
        EndChecks { clip, stage, lod, mounts: raea_sys_stage::Mounts::new(w, h), any: false, parts: 0 }
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
        if let Some(l) = &mut self.lod {
            l.push(frame, its);
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
        // parts shown closer than their planned detail, said as the staging's are
        let closer = self.lod.map_or_else(Vec::new, |l| l.findings(raea_sys_stage::BY));
        closer.iter().for_each(|f| eprintln!("raea: {}", f.line()));
        raea_core_facts::finding::said(&closer);
        found.extend(closer);
        // the clipping check's solids, kept for the process, aren't needed again by a render
        raea_sys_contact::forget();
        mark("end checks: done, their solids let go");
        (found, Some(seen))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn item(kind: &'static str, props: Value, shape: Option<&str>) -> Item {
        Item { kind, props: Arc::new(props.as_object().cloned().unwrap_or_default()), shape: shape.map(Arc::from), at: None, material: None, world: Default::default() }
    }

    // MK21: a render's end checks say a part shown closer than its planned detail, with its other findings
    #[test]
    fn a_render_says_a_part_shown_closer_than_its_planned_detail() {
        let frame = |z: f32, lod: u8| vec![item("camera", json!({"position": [0.0, 0.0, z], "lookAt": [0, 0, 0], "fov": 50}), None), item("mesh", json!({"name": "kiosk", "lod": lod}), Some("box(1, 1, 1)"))];
        let run = |z: f32, lod: u8| {
            let mut e = EndChecks::new(1280, 720, 30.0);
            e.push(0, &frame(z, lod), None);
            e.told(|_| {}).0.into_iter().filter(|f| f.check == "lod-closer").collect::<Vec<_>>()
        };
        let found = run(2.0, 4);
        assert_eq!((found.len(), found[0].level, found[0].by.as_str()), (1, raea_core_facts::Level::Error, "raea render"), "{found:?}");
        assert!(run(2.0, 0).is_empty() && run(80.0, 4).is_empty(), "a hero up close and a far part far away are as planned");
    }
}
