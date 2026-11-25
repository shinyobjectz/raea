//! What is said of planned detail once the frames are read: the parts a job is held at (`raea make status`'s lod gate,
//! MK24) and the table `raea scene lod` prints (MK23).

use raea_core_facts::Finding;

use crate::{Seen, level};

/// Of a render's or check's findings, the parts seen closer than planned (`lod-closer`), and those of them not yet
/// answered in the job's notes: a line of NOTES.md with `lod` and the part's name answers it.
pub fn held<'a>(findings: &'a [Finding], notes: &str) -> (Vec<&'a str>, Vec<&'a str>) {
    let answered = |name: &str| notes.lines().any(|l| l.to_lowercase().contains("lod") && l.contains(name));
    let name = |f: &'a Finding| (f.check == "lod-closer").then(|| f.about.first()).flatten().map(|a| a.name.as_str());
    let closer: Vec<&str> = findings.iter().filter_map(name).collect();
    let open = closer.iter().copied().filter(|n| !answered(n)).collect();
    (closer, open)
}

/// A composition's lod gate, from the findings of its last render or check (the record's entry `id`): whether it is
/// clear and what it says, None when the job plans no level and the render found none. Two levels finer is an error,
/// which the checks gate holds until it's fixed; one level finer is held here until the part is replanned and rendered
/// again, or answered in the notes.
pub fn gate(of: &str, id: &str, findings: &[Finding], notes: &str, plans: bool) -> Option<(bool, String)> {
    let (closer, open) = held(findings, notes);
    if open.is_empty() {
        return (plans || !closer.is_empty()).then(|| (true, format!("no part closer than its planned detail left unanswered at {id}")));
    }
    Some((false, format!("seen closer than planned at {id}: {}. Build each for the level it's seen at and plan it there (`lod` on its object in brief.ts or on the part), keep the camera off it, or answer in NOTES.md on a line with `lod` and its name; raea scene lod {of} lists every part", open.join(", "))))
}

/// What a row says of its part: closer than planned, never as close as planned, or as planned.
fn verdict(s: &Seen) -> &'static str {
    match (s.planned, s.level()) {
        (Some(p), Some(l)) if l < p => "CLOSER than planned",
        (Some(p), Some(l)) if l > p => "never this close: planned finer than any frame shows it",
        (Some(_), _) => "as planned",
        (None, _) => "no plan",
    }
}

/// The rows in reading order: seen closer than planned (the widest gap first), the rest of the planned, then the
/// unplanned, each by how large it gets.
pub fn ordered(mut seen: Vec<Seen>) -> Vec<Seen> {
    let gap = |s: &Seen| s.planned.zip(s.level()).map_or(0, |(p, l)| p.saturating_sub(l));
    let size = |s: &Seen| s.finest.map_or(0.0, |f| f.1);
    seen.sort_by(|a, b| (gap(b), b.planned.is_some()).cmp(&(gap(a), a.planned.is_some())).then(size(b).total_cmp(&size(a))).then(a.name.cmp(&b.name)));
    seen
}

/// The table: a row a part, its plan beside the finest level a frame showed it at, how much of the frame it spanned
/// there, at which frame, and how many frames it was in.
pub fn table(seen: &[Seen]) -> Vec<String> {
    let mut t = vec![format!("{:<32} {:>7} {:>5} {:>7} {:>7} {:>7}  ", "part", "planned", "seen", "largest", "at", "frames")];
    let n = |l: Option<u8>| l.map_or("-".into(), |l| l.to_string());
    for s in seen {
        let (at, span) = s.finest.map_or(("-".into(), "-".into()), |(f, sp)| (f.to_string(), format!("{:.0}%", sp * 100.0)));
        t.push(format!("{:<32} {:>7} {:>5} {:>7} {:>7} {:>7}  {}", s.name.chars().take(32).collect::<String>(), n(s.planned), n(s.finest.map(|f| level(f.1))), span, at, s.frames, verdict(s)));
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use raea_core_facts::Level;

    fn row(name: &str, planned: Option<u8>, span: f32) -> Seen {
        Seen { name: name.into(), planned, frames: 10, finest: Some((4, span)), closer: 0 }
    }

    #[test]
    fn the_table_reads_closer_than_planned_first_then_planned_then_the_rest_by_size() {
        let rows = ordered(vec![row("floor", None, 1.0), row("desk", Some(0), 0.6), row("kiosk", Some(3), 0.3), row("tower", Some(4), 0.05), row("lamp", None, 0.1), row("phone", Some(0), 0.04)]);
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["kiosk", "tower", "desk", "phone", "floor", "lamp"]);
        let t = table(&rows);
        assert!(t[1].starts_with("kiosk") && t[1].ends_with("CLOSER than planned"), "{}", t[1]);
        assert!(t[3].ends_with("as planned"), "{}", t[3]);
        assert!(t[4].ends_with("never this close: planned finer than any frame shows it"), "{}", t[4]);
        assert!(t[5].ends_with("no plan"), "{}", t[5]);
    }

    #[test]
    fn a_part_seen_closer_is_held_until_a_notes_line_with_lod_and_its_name_answers_it() {
        let closer = |name: &str| Finding::new("lod-closer", Level::Warn, format!("'{name}' is planned at LOD 3"), "raea render").about(name, None);
        let fs = [closer("kiosk"), Finding::new("clips", Level::Error, "the mug is in the desk", "raea render").about("mug", None), closer("tower")];
        assert_eq!(held(&fs, ""), (vec!["kiosk", "tower"], vec!["kiosk", "tower"]));
        // a line about the part that isn't about its detail answers nothing
        assert_eq!(held(&fs, "the kiosk is fine\n- LOD tower: one frame at the cut").1, ["kiosk"]);
        assert!(held(&fs, "- lod kiosk and tower: left as boxes, the cut hides them").1.is_empty());
        // the gate: open with the parts named, clear once answered, and absent from a job that plans and found nothing
        let open = gate("scene.tsx", "r1", &fs, "", false).unwrap();
        assert!(!open.0 && open.1.starts_with("seen closer than planned at r1: kiosk, tower.") && open.1.ends_with("raea scene lod scene.tsx lists every part"), "{open:?}");
        assert_eq!(gate("scene.tsx", "r1", &fs, "lod kiosk, tower: boxes", false).map(|g| g.0), Some(true));
        assert_eq!((gate("scene.tsx", "r1", &[], "", false), gate("scene.tsx", "r1", &[], "", true).map(|g| g.0)), (None, Some(true)));
    }
}
