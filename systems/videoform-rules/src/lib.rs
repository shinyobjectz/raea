//! The rules (docs/videoform.feature VF8): what holds across the references, from their kept rows, each with how
//! strongly it holds (the share of the references it holds in, of those it could), written as rule fields of a
//! controller, NAME.rules.ts, that `videoform.rules` names; the checks they name are raea check's (VF10). The sorts:
//!   - covers: every video has a kind of shot, or shows (says, has heard) a term
//!   - opens and closes: its first shot and its last
//!   - slot: what a section is made of, more than the rest of the video is; order: which section comes first
//!   - together: where a place is shown, a thing of its is shown with it (the kitchen's counters)
//!   - event: when a word is said or shown, what the shot or the next does (punches in, puts words on screen), where
//!     that's more than chance
//! The person confirms the strongest on a contact sheet of the shot kinds' stills, and each answer is kept as a
//! correction by the rule's id: keep (an error to break), free (a note) or ignore (dropped). Its own crate beside
//! raea-sys-videoform (11-21), which was full with VF8b and VF8c to come.

use raea_sys_videoform::kinds::Kinds;
use raea_sys_videoform::rows::Rows;
use raea_sys_videoform::vocab::Report;
use raea_core_brief::videoform::Correction;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// A rule holds in this share of the references it could hold in, or more
pub const RULE: f64 = 0.7;
/// and it could hold in this many or more
const ENOUGH: usize = 3;
/// Strong: in this share or more, with five references or more (under five nothing gates, VF8b)
const STRONG: f64 = 0.85;
const FEW: usize = 5;
/// What a section or an event brings comes this many times as often there as elsewhere, or it's chance
const LIFT: f64 = 1.5;
/// The first tenth of a video: what's there is its opening
const OPENING: f64 = 0.1;
/// The most event rules kept, the strongest
const EVENTS: usize = 12;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// how the person names it: raea videoform set NAME ID keep|free|ignore
    pub id: String,
    /// covers, opens, closes, slot, order, together, event
    pub sort: String,
    pub text: String,
    /// the check that holds it (raea check's, VF10), and what it's held to
    pub check: String,
    pub values: Map<String, Value>,
    /// the references it holds in, and those it could
    pub holds: Vec<usize>,
    pub of: Vec<usize>,
    /// how many times as often as elsewhere (slot and event rules)
    pub lift: Option<f64>,
    /// warn (strong), note, or error once the person keeps it
    pub level: String,
}

impl Rule {
    pub fn strength(&self) -> f64 {
        self.holds.len() as f64 / self.of.len().max(1) as f64
    }
}

/// Something a shot has: a term or pattern firing, a kind, a framing, words on screen
struct Feat {
    id: String,
    words: String,
    /// shows, has (heard), says, or "on screen"
    how: &'static str,
    values: Map<String, Value>,
}

fn slug(s: &str) -> String {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-")
}

/// A section's name as a sentence starts it: "the hook" as it is, "call to action" as "the call to action"
fn the(name: &str) -> String {
    if name.to_lowercase().starts_with("the ") { name.to_string() } else { format!("the {name}") }
}

/// What a lead ("every video", "the hook") does with a feature, as a sentence
fn says(lead: &str, f: &Feat) -> String {
    let s = if f.how == "on screen" { format!("{lead} shows {} on screen", f.words) } else { format!("{lead} {} {}", f.how, f.words) };
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn tenth(x: f64) -> f64 {
    (x.min(99.0) * 10.0).round() / 10.0
}

fn map(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// The shots a feature is on, in one reference
fn with(shots: &[Vec<bool>], f: usize) -> Vec<usize> {
    (0..shots.len()).filter(|&s| shots[s][f]).collect()
}

/// How tight a framing is: wide, medium, close-up (no one in it: none)
fn tight(f: &str) -> Option<u8> {
    ["wide", "medium", "close-up"].iter().position(|x| *x == f).map(|i| i as u8)
}

/// What's looked for on each shot, and where each is: [ref][shot][feature]
fn features(refs: &[Rows], fired: &[Vec<Vec<bool>>], rep: &Report, kinds: &Kinds) -> (Vec<Feat>, Vec<Vec<Vec<bool>>>) {
    let mut feats = Vec::new();
    let mut col: Vec<Box<dyn Fn(usize, usize) -> bool + '_>> = Vec::new();
    for (t, term) in rep.terms.iter().enumerate() {
        if term.name.starts_with("(ignore)") || term.verdict != "fires" {
            continue;
        }
        let how = match term.kind.as_str() {
            "sound" => "has",
            "said" => "says",
            "on screen" => "on screen",
            _ => "shows",
        };
        // a pattern for words on screen is often named for it ("price on screen")
        let words = if how == "on screen" { term.name.trim_end_matches(" on screen").to_string() } else { term.name.clone() };
        feats.push(Feat { id: slug(&term.name), words, how, values: map(json!({"term": term.name})) });
        col.push(Box::new(move |r, s| fired[r][s][t]));
    }
    for (k, kind) in kinds.kinds.iter().enumerate() {
        feats.push(Feat { id: format!("kind-{k}"), words: format!("the kind \"{}\"", kind.name), how: "shows", values: map(json!({"kind": k, "name": kind.name})) });
        col.push(Box::new(move |r, s| kinds.of[r][s] == Some(k)));
    }
    for f in ["wide", "close-up"] {
        feats.push(Feat { id: f.into(), words: format!("a {f} shot"), how: "shows", values: map(json!({"framing": f})) });
        col.push(Box::new(move |r, s| refs[r].shots[s].framing == f));
    }
    feats.push(Feat { id: "words-on-screen".into(), words: "words on screen".into(), how: "shows", values: map(json!({"text": true})) });
    col.push(Box::new(|r, s| !refs[r].shots[s].text.is_empty()));
    let on = refs.iter().enumerate().map(|(r, rows)| (0..rows.shots.len()).map(|s| col.iter().map(|c| c(r, s)).collect()).collect()).collect();
    (feats, on)
}

/// What a shot can do after a trigger: punch in (tighter than the shot before), or have a feature
fn punched(rows: &Rows, s: usize) -> bool {
    s > 0 && matches!((tight(&rows.shots[s].framing), tight(&rows.shots[s - 1].framing)), (Some(a), Some(b)) if a > b)
}

/// The rules the references hold, strongest first, with the person's word on each (`ignore` leaves one out)
pub fn rules(refs: &[Rows], fired: &[Vec<Vec<bool>>], rep: &Report, places: &[(String, Vec<String>)], said: &[Correction]) -> Vec<Rule> {
    let n = refs.len();
    let kinds = &rep.kinds;
    let (feats, on) = features(refs, fired, rep, kinds);
    let on = &on;
    let all: Vec<usize> = (0..n).collect();
    let mut out = Vec::new();
    let mut push = |sort: &str, id: String, text: String, check: &str, values: Map<String, Value>, holds: Vec<usize>, of: Vec<usize>, lift: Option<f64>| {
        let r = Rule { id: format!("{sort}:{id}"), sort: sort.into(), text, check: check.into(), values, holds, of, lift, level: String::new() };
        if r.of.len() >= ENOUGH && r.strength() >= RULE {
            out.push(r);
        }
    };
    let holds_where = |test: &dyn Fn(usize) -> bool| -> Vec<usize> { all.iter().copied().filter(|&r| test(r)).collect() };
    for (f, ft) in feats.iter().enumerate() {
        // words on screen and framings are in any video: only how it opens and closes says something of them
        let at: Vec<Vec<usize>> = on.iter().map(|shots| with(shots, f)).collect();
        if ft.values.contains_key("term") || ft.values.contains_key("kind") {
            push("covers", ft.id.clone(), says("every video", ft), "videoform-covers", ft.values.clone(), holds_where(&|r| !at[r].is_empty()), all.clone(), None);
        }
        push("opens", ft.id.clone(), says("its first shot", ft), "videoform-opens", ft.values.clone(), holds_where(&|r| at[r].first() == Some(&0)), all.clone(), None);
        push("closes", ft.id.clone(), says("its last shot", ft), "videoform-closes", ft.values.clone(), holds_where(&|r| at[r].last() == Some(&(on[r].len() - 1))), all.clone(), None);
    }
    // what each section is made of, more than the rest of the video is
    let spans = raea_sys_videoform::structure::spans(refs, &rep.sections);
    for (i, sec) in rep.sections.iter().enumerate() {
        let of: Vec<usize> = all.iter().copied().filter(|&r| spans[r][i].is_some()).collect();
        for (f, ft) in feats.iter().enumerate() {
            // the terms it's found by are in it by its making
            if let Some(slot) = rep.structure.slots.get(i)
                && slot.shows.contains(&ft.words)
            {
                continue;
            }
            // [in the section, elsewhere]: shots with it, and shots
            let mut c = [[0usize; 2]; 2];
            let holds: Vec<usize> = of.iter().copied().filter(|&r| {
                let (s, e) = spans[r][i].unwrap();
                for (j, x) in on[r].iter().enumerate() {
                    let k = usize::from(!(s..e).contains(&j));
                    c[k] = [c[k][0] + usize::from(x[f]), c[k][1] + 1];
                }
                (s..e).any(|j| on[r][j][f])
            }).collect();
            let share = |c: [usize; 2]| c[0] as f64 / c[1].max(1) as f64;
            let lift = share(c[0]) / share(c[1]).max(1e-6);
            // a section that is the whole video has no elsewhere to be more than
            if c[1][1] > 0 && lift >= LIFT {
                let mut values = ft.values.clone();
                values.insert("slot".into(), sec.name.clone().into());
                push("slot", format!("{}:{}", slug(&sec.name), ft.id), says(&the(&sec.name), ft), "videoform-slot", values, holds, of.clone(), Some(tenth(lift)));
            }
        }
    }
    for w in rep.sections.windows(2) {
        let of: Vec<usize> = all.iter().copied().filter(|&r| w[0].found[r].is_some() && w[1].found[r].is_some()).collect();
        let holds = of.iter().copied().filter(|&r| w[0].found[r] < w[1].found[r]).collect();
        let values = map(json!({"before": w[0].name, "after": w[1].name}));
        push("order", format!("{}:{}", slug(&w[0].name), slug(&w[1].name)), says(&the(&w[0].name), &Feat { id: String::new(), words: the(&w[1].name), how: "comes before", values: Map::new() }), "videoform-order", values, holds, of, None);
    }
    // where a place is shown, its things with it: in every run of its shots, a shot of the thing within one of it
    let col: std::collections::BTreeMap<&str, usize> = feats.iter().map(|ft| ft.words.as_str()).zip(0..).collect();
    for (place, things) in places {
        let Some(&p) = col.get(place.as_str()) else { continue };
        let of: Vec<usize> = all.iter().copied().filter(|&r| on[r].iter().any(|s| s[p])).collect();
        for thing in things {
            let Some(&t) = col.get(thing.as_str()) else { continue };
            let holds = of.iter().copied().filter(|&r| {
                let shots = &on[r];
                let near = |j: usize| (j.saturating_sub(1)..=(j + 1).min(shots.len() - 1)).any(|x| shots[x][t]);
                // each run of the place's shots, a gap of one shot allowed
                let mut runs: Vec<Vec<usize>> = Vec::new();
                for j in (0..shots.len()).filter(|&j| shots[j][p]) {
                    match runs.last_mut() {
                        Some(run) if j <= run.last().unwrap() + 2 => run.push(j),
                        _ => runs.push(vec![j]),
                    }
                }
                runs.iter().all(|run| run.iter().any(|&j| near(j)))
            }).collect::<Vec<_>>();
            let close = holds.iter().flat_map(|&r| with(&on[r], t).into_iter().map(move |j| refs[r].shots[j].framing == "close-up")).collect::<Vec<_>>();
            let close_up = close.iter().filter(|c| **c).count() * 2 > close.len();
            let text = format!("Wherever it shows {place}, it shows {thing}{} with it", if close_up { " close up" } else { "" });
            let values = map(json!({"place": place, "thing": thing, "within": 1, "close_up": close_up}));
            push("together", format!("{}:{}", slug(place), slug(thing)), text, "videoform-together", values, holds, of.clone(), None);
        }
    }
    // when a word is said, on screen or heard, what the shot or the next does, more than after any shot
    let mut events = Vec::new();
    let triggers = feats.iter().enumerate().filter(|(_, f)| f.values.contains_key("term") && f.how != "shows");
    for (t, tf) in triggers {
        // what's shown at the very start (the title card's words) is how it opens, said by the opens rules
        let at: Vec<(usize, usize)> = all.iter().flat_map(|&r| with(&on[r], t).into_iter().map(move |s| (r, s))).collect();
        if at.iter().filter(|&&(r, s)| refs[r].shots[s].start < OPENING * refs[r].shots.last().map_or(0.0, |l| l.end)).count() as f64 >= at.len() as f64 * RULE {
            continue;
        }
        // words on screen follow words on screen by their making
        let shown = tf.how == "on screen";
        let responses = feats.iter().enumerate().filter(|(f, ft)| *f != t && (ft.how == "shows" || ft.how == "on screen") && !(shown && (ft.how == "on screen" || ft.values.contains_key("text")))).map(|(f, ft)| (Some(f), ft.id.clone(), ft.words.clone()));
        for (f, id, words) in responses.chain([(None, "punch-in".to_string(), "a punch-in".to_string())]) {
            let does = |r: usize, s: usize| (s..(s + 2).min(on[r].len())).any(|j| f.map_or(punched(&refs[r], j), |f| on[r][j][f]));
            let (mut after, mut base) = ([0usize; 2], [0usize; 2]);
            let mut of = Vec::new();
            let holds: Vec<usize> = all.iter().copied().filter(|&r| {
                let at = with(&on[r], t);
                (0..on[r].len()).for_each(|s| {
                    base[0] += usize::from(does(r, s));
                    base[1] += 1;
                });
                if at.is_empty() {
                    return false;
                }
                of.push(r);
                let hit = at.iter().filter(|&&s| does(r, s)).count();
                after[0] += hit;
                after[1] += at.len();
                hit * 2 >= at.len()
            }).collect();
            let lift = (after[0] as f64 / after[1].max(1) as f64) / (base[0] as f64 / base[1].max(1) as f64).max(1e-6);
            if lift >= LIFT && of.len() >= ENOUGH && holds.len() as f64 >= of.len() as f64 * RULE {
                let when = match tf.how { "says" => format!("{} is said", tf.words), "has" => format!("{} is heard", tf.words), _ => format!("{} is on screen", tf.words) };
                let values = map(json!({"when": tf.values["term"], "then": id, "within": 1}));
                let text = format!("When {when}, the shot or the next {}", if f.is_none() { "punches in".to_string() } else { format!("shows {words}") });
                events.push(Rule { id: format!("event:{}:{id}", tf.id), sort: "event".into(), text, check: "videoform-event".into(), values, holds, of, lift: Some(tenth(lift)), level: String::new() });
            }
        }
    }
    events.sort_by(|a, b| b.strength().total_cmp(&a.strength()).then(b.lift.unwrap_or(0.0).total_cmp(&a.lift.unwrap_or(0.0))));
    out.extend(events.into_iter().take(EVENTS));
    // the person's word, then the strength
    out.retain(|r| said.iter().all(|c| c.measure != r.id || c.say != "ignore"));
    for r in &mut out {
        let word = said.iter().find(|c| c.measure == r.id).map(|c| c.say.as_str());
        r.level = match word {
            Some("keep") => "error",
            Some("free") => "note",
            _ if r.strength() >= STRONG && n >= FEW => "warn",
            _ => "note",
        }.into();
    }
    out.sort_by(|a, b| b.strength().total_cmp(&a.strength()).then(b.of.len().cmp(&a.of.len())));
    out
}

/// The sorts' groups in NAME.rules.ts, in order
const GROUPS: [(&str, &[&str]); 5] = [
    ("What every video has", &["covers"]),
    ("How it opens and closes", &["opens", "closes"]),
    ("Section by section", &["slot", "order"]),
    ("What's shown together", &["together"]),
    ("What follows what", &["event"]),
];

/// The rules as NAME.rules.ts: a controller whose fields are rules (docs/direction.feature DR4), each with how
/// strongly it holds as its help
pub fn rules_ts(rules: &[Rule], name: &str, n: usize) -> String {
    // a field's id, from its rule's: a controller's are at most 48 of a-z, 0-9 and -, and two are never alike
    let mut ids: Vec<String> = Vec::new();
    for r in rules {
        let base: String = slug(&r.id).chars().take(44).collect::<String>().trim_end_matches('-').to_string();
        let (mut id, mut k) = (base.clone(), 1);
        while ids.contains(&id) {
            k += 1;
            id = format!("{base}-{k}");
        }
        ids.push(id);
    }
    let groups: Vec<Value> = GROUPS.iter().filter_map(|(title, sorts)| {
        let fields: Vec<Value> = rules.iter().zip(&ids).filter(|(r, _)| sorts.contains(&r.sort.as_str())).map(|(r, id)| {
            let lift = r.lift.map(|l| format!(", {l} times as often as elsewhere")).unwrap_or_default();
            let help = format!("holds in {} of the {} references it could ({n} in all){lift}; {}", r.holds.len(), r.of.len(), r.id);
            json!({"id": id, "label": r.text, "help": help, "type": "rule", "check": r.check, "values": r.values, "level": r.level})
        }).collect();
        (!fields.is_empty()).then(|| json!({"title": title, "fields": fields}))
    }).collect();
    let c = json!({"title": format!("{name}'s rules"), "help": format!("Mined from {n} references by raea videoform (docs/videoform.feature VF8): confirm or drop each with raea videoform set {name} ID keep|free|ignore"), "groups": groups});
    format!(
        "// The videoform's rules (docs/videoform.feature VF8), mined from its references by raea videoform and made again on\n// every add and report: your word on each is kept in fingerprint.ts (raea videoform set NAME ID keep|free|ignore).\nimport {{ controller }} from \"raea/brief\";\n\nexport default controller({});\n",
        serde_json::to_string_pretty(&c).unwrap()
    )
}

/// The rules, as the person reads them: strongest first, each with its id and where it doesn't hold
pub fn lines(rules: &[Rule], names: &[&str], name: &str) -> Vec<String> {
    let n = names.len();
    let mut out = vec![format!("Rules, strongest first ({}; your word on one: raea videoform set {name} ID keep|free|ignore):", if n < FEW { "under five references, none is strong enough to gate on" } else { "strong ones warn, kept ones are errors" })];
    for r in rules {
        let not: Vec<&str> = r.of.iter().filter(|x| !r.holds.contains(x)).map(|&x| names[x]).collect();
        let not = if not.is_empty() { String::new() } else { format!("; not in {}", not.join(", ")) };
        let lift = r.lift.map(|l| format!(", {l}x")).unwrap_or_default();
        out.push(format!("  {:<5} {} ({} of {}{lift}{not})  {}", r.level, r.text, r.holds.len(), r.of.len(), r.id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use raea_sys_videoform::kinds::Kind;
    use raea_sys_videoform::rows::Shot;
    use raea_sys_videoform::vocab::{SectionFound, TermFound};

    #[test]
    fn what_holds_in_most_references_is_a_rule_with_its_strength() {
        // five references: a price on screen on the first shot of each; the kitchen (term 1) shown in 4, its counters
        // (term 2) beside it in 3 of those; "price" said (term 3) followed by a punch-in in each
        let shot = |at: f64, framing: &str, text: bool| Shot { start: at, end: at + 1.0, framing: framing.into(), text: if text { vec![("$1".into(), [0.0; 4])] } else { vec![] }, ..Default::default() };
        let refs: Vec<Rows> = (0..5).map(|_| Rows { shots: vec![shot(0.0, "wide", true), shot(1.0, "wide", false), shot(2.0, "close-up", false), shot(3.0, "medium", false)], ..Default::default() }).collect();
        let fired: Vec<Vec<Vec<bool>>> = (0..5).map(|r| vec![
            vec![true, false, false, false],
            vec![false, r < 4, false, true],
            vec![false, false, r < 3, false],
            vec![false, false, false, false],
        ]).collect();
        let term = |name: &str, kind: &str| TermFound { name: name.into(), kind: kind.into(), verdict: "fires".into(), ..Default::default() };
        let rep = Report {
            terms: vec![term("price", "on screen"), term("the kitchen", "picture"), term("the counters", "picture"), term("a number", "said")],
            sections: vec![SectionFound { name: "the open".into(), found: vec![Some(0.0); 5], hits: vec![vec![0]; 5], ..Default::default() }],
            kinds: Kinds { kinds: vec![Kind { name: "a room".into(), ..Default::default() }], of: vec![vec![Some(0); 4]; 5] },
            ..Default::default()
        };
        let got = rules(&refs, &fired, &rep, &[("the kitchen".into(), vec!["the counters".into()])], &[]);
        let find = |id: &str| got.iter().find(|r| r.id == id);
        let opens = find("opens:price").expect("opens on the price");
        assert_eq!((opens.holds.len(), opens.of.len(), opens.level.as_str()), (5, 5, "warn"));
        assert_eq!(find("covers:the-kitchen").map(|r| r.holds.len()), Some(4), "the kitchen in 4 of 5, past 0.7");
        let with = find("together:the-kitchen:the-counters").expect("the counters with the kitchen");
        assert_eq!((with.holds.len(), with.of.len(), with.level.as_str()), (3, 4, "note"));
        assert!(with.text.ends_with("close up with it"), "{}", with.text);
        assert!(find("event:a-number:punch-in").is_some(), "a punch-in after the number, every time: {:?}", got.iter().map(|r| &r.id).collect::<Vec<_>>());
        assert!(got.iter().all(|r| !r.id.starts_with("event:price")), "the price is the opening's, said by opens:price");
        assert!(find("covers:kind-0").is_some() && find("covers:words-on-screen").is_none(), "words on screen are in any video");
        // the person's word: ignored is dropped, kept is an error
        let c = |m: &str, say: &str| Correction { measure: m.into(), say: say.into(), ..Default::default() };
        let got = rules(&refs, &fired, &rep, &[], &[c("opens:price", "keep"), c("covers:the-kitchen", "ignore")]);
        assert_eq!(got.iter().find(|r| r.id == "opens:price").map(|r| r.level.as_str()), Some("error"));
        assert!(got.iter().all(|r| r.id != "covers:the-kitchen"));
        let ts = rules_ts(&got, "homes", 5);
        assert!(ts.contains("\"check\": \"videoform-opens\"") && ts.contains("\"type\": \"rule\""), "{ts}");
    }
}
