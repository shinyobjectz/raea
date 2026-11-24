//! A job made to a frozen plan (docs/make.feature MK0): `make` on a project's brief.ts, written with `scene()`, `kit()`
//! or `shot()` from "raea/brief" before anything is built, and read here. One loop, three kinds: a scene (a dressed,
//! lit place), a kit (props that belong together) and a shot (stills or camera moves of a built scene). Every object
//! carries its real size and whether it came from the reference or was invented; every dial its value, where the value
//! came from, and what it meant here. `raea make status` (apps/make) holds the job to it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "MakeKind")]
pub enum Kind {
    /// a dressed, lit place from references or a brief
    Scene,
    /// a set of props in one style, at real size, to drop into any scene
    Kit,
    /// stills or camera moves of a built scene, from words
    Shot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
pub struct Make {
    pub kind: Kind,
    /// the pictures the job answers to (paths beside the brief): what is in it and where for a scene, the style for a kit
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub references: Vec<String>,
    /// one paragraph every reference prompt carries: era, palette, materials, wear, and one fictional maker
    pub style: Option<String>,
    /// by name, from the kind's set (DIALS); one left out is read from the references
    #[serde(default)]
    #[ts(as = "Option<BTreeMap<String, Dial>>", optional)]
    pub dials: BTreeMap<String, Dial>,
    #[serde(default)]
    #[ts(as = "Option<Vec<Object>>", optional)]
    pub objects: Vec<Object>,
    #[serde(default)]
    #[ts(as = "Option<Vec<Shot>>", optional)]
    pub shots: Vec<Shot>,
    /// the compositions the job's checks hold (relative to the brief): each needs a clean render or check newer than it
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub compositions: Vec<String>,
    /// what must exist when it's done, relative to the brief; replaces the kind's own list (`deliverables`)
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub deliverables: Vec<String>,
    /// the most critic rounds the job gets; at the cap it is delivered with what's still missing (8 by default)
    pub rounds: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "MakeSource")]
pub enum Source {
    /// set by the person who asked: an instruction, even when the reference says otherwise
    Customer,
    /// read from the references, because nobody set it
    Reference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "MakeDialValue")]
pub enum DialValue {
    Number(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "MakeDial")]
pub enum Dial {
    /// `wear: 0.4`: the value alone, set by the person
    Bare(DialValue),
    /// `wear: { value: 0.4, source: "reference", means: "scuffed bakelite, worn paint on the edges" }`
    Full {
        value: DialValue,
        #[ts(optional)]
        source: Option<Source>,
        /// what the value meant for this job, in one line
        #[ts(optional)]
        means: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "MakeRoute")]
pub enum Route {
    /// defined by numbers: Manifold solids, raea/room
    Solid,
    /// defined by a rule: fields, growth, instances
    Rule,
    /// defined by its looks: generated (raea gen)
    Gen,
    /// an exact asset that already exists
    Library,
    /// far background: a picture on a plane
    Picture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "MakeOrigin")]
pub enum Origin {
    /// seen in a reference
    Reference,
    /// added to complete the place or the set, in the same world
    Invented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
#[ts(rename = "MakePick")]
pub struct Pick {
    /// "tripo-h31", "meshy", ...
    pub generator: String,
    /// one line: why this one ("sharper dial numerals; Tripo fused the handset to the cradle")
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
#[ts(rename = "MakeObject")]
pub struct Object {
    /// snake_case, unique in the job
    pub id: String,
    pub label: String,
    /// real height in metres
    pub h: Option<f64>,
    /// [width, depth, height] in metres, for boxy things (generators invent depth)
    pub dims: Option<[f64; 3]>,
    /// largest extent in metres, for things that lie flat (tools, papers)
    pub max: Option<f64>,
    pub route: Option<Route>,
    /// one object, its condition and its materials; never a real brand
    pub prompt: Option<String>,
    pub source: Option<Origin>,
    /// where it is in reference 0: [x0, y0, x1, y1] in 0..1
    pub crop: Option<[f64; 4]>,
    /// where it stands in a scene: metres, the floor's front-left corner at the origin, rot 0 = facing the entrance
    pub at: Option<[f64; 3]>,
    pub rot: Option<f64>,
    pub hero: Option<bool>,
    pub pick: Option<Pick>,
    /// what was fixed by script ("glass made transmissive, label kept opaque")
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub fixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(optional_fields)]
#[ts(rename = "MakeShot")]
pub struct Shot {
    /// [a-z0-9_]: the file it lands as (out/shots/<name>.png or .mp4)
    pub name: String,
    /// in a crew's words: "slow push along the counter, rack focus to the register"
    pub description: Option<String>,
    /// static, dolly, truck, pedestal, crane, pan, tilt, orbit, dollyZoom, path
    #[serde(rename = "move")]
    #[ts(rename = "move")]
    pub move_: Option<String>,
    pub seconds: Option<f64>,
    pub lens: Option<f64>,
    /// none, subtle, walk
    pub handheld: Option<String>,
    /// a still rather than a move
    pub still: Option<bool>,
    /// the composition that renders it (relative to the brief): `raea make framing` and `lenschart` draw it
    pub composition: Option<String>,
    /// its frame rate, for the frames `seconds` spans (30 when left out)
    pub fps: Option<f64>,
}

/// Each kind's dials: what may be set, and what an unset one is read from the references as.
pub fn dials(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Scene => &["density", "tidiness", "wear", "wetness", "time_of_day", "weather", "atmosphere", "power", "warmth", "look"],
        Kind::Kit => &["count", "variety", "detail", "wear", "wetness", "warmth", "look"],
        Kind::Shot => &["res", "seconds", "lens", "handheld", "look"],
    }
}

/// The round cap when the plan names none: AnyKit's and raea judge's 8.
pub const ROUNDS: u32 = 8;

/// What must exist when the job is done, relative to the brief's folder: the plan's own list, else the kind's.
pub fn deliverables(m: &Make) -> Vec<String> {
    if !m.deliverables.is_empty() {
        return m.deliverables.clone();
    }
    match m.kind {
        // the stills are counted (three or more), not named: apps/make checks the folder
        Kind::Scene => vec!["out/stills/".into()],
        Kind::Kit => {
            let mut v: Vec<String> = m
                .objects
                .iter()
                .flat_map(|o| [format!("out/kit/{0}/{0}.glb", o.id), format!("out/kit/{0}/{0}_lod1.glb", o.id), format!("out/kit/thumbs/{}.png", o.id)])
                .collect();
            // the reference sheet every reference was checked on before any 3D (MK10)
            v.extend(["out/kit/lineup.png".into(), "out/kit/kit.json".into(), "review/refs.png".into()]);
            v
        }
        // a shot is a still or a move, so either file answers; apps/make reads `a|b`
        Kind::Shot => m.shots.iter().map(|s| format!("out/shots/{0}.png|out/shots/{0}.mp4", s.name)).collect(),
    }
}

/// What the plan still lacks before it can be built against, each with what to do. Empty when it's ready.
pub fn doctor(m: &Make) -> Vec<String> {
    let mut out = Vec::new();
    let allowed = dials(m.kind);
    for name in m.dials.keys().filter(|k| !allowed.contains(&k.as_str())) {
        out.push(format!("dial `{name}` isn't one of a {:?}'s ({}): drop it or say it in `style`", m.kind, allowed.join(", ")));
    }
    match m.kind {
        Kind::Shot if m.shots.is_empty() => out.push("no shots: list each with a name and a description".into()),
        Kind::Scene | Kind::Kit if m.objects.is_empty() => out.push("no objects: list every object with its real size and route".into()),
        _ => {}
    }
    if m.kind == Kind::Kit && m.style.is_none() {
        out.push("no `style`: one paragraph (era, palette, materials, wear, one fictional maker) every reference carries".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for o in &m.objects {
        if !seen.insert(o.id.as_str()) {
            out.push(format!("object `{}` is listed twice", o.id));
        }
        if o.h.is_none() && o.dims.is_none() && o.max.is_none() {
            out.push(format!("object `{}` has no size: give `h` (metres), `dims` for boxy things or `max` for flat ones", o.id));
        }
        if o.route.is_none() && m.kind == Kind::Scene {
            out.push(format!("object `{}` has no route: solid, rule, gen, library or picture", o.id));
        }
        if matches!(o.route, Some(Route::Gen)) || m.kind == Kind::Kit {
            if o.prompt.is_none() {
                out.push(format!("object `{}` is generated but has no prompt: one object, its condition and materials", o.id));
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for s in &m.shots {
        if !names.insert(s.name.as_str()) {
            out.push(format!("shot `{}` is listed twice", s.name));
        }
        if !s.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            out.push(format!("shot `{}`: names are [a-z0-9_], the file it lands as", s.name));
        }
        if s.composition.is_none() {
            out.push(format!("shot `{}` has no composition: the .tsx that renders it, for its framing sheet and lens chart", s.name));
        }
    }
    out
}
