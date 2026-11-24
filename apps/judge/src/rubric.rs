//! The frozen rubric (the 11-06 prompt pack, section 3, in clef-omni's native question types): a 0-10 score per category
//! as an ordered six-level rubric, and every named defect as a yes/no question, so a defect is a probability rather
//! than prose. clef-omni answers questions, not essays; the defects it can name are the ones asked. Nothing here says
//! where the pass line is (the loop decides, in code: `pass`).

use serde_json::{Value, json};

/// The six levels each category is scored on, lowest first (clef's `score`: the probability-weighted level), read as
/// 0, 2, 4, 6, 8, 10.
fn levels(what: &str) -> Value {
    json!([
        format!("0: no {what} to speak of; broken or missing"),
        format!("2: {what} is crude: primitives and placeholders"),
        format!("4: {what} is a rough draft, plainly unfinished"),
        format!("6: {what} is a believable draft"),
        format!("8: {what} is a production base an artist would accept"),
        format!("10: {what} is indistinguishable from the real thing"),
    ])
}

/// A category: (id, the sheet it is judged on, the question).
pub type Score = (&'static str, &'static str, &'static str);

/// (id, the sheet it is judged on, the question): the rubric raea judge has always asked, when no `--kind` is given.
pub const SCORES: [Score; 7] = [
    ("composition", "colour", "Composition: the camera, framing and layout; every major object's position and scale against the brief's space."),
    ("form", "clay", "Form, judged on these grey clay tiles: silhouettes read at a glance, parts have real thickness, nothing blobby or melted, no primitive standing in for a thing, repeated things vary."),
    ("assembly", "clay", "Assembly, judged on these grey clay tiles: nothing floats, sinks, pokes through or rests on the wrong thing; joints touch; the low tile shows the feet resting."),
    ("materials", "colour", "Materials: every surface reads as its real material with wear where use puts it; nothing a flat default colour; glass is glass."),
    ("light", "colour", "Light: motivated by the place's own sources; exposure and contrast believable; no surface implausibly dark or bright."),
    ("detail", "colour", "Detail: close surfaces hold real geometry or texture at that distance, not smooth stand-ins; printed matter reads."),
    ("world", "colour", "World: one coherent real place matching the brief; nothing off-era, no real brands, nothing invented beyond the brief."),
];

/// The job kinds with a rubric of their own (docs/make.feature MK5): `raea judge DIR --kind scene|kit|shot`.
pub const KINDS: [&str; 3] = ["scene", "kit", "shot"];

/// A scene made to a plan (AnyScene's categories): judged on the six-camera sheets.
pub const SCENE: [Score; 8] = [
    ("layout", "clay", "Layout, judged on these grey clay tiles: every major object where the plan and the reference put it, at its real size, with room to move between them."),
    ("props", "clay", "Props, judged on these grey clay tiles: each object's form reads at a glance, with real thickness; nothing blobby or a primitive standing in; nothing floats, sinks or pokes through."),
    ("signage", "colour", "Screens and signage: every screen, sign, label and printed thing reads, belongs to one invented world, and shows no real brand."),
    ("materials", "colour", "Materials: every surface reads as its real material with wear where use puts it; nothing a flat default colour; glass is glass."),
    ("light", "colour", "Light: motivated by the place's own sources; exposure and contrast believable; emissive things glow without lighting the room wrong."),
    ("atmosphere", "colour", "Atmosphere: the air, haze, dust or weather the place would have, matching the plan's dials; not empty and clinical."),
    ("plausibility", "colour", "Plausibility: one coherent real place a person could stand in; nothing off-era, nothing that couldn't be there."),
    ("composition", "colour", "Composition: the hero camera's framing and the place's layout read as the reference's."),
];

/// A kit (AnyKit's categories): judged on the six-camera sheets of its lineup.
pub const KIT: [Score; 8] = [
    ("cohesion", "colour", "Cohesion: every prop belongs to one set: the same era, palette, materials, wear and maker; none is an outlier."),
    ("fidelity", "colour", "Reference fidelity: the props read as the style references' world, at normal viewing distance."),
    ("brief", "colour", "Brief: the kit holds the props the brief asked for, each recognisable as what it is meant to be."),
    ("mesh", "clay", "Mesh, judged on these grey clay tiles: crisp silhouettes, real thickness, no melted or blobby forms, no holes or spikes."),
    ("texture", "colour", "Texture: real materials with wear where use puts it; no smeared, stretched or flat default surfaces; glass is glass."),
    ("assembly", "clay", "Assembly, judged on these grey clay tiles: every part of each prop is connected to its body; handles, legs and lids attached."),
    ("scale", "clay", "Scale, judged on these grey clay tiles: each prop's size is right beside the others and the scale bar."),
    ("labels", "colour", "Labels: printed text and logos read, belong to the one invented maker, and no real brand shows."),
];

/// A shot list (AnyShot's categories): judged on the framing sheet, each shot's first, middle and last frame.
pub const SHOT: [Score; 6] = [
    ("framing", "colour", "Framing: each shot frames its subject as its description says, at every one of its frames."),
    ("move", "colour", "Move: the camera's change from first to last frame is the move the description names, at a believable size."),
    ("focus", "colour", "Focus: what the description says is sharp is sharp; nothing important lost in blur."),
    ("clearance", "colour", "Clearance: the camera is never inside or against a wall or object; nothing blocks the frame."),
    ("continuity", "colour", "Continuity: the shots read as one place at one time: the same light, weather and dressing across them."),
    ("grade", "colour", "Grade: the look matches the plan's look dial, with believable exposure and colour."),
];

/// The categories asked for a kind: its own rubric, or the default one.
pub fn scores(kind: Option<&str>) -> &'static [Score] {
    match kind {
        Some("scene") => &SCENE,
        Some("kit") => &KIT,
        Some("shot") => &SHOT,
        _ => &SCORES,
    }
}

/// What a kind's critic is told the bar is (AnyKit's own words, docs/make.feature MK5).
pub const BAR: &str = "The bar is a production base an artist then refines, judged at normal viewing distance: take no points off for choices that differ from the reference, don't nitpick, and name at most five fixes, most damaging first.";

/// The sheets a kind is judged on: the six-camera colour and clay sheets, or a shot list's framing sheet alone.
pub fn sheets(kind: Option<&str>) -> &'static [(&'static str, &'static str)] {
    match kind {
        Some("shot") => &[("colour", "framing.png")],
        _ => &[("colour", "colour.png"), ("clay", "clay.png")],
    }
}

/// (id, sheet, the yes/no question, blocking when yes)
pub const DEFECTS: [(&str, &str, &str, bool); 12] = [
    ("floating", "clay", "Does any object float above the surface it should rest on, with a visible gap under it?", true),
    ("intersecting", "clay", "Does any object pass through another object, a wall or the floor?", true),
    ("blobby", "clay", "Is any object a blobby, melted or lumpy shape where the real thing has crisp form?", true),
    ("stand_in", "clay", "Is any object just a plain box, ball or cylinder standing in for a more detailed real thing?", true),
    ("clones", "clay", "Are several copies of one thing identical and evenly spaced in an obviously artificial way?", false),
    ("flat_colour", "colour", "Is any large surface a flat, uniform default colour with no texture, variation or wear?", true),
    ("plastic", "colour", "Do the surfaces look like shiny uniform plastic or CG rather than their real materials?", true),
    ("garbled_text", "colour", "Is any visible text garbled, tiled or unreadable where it should read?", false),
    ("too_dark", "colour", "Is the scene so dark that important objects are hard to see?", true),
    ("too_bright", "colour", "Is the scene washed out, with large areas blown to white?", true),
    ("brands", "colour", "Is a real brand name or logo visible?", true),
    ("not_doing", "colour", "Does the scene show anything the brief says it must not contain?", true),
];

/// The paragraph clef reads before the sheet: what it is looking at, and the brief's words (no code, no log, no line).
pub fn state(sheet: &str, brief: &Value) -> String {
    state_for(None, sheet, brief)
}

/// state, for a kind: a kind's critic also reads what it is looking at in its terms, and the bar.
pub fn state_for(kind: Option<&str>, sheet: &str, brief: &Value) -> String {
    let what = match kind {
        Some("shot") => "The image is a framing sheet: each row is one shot, its first, middle and last frame left to right, each stamped with the shot's name and time. Judge each shot as its description would.".to_string(),
        _ => {
            let tiles = "six tiles of one 3D scene, left to right and top to bottom: hero (the shot's own camera), front, right, top, iso, and low (a camera 0.3 m off the floor)";
            let what = if sheet == "clay" { format!("The image is {tiles}, rendered in clay: every surface plain grey, so only form and assembly show.") } else { format!("The image is {tiles}, in full colour.") };
            let target = if sheet == "colour" { " A second image, when given, is the target picture the scene should read as." } else { "" };
            let kit = if kind == Some("kit") { " The scene is a kit: a lineup of props made to belong together, with a scale bar." } else { "" };
            format!("{what}{target}{kit}")
        }
    };
    // the default rubric's words stay as calibrated: "the scene"
    let it = match kind { None | Some("scene") => "the scene", Some("kit") => "the kit", _ => "the shots" };
    let mut s = format!("{what} Judge only what is in the pixels, as the person who will use {it} would.");
    let look = brief["look"].as_str().unwrap_or("");
    let logline = brief["logline"].as_str().unwrap_or("");
    let not: Vec<&str> = brief["notDoing"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !logline.is_empty() || !look.is_empty() {
        s.push_str(&format!(" The brief: {logline} Look: {look}."));
    }
    if !not.is_empty() {
        let it = it[..1].to_uppercase() + &it[1..];
        s.push_str(&format!(" {it} must not contain: {}.", not.join("; ")));
    }
    if kind.is_some() {
        s.push(' ');
        s.push_str(BAR);
    }
    s
}

/// The questions for one sheet, by id.
pub fn questions(sheet: &str) -> Value {
    questions_for(None, sheet)
}

/// questions, for a kind's rubric.
pub fn questions_for(kind: Option<&str>, sheet: &str) -> Value {
    let mut q = serde_json::Map::new();
    for &(id, on, ask) in scores(kind) {
        if on == sheet {
            q.insert(format!("score_{id}"), json!({"type": "score", "instructions": ask, "criteria": levels(id)}));
        }
    }
    for (id, on, ask, _) in DEFECTS {
        if on == sheet {
            q.insert(format!("defect_{id}"), json!({"type": "noul", "instructions": ask}));
        }
    }
    Value::Object(q)
}

/// The blind read on the hero tile alone: is this a photograph (reported, never gated, until calibrated).
pub fn blind() -> Value {
    json!({"photo": {"type": "noul", "instructions": "Is this a photograph of a real place, rather than a computer render?"}})
}

/// clef's answers read into scores (0-10) and defects (probability of yes, blocking ones flagged).
pub fn read(answers: &Value) -> (serde_json::Map<String, Value>, Vec<Value>) {
    read_for(None, answers)
}

/// read, for a kind's rubric.
pub fn read_for(kind: Option<&str>, answers: &Value) -> (serde_json::Map<String, Value>, Vec<Value>) {
    let mut scores = serde_json::Map::new();
    let mut defects = Vec::new();
    for &(id, _, _) in self::scores(kind) {
        if let Some(a) = answers.get(format!("score_{id}")) {
            let level = a["score"].as_f64().unwrap_or(0.0);
            scores.insert(id.into(), json!((level * 2.0 * 100.0).round() / 100.0));
        }
    }
    for (id, _, ask, blocking) in DEFECTS {
        if let Some(a) = answers.get(format!("defect_{id}")) {
            let p = a["noul"].as_f64().unwrap_or(0.0);
            defects.push(json!({"defect": id, "p": (p * 1000.0).round() / 1000.0, "present": p >= 0.5, "level": if blocking { "blocking" } else { "detail" }, "asked": ask}));
        }
    }
    (scores, defects)
}

/// The loop's pass, in code: no blocking defect present, the lowest score 6 or more and the mean 7 or more (raea's
/// 0-10 scale; the prompt pack's line). Scores mapped by a calibration first, when there is one.
pub fn pass(scores: &serde_json::Map<String, Value>, defects: &[Value]) -> (bool, Vec<String>) {
    pass_for(None, scores, defects)
}

/// pass, for a kind's rubric: every one of its categories scored.
pub fn pass_for(kind: Option<&str>, scores: &serde_json::Map<String, Value>, defects: &[Value]) -> (bool, Vec<String>) {
    let want = self::scores(kind).len();
    let vals: Vec<f64> = scores.values().filter_map(Value::as_f64).collect();
    let mut why = Vec::new();
    let blocking = blocking(defects);
    if !blocking.is_empty() {
        why.push(format!("blocking: {}", blocking.join(", ")));
    }
    let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let mean = if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 };
    if vals.len() < want {
        why.push(format!("only {} of {want} categories scored", vals.len()));
    }
    if min < 6.0 {
        let low: Vec<String> = scores.iter().filter(|(_, v)| v.as_f64().is_some_and(|x| x < 6.0)).map(|(k, _)| k.clone()).collect();
        why.push(format!("below a draft: {}", low.join(", ")));
    }
    if mean < 7.0 {
        why.push("the whole is under a production base".into());
    }
    (why.is_empty(), why)
}

/// The blocking defects present, by name.
pub fn blocking(defects: &[Value]) -> Vec<&str> {
    defects.iter().filter(|d| d["present"] == true && d["level"] == "blocking").filter_map(|d| d["defect"].as_str()).collect()
}
