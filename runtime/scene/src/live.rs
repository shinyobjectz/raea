use super::*;

/// What live play hands a frame's 3D: its scene, and the colour clearing behind it.
#[derive(Clone, Copy)]
pub struct Shot<'a> {
    pub scene: &'a [Node3],
    pub clear: Option<&'a str>,
}

/// The kinds whose rows are known by their part's identity, not their place in the frame (SG2)
const BY_IDENTITY: [&str; 12] = ["light", "mesh", "model", "panel", "decal", "text", "plate", "fogvolume", "fire", "grass", "splat", "meshseq"];

/// A live game's 3D (live.rs): its rows made frame by frame from that frame's own scene, as play computed it, instead
/// of from a scripted run stepped ahead (scene_program). Parts are matched across frames by the identity `tracks`
/// gives them (a name, else kind and shape counted in tree order), so a part keeps its row as a render's does. The
/// columns a render takes from a part's first frame (shape, surface, light colour, a model's file) are taken from the
/// frame it first shows here; the columns a render keys come from each frame. A part missing from a frame is hidden (a
/// light goes dark), as a render's `visible` column hides it. Each frame only what changed goes to the engine.
#[derive(Default, Clone)]
pub struct LiveScene {
    /// identity (as `tracks` keys it) -> row id
    pub(crate) ids: std::collections::HashMap<String, String>,
    /// per row id: its kind, its first values, what was last sent, whether its colour has changed (a render keys
    /// col_r/g/b on every frame once it does), and the frames it has been missing
    pub(crate) rows: std::collections::HashMap<String, LiveRow>,
    /// rows made per kind of BY_IDENTITY: the next one's number
    pub(crate) made: [usize; 12],
    /// row id -> a part's kept inputs this frame (not a light's), for its row (the fast path's SG1 guard)
    pub(crate) kept_now: std::collections::HashMap<String, KeptInputs>,
    pub(crate) cam: BTreeMap<String, V>,
    /// frame 0 was stepped when play opened (live.rs), and nothing since
    pub warm: bool,
    /// identity -> a panel's `src`, or an <Instances>' placements (its shared data-at), as the full path last made its row
    /// (the fast path leaves it a new one)
    pub(crate) panel_srcs: std::collections::HashMap<String, std::sync::Arc<str>>,
    /// what the lights' sets and the parts' components were made from when the full path last made them (scene_sig):
    /// the fast path, which doesn't make them, goes the full way when it differs
    pub(crate) light_sig: String,
}

#[derive(Clone)]
pub(crate) struct LiveRow {
    pub(crate) kind: String,
    pub(crate) first: BTreeMap<String, V>,
    pub(crate) sent: BTreeMap<String, V>,
    pub(crate) recoloured: bool,
    pub(crate) missing: u32,
    /// seen this frame (frame_fast's mark)
    pub(crate) seen: bool,
    pub(crate) kept: Option<KeptInputs>,
}

impl LiveScene {
    /// Start again (the engine was dropped: a new size).
    pub fn reset(&mut self) {
        *self = LiveScene::default();
    }

    /// The frame's camera row, as a render's program holds it at this frame (camera.rs), its spawns into `p`: a frame
    /// without a camera holds the one before.
    pub(crate) fn camera_row(&self, all: &[Vec<Item>], clear: Option<&str>, p: &mut Program) -> BTreeMap<String, V> {
        let mut cam = BTreeMap::new();
        cam.insert("id".into(), V::S("cam".into()));
        // the holder's background as the clear colour; with none, Bevy's ClearColor resource (srgb 43, 44, 47 unless a
        // <BevyResource type="bevy_camera::clear_color::ClearColor"> sets it), not black (docs/control conventions C17)
        if let Some(bg) = clear.and_then(hex_of) {
            // and keyed frame by frame (bg_r..bg_a, sRGB), as a render keys a holder's background that changes: each
            // frame's own, where play held the first frame's (RR1: render = play)
            if let Some(c) = rgba_of(&bg) {
                for (k, name) in ["bg_r", "bg_g", "bg_b", "bg_a"].into_iter().enumerate() {
                    cam.insert(name.into(), V::N(r6(c[k])));
                }
            }
            cam.insert("bg".into(), V::S(bg));
        }
        camera(all, p, &mut cam);
        // its post-processing as a table, so a frame's sky, fog, grade or depth of field replaces the last whole
        // (as_table; depth of field's aperture and focus stay keyed columns, so a pull doesn't change the set)
        let set = frame_post(&all[0]);
        as_table(&mut cam, |k| set.contains_key(k));
        // an <Environment>'s panoramas: every one shown so far, as a render's row lists them all (#38)
        env_shown(self.cam.get("env_srcs"), &mut cam);
        if !all[0].iter().any(|i| i.kind == "camera") {
            // no camera this frame: a render holds the one before (filled)
            for k in ["x", "y", "z", "look_x", "look_y", "look_z", "fov", "focus"] {
                if let Some(v) = self.cam.get(k) {
                    cam.insert(k.into(), v.clone());
                }
            }
        }
        cam
    }

    /// What the camera row changes, of the columns a render keys (cam_keyed): the camera's tuple, or None.
    pub(crate) fn camera_diff(sent: &mut BTreeMap<String, V>, cam: &BTreeMap<String, V>) -> Option<raea_core_program::eval::Tup> {
        let mut vals = BTreeMap::new();
        // and each frame's post-processing table and page background (RR1: render = play), and its shot (RR2)
        let keyed = |k: &str| cam_keyed(k) || matches!(k, "frame_sets" | "frame_set_i" | "bg_r" | "bg_g" | "bg_b" | "bg_a" | "shot");
        // the ambient and the shot back to 0 when the row stops holding them (no light; the first camera's shot again)
        for k in cam.keys().map(String::as_str).filter(|k| keyed(k)).chain(["ambient", "shot"]) {
            let v = cam.get(k).cloned().or_else(|| (matches!(k, "ambient" | "shot") && sent.contains_key(k)).then_some(V::N(0.0)));
            if let Some(v) = v {
                if sent.get(k) != Some(&v) {
                    sent.insert(k.into(), v.clone());
                    vals.insert(k.to_string(), v);
                }
            }
        }
        (!vals.is_empty()).then(|| raea_core_program::eval::Tup { kind: "camera".into(), id: "cam".into(), vals })
    }

    /// The frame's rows as a render's program would hold them at this frame: the camera row and each part's full row,
    /// by identity, with the row ids this scene keeps.
    pub(crate) fn current(&mut self, f: Shot) -> (BTreeMap<String, V>, Vec<(String, String, BTreeMap<String, V>)>) {
        let all = vec![items_of(f.scene)];
        self.light_sig = scene_sig(&all[0]);
        self.kept_now.clear();
        // the camera's own spawns (its fog, sky...) in the frame's program with the rest
        let mut p = Program::default();
        let cam = self.camera_row(&all, f.clear, &mut p);
        let tr = Tracks::of(&all);
        rows_into_of(&all, &tr, &mut p);
        for (key, src) in tr.panels.iter().filter_map(|(k, t)| Some((k, t[0].as_ref()?.props.get("src")?.as_str()?))) {
            self.panel_srcs.insert(key.clone(), src.into());
        }
        for (key, at) in tr.meshes.iter().chain(&tr.models).filter_map(|(k, t)| Some((k, t[0].as_ref()?.at.clone()?))) {
            self.panel_srcs.insert(key.clone(), at);
        }
        // each local row (this frame's tracks, numbered as rows_into numbers them) by its identity
        let mut local: std::collections::HashMap<String, &String> = std::collections::HashMap::new();
        for (li, (key, track)) in tr.lights.iter().enumerate() {
            let kind = track[0].as_ref().map(|i| i.kind).unwrap_or("directional");
            local.insert(if kind == "directional" { format!("sun{li}") } else { format!("{kind}{li}") }, key);
        }
        // a text block too (SG2: by its frame's place, a block mounted as another went took that one's row and words), and
        // footage, splats, sequences, fog, fires and grass (a plate mounted as another went kept the other's clip)
        for (prefix, group) in [("mesh", &tr.meshes), ("model", &tr.models), ("plane", &tr.panels), ("decal", &tr.decals), ("text", &tr.texts), ("plate", &tr.plates), ("fog", &tr.fogs), ("fire", &tr.fires), ("grass", &tr.grasses), ("splat", &tr.splats), ("meshseq", &tr.meshseqs)] {
            for (k, (key, _)) in group.iter().enumerate() {
                local.insert(format!("{prefix}{k}"), key);
            }
        }
        let mut out = Vec::new();
        // the other kinds (bare entities, Bevy components and resources, sprites, 2D shapes...): by the ids a render
        // gives them, a component's target as the live rows name it (G15)
        let mut others: Vec<(String, BTreeMap<String, V>)> = Vec::new();
        let mut live_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (kind, rows) in p.spawns {
            let Some(group) = BY_IDENTITY.iter().position(|g| *g == kind) else {
                if kind != "camera" {
                    others.extend(rows.into_iter().map(|r| (kind.clone(), r)));
                }
                continue;
            };
            for mut row in rows {
                let Some(V::S(lid)) = row.get("id").cloned() else { continue };
                let Some(key) = local.get(&lid) else { continue };
                let id = match self.ids.get(key.as_str()) {
                    Some(id) => id.clone(),
                    None => {
                        let prefix: String = lid.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                        let id = format!("{prefix}{}", self.made[group]);
                        self.made[group] += 1;
                        self.ids.insert((*key).clone(), id.clone());
                        id
                    }
                };
                live_id.insert(lid.clone(), id.clone());
                if let Some(it) = [("mesh", &tr.meshes), ("model", &tr.models), ("plane", &tr.panels), ("decal", &tr.decals)].into_iter().find_map(|(p, g)| g.get(lid.strip_prefix(p)?.parse::<usize>().ok()?)?.1[0].as_ref()) {
                    self.kept_now.insert(id.clone(), KeptInputs::of(it));
                }
                row.insert("id".into(), V::S(id.clone()));
                // a light's colour, shadows and shafts as a table, so a frame's replaces the last whole (as_table)
                if kind == "light" {
                    as_table(&mut row, light_set_col);
                }
                out.push((kind.clone(), id, row));
            }
        }
        // a model's references to other models (attach_to, bone_rows: a <Character>'s skin follows its joints) name
        // this frame's local ids; they become this scene's ids, or a skin mounted after the first (a keyed <Character>,
        // person after person) follows rows that are someone else's, or none
        for (_, _, row) in out.iter_mut() {
            for k in ["attach_to", "bone_rows"] {
                if let Some(V::S(refs)) = row.get(k) {
                    let ids: Vec<&str> = refs.split(',').map(|r| live_id.get(r).map_or(r, String::as_str)).collect();
                    row.insert(k.into(), V::S(ids.join(",")));
                }
            }
        }
        // a target of a kind that keeps its render id (a sprite, a 2D shape) is that row's
        let kept: std::collections::HashSet<String> = others.iter().filter_map(|(_, r)| match r.get("id") { Some(V::S(i)) => Some(i.clone()), _ => None }).collect();
        for (kind, mut row) in others {
            let Some(V::S(mut id)) = row.get("id").cloned() else { continue };
            if let Some(V::S(t)) = row.get("target").cloned() {
                let Some(to) = (if t.starts_with("ent") || t == "cam" || kept.contains(&t) { Some(t.clone()) } else { live_id.get(&t).cloned() }) else { continue };
                id = id.replacen(&t, &to, 1);
                row.insert("id".into(), V::S(id.clone()));
                row.insert("target".into(), V::S(to));
            }
            out.push((kind, id, row));
        }
        (cam, out)
    }

    /// The program the live engine opens with: this frame's camera and parts (a render's rows at this frame).
    pub fn open(&mut self, f: Shot, w: u32, h: u32, fps: f64) -> Program {
        self.reset();
        let (cam, rows) = self.current(f);
        let mut p = Program { fps, width: w, height: h, frames: 1, ..Default::default() };
        self.cam = cam.clone();
        // the cameras after the first (two_passes' second pass), as this frame places them: they hold that place in play
        let more = camera(&[items_of(f.scene)], &mut Program::default(), &mut cam.clone());
        p.spawns.push(("camera".into(), std::iter::once(cam).chain(more).collect()));
        for (kind, id, row) in rows {
            let first = row.clone();
            let kept = self.kept_now.remove(&id);
            self.rows.insert(id, LiveRow { kind: kind.clone(), first, sent: row.clone(), recoloured: false, missing: 0, seen: false, kept });
            p.spawns.push((kind, vec![row]));
        }
        p
    }

    /// What this frame changes: the rows to set (a new part's whole row; for one seen before, only the columns whose
    /// values changed) and the rows to drop (missing LIVE_FORGET frames).
    pub fn frame(&mut self, f: Shot) -> (Vec<raea_core_program::eval::Tup>, Vec<(String, String)>) {
        // RAEA_LIVE3D_CHECK=1: each frame also diffed from a render's full rows on a copy, and the two must agree
        let check = std::env::var_os("RAEA_LIVE3D_CHECK").is_some();
        let before = check.then(|| self.clone());
        let fast = std::env::var("RAEA_LIVE3D_FAST").as_deref() != Ok("0");
        let (out, used_fast) = match fast.then(|| self.frame_fast(f)).flatten() {
            Some(out) => (out, true),
            None => {
                let (cam, rows) = self.current(f);
                (self.diff(cam, rows), false)
            }
        };
        if let (Some(mut slow), true) = (before, used_fast) {
            let (cam, rows) = slow.current(f);
            let want = slow.diff(cam, rows);
            assert!(want == out, "RAEA_LIVE3D_CHECK: fast rows {:?} != full rows {:?}", out, want);
            assert!(slow.cam == self.cam, "RAEA_LIVE3D_CHECK: camera {:?} != {:?}", self.cam, slow.cam);
            assert!(slow.rows.len() == self.rows.len(), "RAEA_LIVE3D_CHECK: {} rows kept, {} on the full path", self.rows.len(), slow.rows.len());
            for (id, r) in &slow.rows {
                assert!(self.rows.get(id).is_some_and(|m| m.sent == r.sent && m.recoloured == r.recoloured && m.missing == r.missing), "RAEA_LIVE3D_CHECK: row {id}");
            }
        }
        out
    }

    /// What this frame changes, from its camera row and parts' rows (`current`'s)
    pub(crate) fn diff(&mut self, cam: BTreeMap<String, V>, rows: Vec<(String, String, BTreeMap<String, V>)>) -> (Vec<raea_core_program::eval::Tup>, Vec<(String, String)>) {
        let mut out: Vec<raea_core_program::eval::Tup> = Vec::new();
        // the camera: whatever of it a render keys, and the ambient light (0 once there's none)
        out.extend(Self::camera_diff(&mut self.cam, &cam));
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut dropped = Vec::new();
        for (kind, id, row) in rows {
            seen.insert(id.clone());
            // a kept column changed (its shape, its words, a map): dropped with the rows under it and spawned again with
            // this frame's values, which the engine would otherwise never read (SG1)
            if self.rows.get(&id).is_some_and(|r| respawns(&kind, &r.first, &row)) {
                self.drop_with_under(&id, &mut dropped);
            }
            let kept = self.kept_now.remove(&id);
            let Some(r) = self.rows.get_mut(&id) else {
                // first seen: its whole row, spawned with these values
                self.rows.insert(id.clone(), LiveRow { kind: kind.clone(), first: row.clone(), sent: row.clone(), recoloured: false, missing: 0, seen: false, kept });
                out.push(raea_core_program::eval::Tup { kind, id, vals: row.into_iter().filter(|(k, _)| k != "id").collect() });
                continue;
            };
            r.missing = 0;
            r.kept = kept;
            let mut want: BTreeMap<String, V> = BTreeMap::new();
            if !LIVE_GROUPS.contains(&kind.as_str()) {
                // a kind the groups don't key column by column: every column it has, sent when it changes
                for (k, v) in row.iter().filter(|(k, _)| k.as_str() != "id") {
                    want.insert(k.clone(), v.clone());
                }
            }
            for k in live_dynamic(&kind) {
                if let Some(v) = row.get(*k) {
                    want.insert(k.to_string(), v.clone());
                }
            }
            // a glyph's too: its `color` is read only at spawn
            if matches!(kind.as_str(), "mesh" | "glyph") {
                // a colour other than its first: keyed from now on, as a render keys it once a part's colour changes
                if row.get("color") != r.first.get("color") {
                    r.recoloured = true;
                }
                if r.recoloured {
                    if let Some(V::S(c)) = row.get("color") {
                        let c = rgb(c);
                        for (k, name) in ["col_r", "col_g", "col_b"].iter().enumerate() {
                            want.insert(name.to_string(), V::N(c[k]));
                        }
                    }
                }
            }
            if kind == "mesh" {
                // the emissive colour the frame it differs from what was sent (none is black), with its strength (0 once black:
                // #106, a video kept the first frame's); else the strength only where the sent colour glows (a render's column)
                let black = || V::S("#000000".into());
                let (em, was) = (row.get("emissive").cloned().unwrap_or_else(black), r.sent.get("emissive").cloned().unwrap_or_else(black));
                match em {
                    _ if em != was => drop((want.insert("emissive".into(), em), want.entry("emissive_k".into()).or_insert(V::N(0.0)))),
                    _ if em == black() => drop(want.remove("emissive_k")),
                    _ => drop(want.entry("emissive_k".into()).or_insert(V::N(1.0))),
                }
            }
            if matches!(kind.as_str(), "mesh" | "glyph") && r.sent.contains_key("opacity") {
                want.entry("opacity".into()).or_insert(V::N(1.0));
            }
            if (r.sent.contains_key("visible") || kind != "light") && !matches!(kind.as_str(), "component" | "resource") {
                want.entry("visible".into()).or_insert(V::N(1.0));
            }
            for k in ["scale_x", "scale_y", "scale_z"] {
                if r.sent.contains_key(k) {
                    want.entry(k.into()).or_insert(V::N(1.0));
                }
            }
            if kind == "light" {
                want.retain(|k, _| k != "visible");
            }
            let changed: BTreeMap<String, V> = want.into_iter().filter(|(k, v)| r.sent.get(k) != Some(v)).collect();
            if !changed.is_empty() {
                for (k, v) in &changed {
                    r.sent.insert(k.clone(), v.clone());
                }
                out.push(raea_core_program::eval::Tup { kind, id, vals: changed });
            }
        }
        // missing this frame: hidden (a light dark); dropped once it's been gone long enough
        let mut gone = Vec::new();
        for (id, r) in self.rows.iter_mut() {
            if seen.contains(id) {
                continue;
            }
            r.missing += 1;
            if r.missing > LIVE_FORGET {
                gone.push((r.kind.clone(), id.clone()));
                continue;
            }
            let off: Vec<(&str, V)> = match r.kind.as_str() {
                "light" => vec![(if r.first.contains_key("lux") { "lux" } else { "lumens" }, V::N(0.0))],
                // a component or resource row stays as last set (its part's own visibility hides it)
                "component" | "resource" => vec![],
                _ => vec![("visible", V::N(0.0))],
            };
            let mut vals = BTreeMap::new();
            for (k, v) in off {
                if r.sent.get(k) != Some(&v) {
                    r.sent.insert(k.to_string(), v.clone());
                    vals.insert(k.to_string(), v);
                }
            }
            if !vals.is_empty() {
                out.push(raea_core_program::eval::Tup { kind: r.kind.clone(), id: id.clone(), vals });
            }
        }
        // the rows under another first (a glyph, a component): a block's entity takes its glyphs' with it (SG4)
        gone.sort_by_key(|(_, id)| !self.rows.get(id).is_some_and(|r| r.first.contains_key("target")));
        for (_, id) in &gone {
            self.rows.remove(id);
            self.ids.retain(|_, v| v != id);
            self.panel_srcs.retain(|k, _| self.ids.contains_key(k));
        }
        dropped.extend(gone);
        (out, dropped)
    }

    /// A row dropped with the rows under it (its glyphs, its components), those under it first (SG4)
    fn drop_with_under(&mut self, id: &str, gone: &mut Vec<(String, String)>) {
        let under: Vec<String> = self.rows.iter().filter(|(_, r)| matches!(r.first.get("target"), Some(V::S(t)) if t == id)).map(|(u, _)| u.clone()).collect();
        for u in under.iter().map(String::as_str).chain([id]) {
            if let Some(r) = self.rows.remove(u) {
                gone.push((r.kind, u.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(props: Value) -> Node3 {
        Node3 { kind: "Model".into(), props: std::sync::Arc::new(props.as_object().unwrap().clone()), shape: None, at: None, kids: Vec::new().into() }
    }

    fn person(who: &str) -> Vec<Node3> {
        vec![model(json!({"name": format!("{who}-head"), "src": format!("/media/{who}/joint.glb")})), model(json!({"name": format!("{who}-body"), "src": format!("/media/{who}/body.glb"), "bones": {"head": format!("{who}-head")}}))]
    }

    /// A keyed <Character> swapping people: the second person's skin follows its own joints by this scene's ids, not
    /// the first frame's local ids (which are the first person's rows: "joint head follows model0", 10-27).
    #[test]
    fn a_later_skin_follows_its_own_joints() {
        let mut ls = LiveScene::default();
        let a = person("a");
        ls.open(Shot { scene: &a, clear: None }, 64, 64, 30.0);
        let b = person("b");
        let (set, _) = ls.frame(Shot { scene: &b, clear: None });
        let id_of = |name: &str| set.iter().find(|t| t.vals.get("name") == Some(&V::S(name.into()))).map(|t| t.id.clone()).unwrap();
        let body = set.iter().find(|t| t.vals.get("name") == Some(&V::S("b-body".into()))).unwrap();
        assert_eq!(body.vals.get("bone_rows"), Some(&V::S(id_of("b-head"))));
    }

    /// A glyph faded in (`reveal`: 0, 0.5, 1) is opaque once done, not left at its last fade (#38: bevy/text3d's tag).
    #[test]
    fn a_faded_glyph_is_opaque_once_done() {
        let text = |o: f64| vec![Node3 { kind: "RichText".into(), props: std::sync::Arc::new(json!({"spans": [{"t": "a"}], "glyphs": [{"at": 0, "span": 0, "ch": "a", "o": o}]}).as_object().unwrap().clone()), shape: None, at: None, kids: Vec::new().into() }];
        let mut ls = LiveScene::default();
        ls.open(Shot { scene: &text(0.5), clear: None }, 64, 64, 30.0);
        let (set, _) = ls.frame(Shot { scene: &text(1.0), clear: None });
        assert_eq!(set.iter().find(|t| t.kind == "glyph").and_then(|t| t.vals.get("opacity")), Some(&V::N(1.0)));
    }

    /// Two cameras cut between (<Camera3d is_active>): the camera row's shot is sent the frame it changes, and 0 when
    /// the first camera is back, so a live engine draws both frames as cuts (RR2, Engine::set_rows).
    #[test]
    fn a_cut_back_sends_the_first_shot() {
        let cams = |on: usize| -> Vec<Node3> { (0..2).map(|i| Node3 { kind: "PerspectiveCamera".into(), props: std::sync::Arc::new(json!({"position": [i, 1, 5], "active": i == on}).as_object().unwrap().clone()), shape: None, at: None, kids: Vec::new().into() }).collect() };
        let shot = |set: &[raea_core_program::eval::Tup]| set.iter().find(|t| t.kind == "camera").and_then(|t| t.vals.get("shot").cloned());
        let mut ls = LiveScene::default();
        ls.open(Shot { scene: &cams(0), clear: None }, 64, 64, 30.0);
        assert_eq!(shot(&ls.frame(Shot { scene: &cams(1), clear: None }).0), Some(V::N(1.0)));
        assert_eq!(shot(&ls.frame(Shot { scene: &cams(1), clear: None }).0), None);
        assert_eq!(shot(&ls.frame(Shot { scene: &cams(0), clear: None }).0), Some(V::N(0.0)));
    }
}
