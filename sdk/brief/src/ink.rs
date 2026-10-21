//! Ink as the brief's backbone (the founder, 2026-10-17: "lets do it with ink"): a story written in inkle's ink, compiled
//! and run by blade-ink (Rust; inkjs in TypeScript), lowered to .brief text and read by the .brief parser, so every
//! reading and refusal .brief has holds for ink, and the graph the compiler reads is the same.
//!
//! The raea profile of ink (format.md, "Ink"):
//!   # title: ... / # format: ... / # uses: ... / # check: RULE                            the story's global tags
//!   # music: "prompt" / # music-gain: -24 / # sfx-gain / # platform / # background / # eyes  :root's stylesheet
//!   # captions-place: low / # captions-keys: a, b                                          captions' stylesheet
//!   # angle: / # hook: / # mode: / # refuses:                                              the strategy's lines
//!   === knot ===                     a section, named for the knot (or its `#section: name`); `#synopsis: ...` says what it does
//!   SHAWN: the words, quoted         a line: the speaker, then words said in a take; `SHAWN (warm): ...` gives delivery
//!   #take:TAKE01                     the take the lines from here on come from
//!   #part:lockup text:"..." | ...    a part on the line, in the .brief part notation; `#punch:1.15` zooms the line
//!   #only:1:1                        the line belongs to that deliverable only
//!   >TEXT<                           text on screen with no line under it
//!   >> product: what is shown #find:hands still            an open beat, a line of its own (#take, #only as lines have them)
//!   ~ beat("product", "what is shown", "hands still")       the same from story logic (external function; played between lines)
//!   ~ check("never says \"game changer\"")                    a check, wherever it reads best
//!   * [hook A] / * [hook B]          choices: `--path 0,1` picks each one; the first is played when none is given
//! Ink's VAR, LIST, conditionals and text interpolation work as ink has them: the lowering sees what the story plays.

use crate::Refusal;
use bladeink::story::Story;
use bladeink::value_type::ValueType;
use std::cell::RefCell;
use std::rc::Rc;

/// the external functions the profile gives a story; each becomes .brief text in the order it's played
const PRELUDE: &str = "EXTERNAL beat(kind, phrase, find)\nEXTERNAL check(rule)\n";

/// what a tag can't hold as itself in ink (`#` ends it, `{}` are logic), as the converter writes it and back
pub fn untag(t: &str) -> String {
    t.replace("&num;", "#").replace("&lbrace;", "{").replace("&rbrace;", "}")
}

fn entag(t: &str) -> String {
    t.replace('#', "&num;").replace('{', "&lbrace;").replace('}', "&rbrace;")
}

fn s(v: &ValueType) -> String {
    match v {
        ValueType::String(x) => x.string.clone(),
        ValueType::Int(i) => i.to_string(),
        ValueType::Float(f) => f.to_string(),
        ValueType::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// an ink story as .brief text along `path` (choice indices, the first choice where the path runs out); each brief line
/// carries the ink line it came from (0 where it came from a tag or a function), for refusals
pub fn lower(source: &str, path: &[usize]) -> Result<(String, Vec<usize>), Vec<Refusal>> {
    let prelude_lines = PRELUDE.lines().count();
    // visit counts on every knot: a line's section is the knot whose count went up as it played (the story's path, read
    // after a line, already points past it, and inside a choice it names the choice, not the knot it diverts to)
    let opts = bladeink_compiler::CompilerOptions { count_all_visits: true, ..Default::default() };
    let knots: Vec<String> = regex::Regex::new(r"(?m)^\s*={2,}\s*(\w+)").unwrap().captures_iter(source).map(|c| c[1].to_string()).filter(|k| k != "function").collect();
    let json = bladeink_compiler::Compiler::with_options(opts).compile(&format!("{PRELUDE}{source}")).map_err(|e| {
        let msg = e.to_string();
        // ink says `<source>:LINE:`; the profile's prelude moved every line down
        let at = regex::Regex::new(r"<source>:(\d+):").unwrap();
        let line = at.captures(&msg).and_then(|c| c[1].parse::<usize>().ok()).map(|l| l.saturating_sub(prelude_lines)).unwrap_or(0);
        vec![Refusal { line, col: 1, msg: format!("ink: {}", at.replace(&msg, "").trim()) }]
    })?;
    let mut story = Story::new(&json).map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("ink: {e}") }])?;
    // what the functions are called with, in order, between the lines the story plays
    let calls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let c1 = calls.clone();
    story.bind_external_function("beat", move |_, args: &[ValueType]| {
        let (kind, phrase, find) = (args.first().map(s).unwrap_or_default(), args.get(1).map(s).unwrap_or_default(), args.get(2).map(s).unwrap_or_default());
        c1.borrow_mut().push(if find.trim().is_empty() { format!("~ {kind}: {phrase}") } else { format!("~ {kind}: {phrase} [find: {find}]") });
        Ok(None)
    }, false).map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("ink: {e}") }])?;
    let c2 = calls.clone();
    story.bind_external_function("check", move |_, args: &[ValueType]| {
        c2.borrow_mut().push(format!("! {}", args.first().map(s).unwrap_or_default()));
        Ok(None)
    }, false).map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("ink: {e}") }])?;

    let mut head: Vec<String> = Vec::new();
    let mut css: Vec<String> = Vec::new();
    let (mut root, mut caps): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut checks: Vec<String> = Vec::new();
    for t in story.get_global_tags().unwrap_or_default().iter().map(|t| untag(t)) {
        let (k, v) = t.split_once(':').map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())).unwrap_or((t.trim().to_lowercase(), String::new()));
        match k.as_str() {
            "title" | "format" | "uses" | "angle" | "hook" | "mode" | "refuses" => {
                let key = if k == "uses" { "Uses".to_string() } else { format!("{}{}", k[..1].to_uppercase(), &k[1..]) };
                head.push(format!("{key}: {v}"));
            }
            "check" => checks.push(format!("! {v}")),
            // the stylesheet as plain tags (ink reads `{...}` in a tag as logic): :root's and captions' properties
            "music" | "music-gain" | "sfx-gain" | "platform" | "background" | "eyes" => root.push(format!("{k}: {v};")),
            "captions-place" => caps.push(format!("place: {v};")),
            "captions-keys" => caps.push(format!("keys: \"{}\";", v.trim_matches('"'))),
            _ if k.starts_with('$') => head.push(format!("{k}: {v}")),
            _ => head.push(format!("// tag not in the profile: {t}")),
        }
    }
    if !root.is_empty() { css.push(format!(":root {{ {} }}", root.join(" "))); }
    if !caps.is_empty() { css.push(format!("captions {{ {} }}", caps.join(" "))); }
    let mut body: Vec<(String, usize)> = Vec::new();
    let (mut knot, mut take) = (String::new(), String::new());
    let mut counts: Vec<i32> = vec![0; knots.len()];
    let mut steps = path.iter();
    // SPEAKER: words, or SPEAKER (delivery): words; a name of up to four words, an extension in capitals (V.O.) its own
    let line_re = regex::Regex::new(r"^([A-Za-z][\w.'-]*(?: [\w.'-]+){0,3}(?:\s*\([A-Z.' ]+\))?)(?:\s*\(([^)]*)\))?\s*:\s*(.+)$").unwrap();
    loop {
        while story.can_continue() {
            let text = story.cont().map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("ink: {e}") }])?;
            for c in calls.borrow_mut().drain(..) {
                body.push((c, 0));
            }
            let tags: Vec<String> = story.get_current_tags().unwrap_or_default().iter().map(|t| untag(t)).collect();
            let now: Vec<i32> = knots.iter().map(|k| story.get_visit_count_at_path_string(k).unwrap_or(0)).collect();
            if let Some(i) = (0..knots.len()).find(|&i| now[i] > counts.get(i).copied().unwrap_or(0)) {
                if knots[i] != knot {
                    knot = knots[i].clone();
                    body.push((String::new(), 0));
                    // a section's own name (spaces and all) when the knot carries it; knot names are identifiers
                    let name = tags.iter().find_map(|t| t.strip_prefix("section:").map(|v| v.trim().to_string())).unwrap_or_else(|| knot.clone());
                    let var = tags.iter().find_map(|t| t.strip_prefix("variant:").map(|v| format!(" {{{}}}", v.trim()))).unwrap_or_default();
                    body.push((format!("# {name}{var}"), 0));
                }
            }
            counts = now;
            let tag = |name: &str| tags.iter().find_map(|t| t.strip_prefix(&format!("{name}:")).map(|v| v.trim().to_string()));
            if let Some(syn) = tag("synopsis") {
                body.push((format!("= {syn}"), 0));
            }
            if let Some(t) = tag("take") {
                if t != take {
                    take = t.clone();
                    body.push((format!(".{t}"), 0));
                }
            }
            let line = text.trim();
            // `_`: a line that only carries its tags (a section with nothing in it yet; an empty knot isn't ink)
            if line.is_empty() || line == "_" {
                continue;
            }
            // a list prop last would close as `]]]`, which reads one bracket early
            let parts: String = tags.iter().filter_map(|t| t.strip_prefix("part:")).map(|p| { let p = p.trim(); format!(" [[{p}{}]]", if p.ends_with(']') { " " } else { "" }) }).collect();
            let only = tag("only").map(|r| format!(" {{{r}}}")).unwrap_or_default();
            if let Some(b) = line.strip_prefix(">>") {
                // an open beat as a line: `>> KIND: what is shown #find:...`, in the order and the knot it's played
                let find = tag("find").map(|f| format!(" [find: {f}]")).unwrap_or_default();
                body.push((format!("~ {}{find}{only}", b.trim()), 0));
            } else if line.starts_with('>') && line.ends_with('<') {
                body.push((format!("{line}{parts}"), 0));
            } else if let Some(c) = line_re.captures(line) {
                body.push((String::new(), 0));
                body.push((format!("@{}", c[1].trim()), 0));
                if let Some(d) = c.get(2) {
                    body.push((format!("({})", d.as_str()), 0));
                }
                if let Some(p) = tag("punch") {
                    body.push((if p.is_empty() { "PUNCH IN:".to_string() } else { format!("PUNCH IN {p}X:") }, 0));
                }
                body.push((format!("{}{parts}{only}", c[3].trim()), 0));
            } else {
                // not a line: the .brief parser refuses it with what it should be
                body.push((String::new(), 0));
                body.push((line.to_string(), 0));
            }
        }
        for c in calls.borrow_mut().drain(..) {
            body.push((c, 0));
        }
        let choices = story.get_current_choices();
        if choices.is_empty() {
            break;
        }
        let i = steps.next().copied().unwrap_or(0);
        if i >= choices.len() {
            return Err(vec![Refusal { line: 0, col: 0, msg: format!("ink: --path asks for choice {i} where there are {} ({})", choices.len(), choices.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" | ")) }]);
        }
        story.choose_choice_index(i).map_err(|e| vec![Refusal { line: 0, col: 0, msg: format!("ink: {e}") }])?;
    }
    let mut out: Vec<(String, usize)> = head.into_iter().map(|h| (h, 0)).collect();
    out.push((String::new(), 0));
    out.extend(checks.into_iter().map(|c| (c, 0)));
    out.extend(body);
    if !css.is_empty() {
        out.push(("---".into(), 0));
        out.extend(css.into_iter().map(|c| (c, 0)));
    }
    // a section heading straight after another, or blanks doubled, read the same; keep the text tidy
    let mut text = Vec::new();
    let mut map = Vec::new();
    for (l, src) in out {
        if l.is_empty() && text.last().is_some_and(|p: &String| p.is_empty()) {
            continue;
        }
        text.push(l);
        map.push(src);
    }
    Ok((text.join("\n") + "\n", map))
}

/// ink's syntax in words a line quotes: escaped so they read as words
fn esc(t: &str) -> String {
    let mut o = String::new();
    for (i, ch) in t.chars().enumerate() {
        if matches!(ch, '{' | '}' | '#' | '\\') || (i == 0 && matches!(ch, '*' | '+' | '-' | '~' | '=')) {
            o.push('\\');
        }
        o.push(ch);
    }
    o.replace("//", "/\\/").replace("->", "-\\>").replace("<>", "<\\>")
}

fn ident(name: &str, used: &mut Vec<String>) -> String {
    let mut k: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    k = k.trim_matches('_').to_string();
    if k.is_empty() || k.chars().next().unwrap().is_ascii_digit() { k = format!("s_{k}"); }
    let base = k.clone();
    let mut n = 2;
    while used.contains(&k) { k = format!("{base}_{n}"); n += 1; }
    used.push(k.clone());
    k
}

/// a .brief as an ink story in the raea profile: what `lower` reads back as the same .brief (the parity check)
pub fn from_brief(d: &crate::Doc) -> String {
    use crate::El;
    let mut out: Vec<String> = Vec::new();
    for c in &d.head { out.push(format!("// {c}")); }
    for (k, v, _) in &d.title { out.push(format!("# {}: {}", if k.starts_with('$') { k.clone() } else { k.to_lowercase() }, entag(v))); }
    for (el, _, _) in &d.body {
        if let El::Check { rule } = el { out.push(format!("# check: {}", entag(rule))); }
    }
    for (sel, decls, _) in &d.style {
        for (k, v, _, _) in decls {
            match sel.as_str() {
                ":root" => out.push(format!("# {k}: {}", entag(v))),
                "captions" => out.push(format!("# captions-{k}: {}", entag(v))),
                _ => {}
            }
        }
    }
    // the knots, in order; a divert from one to the next
    let mut used = Vec::new();
    let mut body: Vec<String> = Vec::new();
    let (mut speaker, mut delivery, mut punch, mut take, mut pending): (Option<String>, Option<String>, Option<f64>, Option<String>, Vec<String>) = (None, None, None, None, Vec::new());
    let mut first: Option<String> = None;
    let mut sent_take: Option<String> = None;
    for (el, _, _) in &d.body {
        match el {
            El::Section { name, variants, .. } => {
                let k = ident(name, &mut used);
                // a section whose tags no line took (it holds nothing to play): a `_` line carries them
                if !pending.is_empty() { body.push(format!("_{}", pending.drain(..).map(|t| format!(" #{}", entag(&t))).collect::<String>())); }
                // what comes before the first section plays first, then diverts into it
                if first.is_none() { first = Some(k.clone()); }
                body.push(format!("-> {k}"));
                body.push(String::new());
                body.push(format!("=== {k} ==="));
                pending.push(format!("section: {name}"));
                if !variants.is_empty() { pending.push(format!("variant: {}", variants.join(", "))); }
                speaker = None; delivery = None;
            }
            El::Synopsis(s_) => pending.push(format!("synopsis: {s_}")),
            El::Take { take: t, .. } => take = Some(t.clone()),
            El::Speaker(s_) => { speaker = Some(s_.clone()); delivery = None; }
            El::Delivery(s_) => delivery = Some(s_.clone()),
            El::Punch(p) => punch = p.or(Some(0.0)),
            El::Blank => { speaker = None; delivery = None; }
            El::Comment(c) => body.push(format!("// {c}")),
            El::Transition(t) => body.push(format!("// {t}")),
            El::Check { .. } => {}
            El::Beat { kind, phrase, find, variants } => {
                let mut tags = std::mem::take(&mut pending);
                if take.is_some() && take != sent_take { tags.push(format!("take:{}", take.clone().unwrap())); sent_take = take.clone(); }
                if !find.is_empty() { tags.push(format!("find:{}", find.join(" "))); }
                if !variants.is_empty() { tags.push(format!("only:{}", variants.join(", "))); }
                body.push(format!(">> {kind}: {}{}", esc(phrase), tags.iter().map(|t| format!(" #{}", entag(t))).collect::<String>()));
            }
            El::Screen { text, notes } => {
                let mut tags = std::mem::take(&mut pending);
                tags.extend(notes.iter().map(|n| format!("part:{}", n.raw.trim())));
                body.push(format!(">{}<{}", esc(text), tags.iter().map(|t| format!(" #{}", entag(t))).collect::<String>()));
            }
            El::Line { text, notes, variants } => {
                let mut tags = std::mem::take(&mut pending);
                if take.is_some() && take != sent_take { tags.push(format!("take:{}", take.clone().unwrap())); sent_take = take.clone(); }
                if let Some(p) = punch.take() { tags.push(if p == 0.0 { "punch:".into() } else { format!("punch:{p}") }); }
                tags.extend(notes.iter().map(|n| format!("part:{}", n.raw.trim())));
                if !variants.is_empty() { tags.push(format!("only:{}", variants.join(", "))); }
                let who = speaker.clone().unwrap_or_else(|| "SPEAKER".into());
                let d_ = delivery.as_ref().map(|x| format!(" ({x})")).unwrap_or_default();
                body.push(format!("{who}{d_}: {}{}", esc(text), tags.iter().map(|t| format!(" #{}", entag(t))).collect::<String>()));
            }
        }
    }
    if !pending.is_empty() { body.push(format!("_{}", pending.drain(..).map(|t| format!(" #{}", entag(&t))).collect::<String>())); }
    body.push("-> END".into());
    out.push(String::new());
    let _ = first;
    out.extend(body);
    out.join("\n") + "\n"
}
