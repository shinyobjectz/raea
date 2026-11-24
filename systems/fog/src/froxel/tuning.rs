//! What a fog camera tunes beyond its look (docs/nobuild.feature NB1a): <VolumetricFog>'s shadow, heroExact,
//! epipolar, tricubic and solari props, read from the camera row's `vfog_*` columns. The RAEA_FOG_* variables that
//! tuned them before are only overrides now: each wins over its prop on every camera, and raea says so once. The
//! settings each camera draws with go into the run's trace (`fog_settings`), so a render says what it drew with.
use std::collections::BTreeMap;

use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_frame::num;

use super::{FroxelFog, ShadowRule};

type Vals = BTreeMap<String, V>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// what dims a light passing through a volume (`shadow`, "absorb" by default)
    pub shadow: ShadowRule,
    /// the hero lights integrated at every pixel instead of at epipolar samples (`heroExact`, false): slower, for a
    /// reference
    pub hero_exact: bool,
    /// the hero passes read the grid's transmittance tricubic, as the apply pass reads its light (`tricubic`, false)
    pub tricubic: bool,
    pub epi: Epipolar,
    pub solari: SolariFog,
}

/// The hero lights' epipolar samples (`epipolar`): lines `line_gap` pixels apart round each light's rectangle, samples
/// `spacing` apart along each, every `every`th traced and the rest interpolated unless they differ from the traced
/// ones by more than `rel` (relative), `abs` (absolute) or `depth` (relative depth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Epipolar {
    pub line_gap: f32,
    pub spacing: f32,
    pub every: u32,
    pub rel: f32,
    pub abs: f32,
    pub depth: f32,
}

/// The fog's light from Solari's scene under <SolariLighting> (`solari`): on, light candidates a cell, frames its
/// history averages, sky rays a cell, what lights it (the emissive meshes, the sky), the history's filter, and what
/// the sky's guide is made of (1 the cell's own last, 2 half a neighbour's, 3 both; 0 none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolariFog {
    pub on: bool,
    pub candidates: u32,
    pub frames: u32,
    pub sky_rays: u32,
    pub lamps: bool,
    pub sky: bool,
    pub filter: bool,
    pub guide: u32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            shadow: ShadowRule::Absorb,
            hero_exact: false,
            tricubic: false,
            epi: Epipolar { line_gap: 8.0, spacing: 6.0, every: super::views::EPI_EVERY, rel: 0.2, abs: 2.0 / 255.0, depth: 0.02 },
            solari: SolariFog { on: true, candidates: 8, frames: 16, sky_rays: 1, lamps: true, sky: true, filter: true, guide: 3 },
        }
    }
}

fn shadow(s: &str) -> Option<ShadowRule> {
    Some(match s {
        "off" => ShadowRule::Off,
        "absorb" => ShadowRule::Absorb,
        "delta" => ShadowRule::Delta,
        "full" => ShadowRule::Full,
        "bevy" => ShadowRule::Bevy,
        _ => return None,
    })
}

/// a variable's whole number over 0
fn count(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse().ok().filter(|v: &u32| *v > 0)
}

/// a variable's comma-separated numbers, each over its place's prop
fn list<T: std::str::FromStr>(name: &str) -> Vec<Option<T>> {
    std::env::var(name).unwrap_or_default().split(',').map(|x| x.trim().parse().ok()).collect()
}

/// The camera row's tuning, each RAEA_FOG_* variable over its prop.
pub fn tuning(vals: &Vals) -> Tuning {
    let d = Tuning::default();
    let on = |k: &str, d: bool| vals.get(k).map_or(d, V::truthy);
    let whole = |k: &str, d: u32| num(vals, k, d as f32).round().max(0.0) as u32;
    let mut t = Tuning {
        shadow: match vals.get("vfog_shadow") {
            Some(V::S(s)) => shadow(s).unwrap_or(d.shadow),
            _ => d.shadow,
        },
        hero_exact: on("vfog_hero_exact", d.hero_exact),
        tricubic: on("vfog_tricubic", d.tricubic),
        epi: Epipolar {
            line_gap: num(vals, "vfog_epi_line", d.epi.line_gap).max(0.5),
            spacing: num(vals, "vfog_epi_spacing", d.epi.spacing).max(0.5),
            every: whole("vfog_epi_every", d.epi.every).max(1),
            rel: num(vals, "vfog_epi_rel", d.epi.rel),
            abs: num(vals, "vfog_epi_abs", d.epi.abs),
            depth: num(vals, "vfog_epi_depth", d.epi.depth),
        },
        solari: SolariFog {
            on: on("vfog_solari", d.solari.on),
            candidates: whole("vfog_solari_candidates", d.solari.candidates).max(1),
            frames: whole("vfog_solari_frames", d.solari.frames).max(1),
            sky_rays: whole("vfog_solari_sky_rays", d.solari.sky_rays).max(1),
            lamps: on("vfog_solari_lamps", d.solari.lamps),
            sky: on("vfog_solari_sky", d.solari.sky),
            filter: on("vfog_solari_filter", d.solari.filter),
            guide: whole("vfog_solari_guide", d.solari.guide) & 3,
        },
    };
    // the variables, as they read before they were props
    if let Some(r) = std::env::var("RAEA_FOG_SHADOW").ok().as_deref().and_then(shadow) {
        t.shadow = r;
    }
    if let Some(v) = std::env::var("RAEA_FOG_HERO_EXACT").ok() {
        t.hero_exact = v != "0";
    }
    if let Some(v) = std::env::var("RAEA_FOG_TRICUBIC_T").ok() {
        t.tricubic = v != "0";
    }
    // RAEA_FOG_EPI=line,spacing,every,rel,abs,depth: each number given over its prop
    for (i, v) in list::<f32>("RAEA_FOG_EPI").into_iter().enumerate() {
        let Some(v) = v else { continue };
        match i {
            0 => t.epi.line_gap = v.max(0.5),
            1 => t.epi.spacing = v.max(0.5),
            2 => t.epi.every = (v as u32).max(1),
            3 => t.epi.rel = v,
            4 => t.epi.abs = v,
            5 => t.epi.depth = v,
            _ => {}
        }
    }
    if let Some(v) = std::env::var("RAEA_FOG_SOLARI").ok() {
        t.solari.on = v != "0";
    }
    t.solari.candidates = count("RAEA_FOG_SOLARI_CANDIDATES").unwrap_or(t.solari.candidates);
    t.solari.frames = count("RAEA_FOG_SOLARI_FRAMES").unwrap_or(t.solari.frames);
    t.solari.sky_rays = count("RAEA_FOG_SOLARI_SKY").unwrap_or(t.solari.sky_rays);
    if let Some(p) = std::env::var("RAEA_FOG_SOLARI_PARTS").ok() {
        (t.solari.lamps, t.solari.sky) = (p.contains("lamps"), p.contains("sky"));
    }
    if let Some(v) = std::env::var("RAEA_FOG_SOLARI_FILTER").ok() {
        t.solari.filter = v != "0";
    }
    if let Some(g) = std::env::var("RAEA_FOG_SOLARI_GUIDE").ok().and_then(|v| v.parse::<u32>().ok()) {
        t.solari.guide = g & 3;
    }
    t
}

/// RAEA_FOG_GRID=tile,slices,samples over the `tile`, `slices` and `samples` props.
pub fn grid(tile: u32, slices: u32, samples: u32) -> (u32, u32, u32) {
    let g: Vec<Option<u32>> = list("RAEA_FOG_GRID");
    let at = |i: usize, d: u32| g.get(i).copied().flatten().unwrap_or(d);
    (at(0, tile).max(1), at(1, slices).clamp(8, 256), at(2, samples).max(1))
}

/// RAEA_FOG_HERO over the `hero` prop ("0" off, anything else on).
pub fn hero(prop: bool) -> bool {
    std::env::var("RAEA_FOG_HERO").ok().map_or(prop, |v| v != "0")
}

/// The RAEA_FOG_* variables that override a prop, each with the prop.
const VARS: &[(&str, &str)] = &[
        ("RAEA_FOG_MODE", "mode"),
        ("RAEA_FOG_HERO", "hero"),
        ("RAEA_FOG_GRID", "tile, slices, samples"),
        ("RAEA_FOG_SHADOW", "shadow"),
        ("RAEA_FOG_HERO_EXACT", "heroExact"),
        ("RAEA_FOG_EPI", "epipolar"),
        ("RAEA_FOG_TRICUBIC_T", "tricubic"),
        ("RAEA_FOG_SOLARI", "solari"),
        ("RAEA_FOG_SOLARI_CANDIDATES", "solari.candidates"),
        ("RAEA_FOG_SOLARI_FRAMES", "solari.frames"),
        ("RAEA_FOG_SOLARI_SKY", "solari.skyRays"),
        ("RAEA_FOG_SOLARI_PARTS", "solari.lamps and solari.sky"),
        ("RAEA_FOG_SOLARI_FILTER", "solari.filter"),
        ("RAEA_FOG_SOLARI_GUIDE", "solari.guide"),
];

/// A camera's fog as it draws, into the run's trace (`fog_settings`) each time it's put on: the mode, and for the grid
/// every setting; and once a run, on stderr, which variables override <VolumetricFog>'s props.
pub fn record(camera: Entity, mode: &str, fog: Option<&FroxelFog>) {
    let env = raea_io_trace::overrides("VolumetricFog", VARS);
    let mut j = serde_json::json!({ "camera": camera.to_bits(), "mode": mode, "env": env.iter().map(|(n, v, _)| format!("{n}={v}")).collect::<Vec<_>>() });
    if let Some(f) = fog {
        let (t, e, s) = (&f.tune, &f.tune.epi, &f.tune.solari);
        j["grid"] = serde_json::json!({
            "slices": f.slices, "tile": f.tile, "samples": f.samples, "hero": f.hero,
            "shadow": format!("{:?}", t.shadow).to_lowercase(), "hero_exact": t.hero_exact, "tricubic": t.tricubic,
            "epipolar": [e.line_gap, e.spacing, e.every as f32, e.rel, e.abs, e.depth],
            "solari": { "on": s.on, "candidates": s.candidates, "frames": s.frames, "sky_rays": s.sky_rays, "lamps": s.lamps, "sky": s.sky, "filter": s.filter, "guide": s.guide },
        });
    }
    raea_io_trace::event("fog_settings", j);
}
