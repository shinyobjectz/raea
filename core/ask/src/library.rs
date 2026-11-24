//! The questions that come standard (docs/ask.feature AK1), each set named: `rubric` (the craft rubric's own questions,
//! read from sdk/guides/craft/rubric.md, so there's one copy), `av` (picture with sound), `defects` (the fingerprint's
//! axes with no labels yet), `platform` (the check packs' questions with their calibration), `decide` (picks and
//! comparisons) and `make` (the review loop's gate questions, asked of a job's sheets: docs/make.feature MK4; each id
//! says its sheet, make.clay.*, make.colour.*, make.refs.*, make.framing.*). The JSON sets ship inside raea; the
//! rubric is read from the guide the caller hands in.

use crate::question::{Kind, Question};

/// Every set, in the order `raea ask list` shows them.
pub const SETS: [&str; 6] = ["rubric", "av", "defects", "platform", "decide", "make"];

fn embedded(name: &str) -> Option<&'static str> {
    Some(match name {
        "av" => include_str!("../library/av.json"),
        "defects" => include_str!("../library/defects.json"),
        "platform" => include_str!("../library/platform.json"),
        "decide" => include_str!("../library/decide.json"),
        "make" => include_str!("../library/make.json"),
        _ => return None,
    })
}

/// The rubric's questions from its guide: each line under "## The questions" is `- id (modes): the question`. A grade
/// over the five levels, as critique asks it. `silent` is when no one speaks, not a mode; a question with an `ID-silent`
/// twin is asked only when someone speaks, and its twin in its place otherwise.
pub fn rubric(md: &str) -> Vec<Question> {
    let body = md.split("## The questions").nth(1).unwrap_or("");
    let body = body.split("\n## ").next().unwrap_or("");
    let mut out: Vec<Question> = body
        .lines()
        .filter_map(|l| {
            let l = l.strip_prefix("- ")?;
            let (head, ask) = l.split_once("): ")?;
            let (id, modes) = head.split_once(" (")?;
            let mut q = Question::plain(id.trim(), ask.trim());
            q.kind = Kind::Grade;
            q.uses = vec!["brief".into(), "measures".into()];
            for m in modes.split(',').map(str::trim) {
                match m {
                    "all" => {}
                    "silent" => q.when.voice = Some(false),
                    m => q.when.modes.push(m.to_string()),
                }
            }
            Some(q)
        })
        .collect();
    let ids: Vec<String> = out.iter().map(|q| q.id.clone()).collect();
    for q in &mut out {
        let twin = format!("{}-silent", q.id);
        if ids.contains(&twin) {
            q.when.voice = Some(true);
            q.swap = Some(twin);
        }
    }
    out
}

/// A set's questions. `rubric_md` is the rubric guide's text, needed only for `rubric`.
pub fn set(name: &str, rubric_md: Option<&str>) -> Result<Vec<Question>, String> {
    if name == "rubric" {
        let md = rubric_md.ok_or("the rubric set is read from the rubric guide (raea guide rubric), which wasn't found")?;
        return Ok(rubric(md));
    }
    let text = embedded(name).ok_or_else(|| format!("no set named {name:?}: {}", SETS.join(", ")))?;
    serde_json::from_str(text).map_err(|e| format!("the {name} set doesn't read: {e}"))
}

/// Every standard question.
pub fn all(rubric_md: Option<&str>) -> Vec<Question> {
    SETS.iter().filter_map(|s| set(s, rubric_md).ok()).flatten().collect()
}

/// A standard question by id.
pub fn find(id: &str, rubric_md: Option<&str>) -> Option<Question> {
    all(rubric_md).into_iter().find(|q| q.id == id)
}
