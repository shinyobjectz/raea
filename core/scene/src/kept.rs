//! What a part's row keeps from the frame it was spawned in, and the guards that keep a later frame's change from being
//! lost (docs/stage-guards.feature SG1). The engine builds a part's mesh, its words' layout and its surface's maps once,
//! when its row is spawned; a later frame reaches only the columns it takes frame by frame (live_dynamic). So a live
//! scene spawns a part again when one of its kept columns changes (`respawns`), and a render's tracks are cut where a
//! part's kept inputs change (`split_kept`). A Sphere whose radius grows, a RichText whose words change and a Ribbon's
//! newest box each draw as their own frame has them, never as their first frame did. (11-21: every curve of the
//! mobile-ads essay broke into dashes, its counters read "0.0 MB" and its typed lines stopped half-typed.)

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::Item;

/// A row missing this many frames is dropped from the engine (its entity, mesh and surface freed) and forgotten: a game
/// that spawns entities with fresh names (`rock-${id}`) would otherwise keep every one it ever made, hidden. Coming back
/// later, it's a new row with the same values, so nothing drawn changes.
pub const LIVE_FORGET: u32 = 240;

/// The kinds a live frame keys column by column (live_dynamic); every other kind is diffed whole.
pub const LIVE_GROUPS: [&str; 5] = ["light", "mesh", "model", "panel", "decal"];

/// A <Model animations>'s keyed columns (its clips' times and weights) and a <Model morphs>'s (its targets' weights).
pub const MIX_COLS: [(&str, &str); 4] = [("mix0_t", "mix0_w"), ("mix1_t", "mix1_w"), ("mix2_t", "mix2_w"), ("mix3_t", "mix3_w")];
pub const GRAPH_COLS: [[&str; 2]; 16] = [["g0_w", "g0_t"], ["g1_w", "g1_t"], ["g2_w", "g2_t"], ["g3_w", "g3_t"], ["g4_w", "g4_t"], ["g5_w", "g5_t"], ["g6_w", "g6_t"], ["g7_w", "g7_t"], ["g8_w", "g8_t"], ["g9_w", "g9_t"], ["g10_w", "g10_t"], ["g11_w", "g11_t"], ["g12_w", "g12_t"], ["g13_w", "g13_t"], ["g14_w", "g14_t"], ["g15_w", "g15_t"]];
/// Up to 64 morph weights a model (Bevy takes 256; a MakeHuman face has 34 expression units).
pub const MORPH_COLS: [&str; 64] = ["morph0", "morph1", "morph2", "morph3", "morph4", "morph5", "morph6", "morph7", "morph8", "morph9", "morph10", "morph11", "morph12", "morph13", "morph14", "morph15", "morph16", "morph17", "morph18", "morph19", "morph20", "morph21", "morph22", "morph23", "morph24", "morph25", "morph26", "morph27", "morph28", "morph29", "morph30", "morph31", "morph32", "morph33", "morph34", "morph35", "morph36", "morph37", "morph38", "morph39", "morph40", "morph41", "morph42", "morph43", "morph44", "morph45", "morph46", "morph47", "morph48", "morph49", "morph50", "morph51", "morph52", "morph53", "morph54", "morph55", "morph56", "morph57", "morph58", "morph59", "morph60", "morph61", "morph62", "morph63"];

/// The columns a frame may change, per kind (what `rows_into` and `camera` key); everything else is the first frame's.
pub fn live_dynamic(kind: &str) -> &'static [&'static str] {
    match kind {
        "light" => &["lux", "lumens", "x", "y", "z", "dir_x", "dir_y", "dir_z", "rot_qx", "rot_qy", "rot_qz", "rot_qw", "inner", "outer", "frame_sets", "frame_set_i"],
        "mesh" => &["x", "y", "z", "q_x", "q_y", "q_z", "q_w", "scale_x", "scale_y", "scale_z", "visible", "col_r", "col_g", "col_b", "rough", "metal", "emissive_k", "opacity", "sh_r", "sh_g", "sh_b", "rim_k", "spec_k", "outline_w", "pass", "inst"],
        "model" => &["x", "y", "z", "q_x", "q_y", "q_z", "q_w", "scale_x", "scale_y", "scale_z", "visible", "anim_t", "mix0_t", "mix0_w", "mix1_t", "mix1_w", "mix2_t", "mix2_w", "mix3_t", "mix3_w", "morph0", "morph1", "morph2", "morph3", "morph4", "morph5", "morph6", "morph7", "morph8", "morph9", "morph10", "morph11", "morph12", "morph13", "morph14", "morph15", "morph16", "morph17", "morph18", "morph19", "morph20", "morph21", "morph22", "morph23", "morph24", "morph25", "morph26", "morph27", "morph28", "morph29", "morph30", "morph31", "morph32", "morph33", "morph34", "morph35", "morph36", "morph37", "morph38", "morph39", "morph40", "morph41", "morph42", "morph43", "morph44", "morph45", "morph46", "morph47", "morph48", "morph49", "morph50", "morph51", "morph52", "morph53", "morph54", "morph55", "morph56", "morph57", "morph58", "morph59", "morph60", "morph61", "morph62", "morph63", "g0_w", "g0_t", "g1_w", "g1_t", "g2_w", "g2_t", "g3_w", "g3_t", "g4_w", "g4_t", "g5_w", "g5_t", "g6_w", "g6_t", "g7_w", "g7_t", "g8_w", "g8_t", "g9_w", "g9_t", "g10_w", "g10_t", "g11_w", "g11_t", "g12_w", "g12_t", "g13_w", "g13_t", "g14_w", "g14_t", "g15_w", "g15_t", "look", "glitch_explode", "glitch_scramble", "glitch_jitter", "glitch_snap", "glitch_smear_x", "glitch_smear_y", "glitch_smear_z", "glitch_seed", "cloth_frame", "screen_power", "pass", "inst"],
        "panel" => &["x", "y", "z", "q_x", "q_y", "q_z", "q_w", "scale_x", "scale_y", "scale_z", "visible", "screen_power", "screen_bright", "pass", "sheet_frame", "sheet_cols", "sheet_rows", "src"],
        "decal" => &["x", "y", "z", "q_x", "q_y", "q_z", "q_w", "scale_x", "scale_y", "scale_z", "visible", "col_r", "col_g", "col_b", "opacity", "rough", "metal"],
        _ => &[],
    }
}

/// A camera column a render keys when it changes (raea-runtime-look camera.rs), which a live frame sends when it does:
/// its place and look, lens, roll (up_*), view window, ambient, sky and environment, aberration, depth of field, motion
/// blur, the paint look and its parts' rotations (*_qx..qw). The rest is the first frame's, as a render's is.
pub fn cam_keyed(k: &str) -> bool {
    // paint_cut: the paint look's worlds, each cut's centre, radius and style moving frame by frame (raea-runtime-look paint_cut_rows)
    ["lens_", "mb_", "up_", "view_", "paint_cut"].iter().any(|p| k.starts_with(p))
        || ["_qx", "_qy", "_qz", "_qw"].iter().any(|q| k.ends_with(q))
        || matches!(k, "x" | "y" | "z" | "look_x" | "look_y" | "look_z" | "fov" | "focus" | "ortho" | "ambient" | "skybox_k" | "env_srcs" | "env_i" | "env_k" | "env_rot" | "sky_k" | "aberration" | "dof_aperture" | "paint_bands" | "paint_grain" | "paint_rag" | "paint_seed")
}

/// What a text block or a glyph takes frame by frame: its placement and whether it shows (text3d's sync_texts,
/// scene3d's sync_meshes; a glyph's colour and fade too, as col_* and opacity).
const PLACED: [&str; 16] = ["x", "y", "z", "q_x", "q_y", "q_z", "q_w", "scale_x", "scale_y", "scale_z", "visible", "pass", "col_r", "col_g", "col_b", "opacity"];

/// A column a part's row keeps from its spawn, so that a change to it needs the part spawned again: not one the engine
/// takes frame by frame (live_dynamic, PLACED), nor its identity. A mesh's and a glyph's colour and a mesh's glow are
/// sent as columns once they change. A shader's uniforms ride in `three` and `shader`, which change on each frame an
/// animated uniform does: those stay with their own fix (frozen uniforms), not a part spawned every frame. A model is
/// spawned again only for a new file, never for its animation's columns. The kinds a row takes whole from its first
/// frame (footage behind the 3D, a splat, a baked sequence, a fog's density texture, a fire's simulation, a field of
/// grass) keep what they are: a plate on another's row kept the old clip (SG2).
pub fn kept_col(kind: &str, col: &str) -> bool {
    if matches!(col, "id" | "name") {
        return false;
    }
    match kind {
        "mesh" => !live_dynamic(kind).contains(&col) && !matches!(col, "color" | "emissive" | "three" | "shader"),
        "panel" | "decal" => !live_dynamic(kind).contains(&col),
        "model" => col == "src",
        "text" => !PLACED.contains(&col),
        "glyph" => matches!(col, "target" | "at" | "span" | "ch"),
        _ => WHOLE.iter().any(|(k, cols, _)| *k == kind && cols.contains(&col)),
    }
}

/// Whether a row's kept columns differ from those its part was spawned with (`first`), so it must be spawned again.
pub fn respawns<T: PartialEq>(kind: &str, first: &std::collections::BTreeMap<String, T>, now: &std::collections::BTreeMap<String, T>) -> bool {
    let differs = |a: &std::collections::BTreeMap<String, T>, b: &std::collections::BTreeMap<String, T>| a.iter().any(|(k, v)| kept_col(kind, k) && b.get(k) != Some(v));
    differs(first, now) || differs(now, first)
}

/// A material's props that reach only columns taken frame by frame (colour, roughness, glow, fade, a toon surface's
/// keyed values) or the part's identity (its shader's source: surface_ident), so they never cut a track.
const MAT_LIVE: [&str; 14] = ["color", "roughness", "metalness", "emissive", "emissiveIntensity", "opacity", "shadowColor", "rimIntensity", "highlightIntensity", "outlineWidth", "uniforms", "shader", "vertexShader", "fragmentShader"];
/// A mesh's own props that its row keeps (meshes_rows): shadows, a page on it, its geometry's attributes, its layers.
const MESH_KEPT: [&str; 9] = ["shadow", "castShadow", "receiveShadow", "surface", "material", "driven", "attributes", "compressed", "layers"];
/// A RichText's props that its block keeps (texts_rows): its words, their runs, em, alignment, wrap and surface.
const TEXT_KEPT: [&str; 13] = ["text", "spans", "size", "align", "lineHeight", "maxWidth", "roughness", "metalness", "emissive", "emissiveIntensity", "unlit", "castShadow", "receiveShadow"];

fn hash_value(v: &Value, h: &mut impl Hasher) {
    match v {
        Value::Null => 0u8.hash(h),
        Value::Bool(b) => b.hash(h),
        Value::Number(n) => n.as_f64().unwrap_or(0.0).to_bits().hash(h),
        Value::String(s) => s.hash(h),
        Value::Array(a) => a.iter().for_each(|x| hash_value(x, h)),
        Value::Object(o) => o.iter().for_each(|(k, x)| {
            k.hash(h);
            hash_value(x, h);
        }),
    }
}

fn hash_props(m: &Map<String, Value>, h: &mut impl Hasher, take: impl Fn(&str) -> bool) {
    for (k, v) in m.iter().filter(|(k, _)| take(k)) {
        k.hash(h);
        hash_value(v, h);
    }
}

/// A part's kept inputs as one number: what its row takes from the frame it's spawned in (its shape, its material's
/// maps and modes, its words and their glyphs, a model's file). Two frames of a part with the same number make the
/// same kept columns.
pub fn spawn_sig(it: &Item) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    it.kind.hash(&mut h);
    match it.kind {
        "mesh" => {
            it.shape.hash(&mut h);
            if let Some(m) = &it.material {
                m.kind.hash(&mut h);
                // a three.js shader's spec is its uniforms and its source, the one keyed and the other its identity
                if &*m.kind != "ThreeShaderMaterial" {
                    hash_props(&m.props, &mut h, |k| !MAT_LIVE.contains(&k));
                }
            }
            hash_props(&it.props, &mut h, |k| MESH_KEPT.contains(&k));
        }
        "text" => {
            hash_props(&it.props, &mut h, |k| TEXT_KEPT.contains(&k));
            for g in it.props.get("glyphs").and_then(Value::as_array).into_iter().flatten() {
                for k in ["at", "span", "ch"] {
                    hash_value(g.get(k).unwrap_or(&Value::Null), &mut h);
                }
            }
        }
        "model" => hash_value(it.props.get("src").unwrap_or(&Value::Null), &mut h),
        // a picture's size and cut, a decal's maps and fade (panels_rows, decals_rows)
        "panel" | "decal" => hash_props(&it.props, &mut h, |k| if it.kind == "panel" { ["width", "height", "shadow", "cutout"] } else { ["src", "normalMap", "fade", "unlit"] }.contains(&k)),
        kind => {
            if let Some((_, _, props)) = WHOLE.iter().find(|(k, ..)| *k == kind) {
                hash_props(&it.props, &mut h, |k| props.contains(&k));
            }
        }
    }
    h.finish()
}

/// Tracks cut where a part's kept inputs change: each run of frames with the same spawn_sig is a track of its own
/// (`ID~1`, `ID~2` after the first), so a render's row never holds one frame's shape or words over another frame's.
pub fn split_kept(tracks: Vec<(String, Vec<Option<Item>>)>) -> Vec<(String, Vec<Option<Item>>)> {
    let mut out = Vec::with_capacity(tracks.len());
    for (id, track) in tracks {
        // a part in one frame (every live frame's, a still's) has nothing to cut
        if track.iter().flatten().nth(1).is_none() {
            out.push((id, track));
            continue;
        }
        let n = track.len();
        let mut runs: Vec<(u64, Vec<Option<Item>>)> = Vec::new();
        for (i, slot) in track.into_iter().enumerate() {
            let Some(it) = slot else { continue };
            let s = spawn_sig(&it);
            if runs.last().is_none_or(|(r, _)| *r != s) {
                runs.push((s, vec![None; n]));
            }
            runs.last_mut().expect("a run was just pushed").1[i] = Some(it);
        }
        out.extend(runs.into_iter().enumerate().map(|(k, (_, t))| (if k == 0 { id.clone() } else { format!("{id}~{k}") }, t)));
    }
    out
}

/// The kinds whose rows take what they are from their first frame: the kind, its row's columns that say it, the props
/// they're made from (plates_rows, fogs_rows, fires_rows, grass.rs, splat.rs, meshseq.rs).
const WHOLE: [(&str, &[&str], &[&str]); 6] = [
    ("plate", &["src", "k1"], &["src", "k1"]),
    ("splat", &["src", "morph_to"], &["src", "morphTo"]),
    ("meshseq", &["src", "map"], &["src", "map"]),
    ("fogvolume", &["density_texture"], &["density_texture"]),
    ("fire", &["sim", "seed", "steps"], &["sim", "seed", "steps"]),
    ("grass", &["grass"], &["spec"]),
];

/// A mesh's kept inputs as a live scene's row holds them, so a frame on the fast path can tell when one changed (then
/// the full path, which spawns the part again). Its props are held, not copied: held, they are never changed in place
/// (read3d's props_in_place takes only props nobody else holds), so the same Arc is the same props, and only a new
/// one is hashed.
#[derive(Clone)]
pub struct KeptInputs {
    sig: u64,
    props: Arc<Map<String, Value>>,
    mat: Option<Arc<Map<String, Value>>>,
    shape: Option<Arc<str>>,
}

impl KeptInputs {
    pub fn of(it: &Item) -> KeptInputs {
        KeptInputs { sig: spawn_sig(it), props: it.props.clone(), mat: it.material.as_ref().map(|m| m.props.clone()), shape: it.shape.clone() }
    }

    /// Whether `it` has the kept inputs these were taken from; when it has, its props are held from now on.
    pub fn same(&mut self, it: &Item) -> bool {
        let held = |a: &Option<Arc<Map<String, Value>>>, b: Option<&Arc<Map<String, Value>>>| match (a, b) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if Arc::ptr_eq(&self.props, &it.props) && held(&self.mat, it.material.as_ref().map(|m| &m.props)) && self.shape == it.shape {
            return true;
        }
        if spawn_sig(it) != self.sig {
            return false;
        }
        *self = KeptInputs::of(it);
        true
    }
}
