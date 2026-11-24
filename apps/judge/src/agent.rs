//! A judge that is an agent CLI rather than clef-omni: `raea judge DIR --by kimi|claude` (Shane, 11-06: "use claude or
//! kimi cli as the judge"). Each call is a new session, run in DIR with only the sheets to look at (kimi's media tool;
//! claude with Read alone), given the fresh-context judge prompt (sdk/guides/craft/bevy/prompts.md section 3) and
//! never the code or the builder's notes. It answers JSON: the seven scores 0-10, the blind read and every defect named;
//! the pass, done and stall are still decided in code (rubric::pass, progress). `raea judge eye DIR --by kimi` records
//! its reading as the eye clef is calibrated against.

use std::path::Path;

use serde_json::{Map, Value, json};

use crate::rubric::{BAR, scores};

pub const BY: [&str; 2] = ["kimi", "claude"];

/// The judge prompt: the sheets by name, the brief's words, and the JSON to answer. A `raea make` job's kind asks its
/// own categories and the bar (docs/make.feature MK5); none asks the rubric raea judge has always asked.
pub fn prompt(kind: Option<&str>, dir: &Path, brief: &Value, target: Option<&Path>) -> String {
    match kind {
        None => prompt_default(dir, brief, target),
        Some(k) => prompt_kind(k, dir, brief, target),
    }
}

fn brief_words(brief: &Value) -> String {
    ["look", "logline", "notDoing"].iter().filter_map(|k| brief.get(*k).map(|v| format!("{k}: {}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))).collect::<Vec<_>>().join("; ")
}

fn prompt_kind(kind: &str, dir: &Path, brief: &Value, target: Option<&Path>) -> String {
    let words = brief_words(brief);
    let target = target.map(|t| format!(" The reference, what it must read as, is {}.", t.display())).unwrap_or_default();
    let d = dir.display();
    let sheets = match kind {
        "shot" => format!("This round's sheet: {d}/framing.png. Each row is one shot: its first, middle and last frame, left to right, each stamped with its name and time. Open it."),
        _ => format!(
            "This round's sheets: {d}/colour.png and {d}/clay.png, each six tiles, left to right, top to bottom: hero, front, right, top, iso, low (0.3 m off the floor). Clay is every surface light grey and matte. Open both{}.",
            if kind == "kit" { "; the scene is the kit's lineup, its props side by side with a scale bar" } else { "" }
        ),
    };
    let blind = if kind == "shot" {
        String::new()
    } else {
        format!("PART 1, BLIND: open {d}/hero-colour.png first, one frame. In one sentence: what is this, what era? List five objects you can name. Is it a photograph or a render, and which three tells decided it?\n\n")
    };
    let cats: String = scores(Some(kind)).iter().map(|(k, on, ask)| format!("- {k}{}: {}\n", if *on == "clay" { " (judged on CLAY)" } else { "" }, ask.split_once(": ").map(|(_, r)| r).unwrap_or(ask))).collect();
    let json_scores = scores(Some(kind)).iter().map(|(k, _, _)| format!("\"{k}\": 0")).collect::<Vec<_>>().join(", ");
    format!(
        "You review a {kind} from images only, as the person who will use it would. You have never seen how it was built and you never will. \
Judge what is in the pixels; do not guess at intentions. Only look at images; change no file.\n\n{blind}\
PART 2, SCORED. The brief: \"{words}\".{target} {sheets}\n\n\
Score each 0-10 (5 = a believable draft; 7 = a production base an artist would accept; 9 = indistinguishable from real). Halves are allowed.\n{cats}\n\
{BAR} Each defect field is one short sentence; \"blocking\" means someone using this would notice it where it will be seen. \
Name exactly what gives it away (not \"the phone looks fake\" but \"the handset is fused to the cradle with no gap\").\n\n\
End your answer with only this JSON, nothing after it:\n\
{{\"blind\": {{\"place\": \"\", \"objects\": [], \"photo\": \"yes|no\", \"tells\": []}}, \
\"scores\": {{{json_scores}}}, \
\"defects\": [{{\"tile\": \"\", \"where\": \"\", \"wrong\": \"\", \"real\": \"\", \"fix\": \"\", \"level\": \"blocking|detail\"}}], \
\"show_as_is\": \"\"}}"
    )
}

fn prompt_default(dir: &Path, brief: &Value, target: Option<&Path>) -> String {
    let words = brief_words(brief);
    let target = target.map(|t| format!(" The target picture, what it must read as, is {}.", t.display())).unwrap_or_default();
    format!(
        "You review a 3D scene from images only, as the person who will use it would. You have never seen how it was built and you never will. \
Judge what is in the pixels; do not guess at intentions. Only look at images; change no file.\n\n\
PART 1, BLIND: open {d}/hero-colour.png first, one frame. In one sentence: what place is this, what time, what era? List five objects you can name. \
Is it a photograph or a render, and which three tells decided it?\n\n\
PART 2, SCORED. The brief: \"{words}\".{target} This round's sheets: {d}/colour.png and {d}/clay.png, each six tiles, left to right, top to bottom: \
hero, front, right, top, iso, low (0.3 m off the floor). Clay is every surface light grey and matte. Open both.\n\n\
Score each 0-10 (5 = a believable draft; 7 = a production base an artist would accept; 9 = indistinguishable from real). Halves are allowed.\n\
- composition: camera, framing, layout, the position and scale of every major object.\n\
- form, judged on CLAY: silhouettes read at a glance; parts have real thickness; nothing blobby, melted or a primitive standing in for a thing; repeated things vary.\n\
- assembly, judged on CLAY first: nothing floats, sinks, pokes through or rests on the wrong thing; joints touch; the low tile shows the feet.\n\
- materials: every surface reads as its real material, with wear where use puts it; nothing a flat default colour.\n\
- light: motivated by the place's own sources; exposure and contrast believable.\n\
- detail: close views hold real geometry or texture, not a smooth stand-in; printed matter reads.\n\
- world: one coherent real place; nothing off-era or invented beyond the brief.\n\n\
Then list the defects, most damaging first: at most 8, each field one short sentence. \"blocking\" means someone using this scene would notice it in the views it will be seen in. \
Name exactly what gives it away (not \"the dryer looks fake\" but \"the dryer door is a flat disc with no rim or glass depth\").\n\n\
End your answer with only this JSON, nothing after it:\n\
{{\"blind\": {{\"place\": \"\", \"objects\": [], \"photo\": \"yes|no\", \"tells\": []}}, \
\"scores\": {{\"composition\": 0, \"form\": 0, \"assembly\": 0, \"materials\": 0, \"light\": 0, \"detail\": 0, \"world\": 0}}, \
\"defects\": [{{\"tile\": \"\", \"where\": \"\", \"wrong\": \"\", \"real\": \"\", \"fix\": \"\", \"level\": \"blocking|detail\"}}], \
\"show_as_is\": \"\"}}",
        d = dir.display()
    )
}

/// The last JSON object in the text that has scores (an agent may think aloud, or draft the JSON twice).
pub fn last_verdict(text: &str) -> Option<Value> {
    let starts: Vec<usize> = text.match_indices('{').map(|(i, _)| i).collect();
    for &s in starts.iter().rev() {
        let mut de = serde_json::Deserializer::from_str(&text[s..]).into_iter::<Value>();
        if let Some(Ok(v)) = de.next()
            && v.get("scores").is_some_and(Value::is_object)
        {
            return Some(v);
        }
    }
    None
}

/// The verdict read into the judge's own shape: scores (0-10, the kind's categories), defects (present, level, named
/// by what is wrong), the blind read's photo (1 a photo, 0 a render) and the agent's own words.
pub fn read(kind: Option<&str>, v: &Value) -> (Map<String, Value>, Vec<Value>, Value) {
    let mut out = Map::new();
    for &(k, _, _) in scores(kind) {
        if let Some(x) = v["scores"][k].as_f64().filter(|x| (0.0..=10.0).contains(x)) {
            out.insert(k.into(), json!(x));
        }
    }
    let defects = v["defects"].as_array().into_iter().flatten().filter(|d| d["wrong"].as_str().is_some_and(|w| !w.trim().is_empty())).map(|d| {
        let level = if d["level"].as_str().unwrap_or("").starts_with("block") { "blocking" } else { "detail" };
        let wrong = d["wrong"].as_str().unwrap_or("");
        json!({"defect": wrong.chars().take(80).collect::<String>(), "present": true, "level": level, "sheet": d["tile"], "asked": wrong,
               "where": d["where"], "real": d["real"], "fix": d["fix"]})
    }).collect();
    let photo = match v["blind"]["photo"].as_str() {
        Some(p) if p.starts_with('y') => json!(1.0),
        Some(p) if p.starts_with('n') => json!(0.0),
        _ => Value::Null,
    };
    (out, defects, photo)
}

/// One fresh judging session by `by` over DIR's sheets: its verdict JSON. Its steps stream (kimi's stream-json), each
/// said on stderr as it comes, so a watcher sees it working through a judgement that can take minutes.
pub fn ask(kind: Option<&str>, by: &str, dir: &Path, brief: &Value, target: Option<&Path>) -> Result<Value, String> {
    use std::io::BufRead as _;
    let dir = dunce::canonicalize(&dir).map_err(|e| format!("judge: {}: {e}", dir.display()))?;
    let p = prompt(kind, &dir, brief, target);
    let mut cmd = match by {
        "kimi" => {
            let mut c = std::process::Command::new("kimi");
            c.args(["-p", &p, "--output-format", "stream-json"]);
            c
        }
        "claude" => {
            let mut c = std::process::Command::new("claude");
            c.args(["-p", &p, "--allowedTools", "Read", "--output-format", "stream-json", "--verbose"]);
            c
        }
        o => return Err(format!("judge: --by {o}: the judges are clef (the default), {}", BY.join(", "))),
    };
    let mut child = cmd.current_dir(&dir).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        .map_err(|e| format!("judge: couldn't run {by}: {e} (is the {by} CLI installed and signed in?)"))?;
    let mut said = Vec::new();
    let mut last = String::new();
    for line in std::io::BufReader::new(child.stdout.take().ok_or("judge: no output pipe")?).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        said.extend(texts(&v));
        let step = match (v["role"].as_str(), v["type"].as_str()) {
            (Some("tool") | Some("user"), _) => "looked at a picture".to_string(),
            (Some("assistant"), _) | (_, Some("assistant")) => texts(&v).last().map_or_else(|| "thinking".into(), |t| t[..t.floor_char_boundary(100)].to_string()),
            _ => continue,
        };
        if step != last {
            eprintln!("judge ({by}): {}", step.replace('\n', " "));
            last = step;
        }
    }
    let out = child.wait_with_output().map_err(|e| format!("judge: {by}: {e}"))?;
    let text = said.join("\n");
    if !out.status.success() && last_verdict(&text).is_none() {
        return Err(format!("judge: {by} failed: {}", String::from_utf8_lossy(&out.stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output")));
    }
    last_verdict(&text).ok_or_else(|| format!("judge: {by} answered no verdict JSON: {}", text.trim().chars().rev().take(300).collect::<String>().chars().rev().collect::<String>()))
}

/// The words in one streamed line: kimi's {role: assistant, content: "..."}; claude's {type: assistant, message:
/// {content: [{type: text, text}]}} and its final {type: result, result}.
fn texts(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if v["role"] == "assistant"
        && let Some(s) = v["content"].as_str()
    {
        out.push(s.to_string());
    }
    for c in v["message"]["content"].as_array().into_iter().flatten() {
        if c["type"] == "text"
            && let Some(s) = c["text"].as_str()
        {
            out.push(s.to_string());
        }
    }
    if v["type"] == "result"
        && let Some(s) = v["result"].as_str()
    {
        out.push(s.to_string());
    }
    out
}
