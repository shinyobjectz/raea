//! One runner for every caller (docs/ask.feature AK3): a batch planned into calls (grouped by file, window, the files
//! beside it and what of the picture its questions look at, at most 64 questions a call, the voice question added to a
//! call whose questions depend on whether anyone speaks when that isn't known), each clip cut once, the calls
//! made in parallel, the answers streamed back as they come and kept. A pool that's spent partway keeps what came and
//! reports the rest as waiting for it. `dry` cuts the media and says what would be sent, and spends nothing. A group can
//! carry clips kept from an earlier call instead of a file to cut (`raea ask try`), and every question goes out with the
//! person's own calibration when it has earned one (calibrate.rs).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use raea_core_ask::{Kind, Picks, Question, Sees, Window, limits};
use raea_io_clef::{Answered, Call, Fail, Media, media};
use serde::Serialize;
use serde_json::{Value, json};

use crate::prepare::{self, Piece, VOICE};
use crate::{calibrate, record};

/// What to ask of one file: its questions (prepared), those dropped with why, a window over all of them (else each
/// question's own), and files set beside it (to pick between, or before and after).
pub struct Entry {
    pub piece: Arc<Piece>,
    pub window: Option<Window>,
    pub questions: Vec<Question>,
    pub dropped: Vec<(String, String)>,
    pub with: Vec<PathBuf>,
}

/// One call's worth.
pub struct Group {
    pub piece: Arc<Piece>,
    pub from: f64,
    pub to: Option<f64>,
    pub sees: Sees,
    pub hears: bool,
    pub with: Vec<PathBuf>,
    pub questions: Vec<Question>,
    /// clips kept from an earlier call, sent again in place of cutting the file
    pub kept: Option<Arc<Kept>>,
}

/// An earlier call's clips and state, and their seconds of video and sound (for the estimate).
pub struct Kept {
    pub media: Media,
    pub state: String,
    pub video_seconds: f64,
    pub sound_seconds: f64,
}

/// One answer, or why a question wasn't asked or still waits: the line `raea ask --json` prints for each.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Answer {
    /// its id in the record (`raea ask verdict` takes it)
    pub answer: Option<String>,
    pub file: String,
    pub id: String,
    pub version: String,
    pub from: f64,
    pub to: Option<f64>,
    pub says: Option<String>,
    pub value: Option<f64>,
    pub problem: Option<f64>,
    pub choice: Option<String>,
    pub flagged: bool,
    pub calibrated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_asked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waits: Option<String>,
    pub route: Option<String>,
    pub cached: bool,
}

/// What the run came to.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub asked: usize,
    pub answered: usize,
    pub calls: usize,
    pub cost: f64,
    pub estimate: f64,
    pub pool_left: Option<f64>,
    pub waiting: usize,
    pub flagged: usize,
    pub flagged_calibrated: usize,
    pub routes: BTreeSet<String>,
    /// the clips cut (each once, however many calls used it)
    pub clips: usize,
}

/// What the runner tells its caller as it goes: an answer, or (dry) a call it would make.
pub enum Event<'a> {
    Answer(&'a Answer),
    Would(&'a Value),
}

pub struct Opts {
    pub jobs: usize,
    pub dry: bool,
    /// how a call is made: `raea_io_clef::ask` (a test gives its own)
    pub ask: fn(&Call) -> Result<Answered, Fail>,
}

impl Opts {
    pub fn new(jobs: usize, dry: bool) -> Opts {
        Opts { jobs, dry, ask: raea_io_clef::ask }
    }
}

const NAMES: [&str; 4] = ["first", "second", "third", "fourth"];

/// A pick's options: the files given, in order (all, or all but the first, the reference).
fn picks(p: Picks, files: usize) -> Vec<String> {
    let n = files.min(NAMES.len());
    let skip = usize::from(p == Picks::AfterFirst);
    NAMES[skip.min(n)..n].iter().map(|s| s.to_string()).collect()
}

/// The batch as calls.
pub fn plan(entries: &[Entry]) -> (Vec<Group>, Vec<Answer>) {
    let mut groups: Vec<Group> = Vec::new();
    let mut unasked = Vec::new();
    let mine = calibrate::local();
    for e in entries {
        let file = e.piece.file.display().to_string();
        for (id, why) in &e.dropped {
            unasked.push(Answer { file: file.clone(), id: id.clone(), not_asked: Some(why.clone()), ..Default::default() });
        }
        for q in &e.questions {
            let w = e.window.or(q.window).unwrap_or_default();
            let (from, to) = (w.from.unwrap_or(0.0).max(0.0), w.to);
            let mut q = q.clone();
            calibrate::apply(&mut q, &mine);
            if let Some(p) = q.picks {
                q.kind = Kind::Choose;
                q.options = picks(p, 1 + e.with.len());
            }
            let compares = q.picks.is_some_and(|_| q.options.len() < 2) || (q.uses.iter().any(|u| u == "with") && e.with.is_empty());
            if compares {
                unasked.push(Answer { file: file.clone(), version: q.version(), not_asked: Some("it compares files: set one or more beside it (--with)".into()), id: q.id, ..Default::default() });
                continue;
            }
            // one place in each call is kept for the voice question
            let same = |g: &&mut Group| Arc::ptr_eq(&g.piece, &e.piece) && g.from == from && g.to == to && g.sees == q.sees && g.with == e.with && g.questions.len() < limits::QUESTIONS - 1;
            match groups.iter_mut().find(same) {
                Some(g) => {
                    g.hears |= q.hears;
                    g.questions.push(q);
                }
                None => groups.push(Group { piece: e.piece.clone(), from, to, sees: q.sees, hears: q.hears, with: e.with.clone(), questions: vec![q], kept: None }),
            }
        }
    }
    for g in &mut groups {
        g.hears &= g.piece.probe.sound || g.with.iter().any(|f| media::probe(f).is_ok_and(|p| p.sound));
        if g.hears && g.piece.voice.is_none() && g.questions.iter().any(|q| q.when.voice.is_some()) {
            g.questions.push(prepare::voice_question());
        }
    }
    (groups, unasked)
}

/// Clips cut once a run, by what they are.
type Cuts = Mutex<HashMap<String, Result<Arc<Vec<u8>>, String>>>;

fn cut(cuts: &Cuts, key: String, make: impl FnOnce() -> Result<Vec<u8>, String>) -> Result<Arc<Vec<u8>>, String> {
    if let Some(r) = cuts.lock().unwrap().get(&key) {
        return r.clone();
    }
    let r = make().map(Arc::new);
    cuts.lock().unwrap().insert(key, r.clone());
    r
}

/// A group's media, and its seconds of video and of sound (for the estimate).
fn media_of(g: &Group, cuts: &Cuts) -> Result<(Media, f64, f64), String> {
    if let Some(k) = &g.kept {
        return Ok((k.media.clone(), k.video_seconds, k.sound_seconds));
    }
    let files: Vec<&Path> = std::iter::once(g.piece.file.as_path()).chain(g.with.iter().map(PathBuf::as_path)).collect();
    let (from, to) = (g.from, g.to);
    let span = |p: &media::Probe| (to.unwrap_or(p.seconds) - from).clamp(0.0, p.seconds.max(0.1));
    let mut m = Media::default();
    let (mut vs, mut ss) = (0.0, 0.0);
    for f in &files {
        let p = if *f == g.piece.file.as_path() { g.piece.probe } else { media::probe(f)? };
        let k = |what: &str| format!("{what}:{}:{from}:{to:?}", f.display());
        match g.sees {
            Sees::Video if p.video => {
                m.videos.push(cut(cuts, k("video"), || media::video(f, from, to))?.to_vec());
                vs += span(&p).min(limits::VIDEO_SECONDS);
            }
            Sees::Video | Sees::Stills if !p.video => m.stills.push(cut(cuts, k("still"), || media::still(f, None))?.to_vec()),
            Sees::Stills if files.len() == 1 => {
                let len = span(&p);
                for i in 0..limits::STILLS {
                    let at = from + len * (i as f64 + 0.5) / limits::STILLS as f64;
                    m.stills.push(cut(cuts, k(&format!("frame{i}")), || media::still(f, Some(at)))?.to_vec());
                }
            }
            Sees::Stills => m.stills.push(cut(cuts, k("frame"), || media::still(f, Some(from + span(&p) / 2.0)))?.to_vec()),
            _ => {}
        }
        if g.hears && p.sound {
            m.sounds.push(cut(cuts, k("sound"), || media::sound(f, from, to))?.to_vec());
            ss += span(&p).min(limits::SOUND_SECONDS);
        }
    }
    if m.videos.len() > limits::VIDEOS || m.stills.len() > limits::STILLS || m.sounds.len() > limits::SOUNDS {
        return Err(format!("too many files for one call: at most {} videos, {} stills and {} sounds", limits::VIDEOS, limits::STILLS, limits::SOUNDS));
    }
    Ok((m, vs, ss))
}

enum Done {
    Asked(Answered, Vec<String>, String),
    Would(Value),
    Failed(String),
    Waits(String),
}

/// Every entry asked, at most `jobs` calls at once, each event told to `out` as it comes (in the caller's thread).
pub fn run(entries: Vec<Entry>, o: &Opts, out: &mut dyn FnMut(Event)) -> Summary {
    let (groups, unasked) = plan(&entries);
    run_groups(groups, unasked, o, out)
}

/// Planned groups asked, the questions not asked told first.
pub fn run_groups(groups: Vec<Group>, unasked: Vec<Answer>, o: &Opts, out: &mut dyn FnMut(Event)) -> Summary {
    let mut sum = Summary::default();
    for a in &unasked {
        out(Event::Answer(a));
    }
    let cuts: Cuts = Mutex::default();
    let (next, spent) = (AtomicUsize::new(0), AtomicBool::new(false));
    let spent_why = Mutex::new(String::new());
    let (tx, rx) = mpsc::channel::<(usize, Done)>();
    std::thread::scope(|s| {
        for _ in 0..o.jobs.clamp(1, 16).min(groups.len().max(1)) {
            let (tx, groups, cuts, next, spent, spent_why) = (tx.clone(), &groups, &cuts, &next, &spent, &spent_why);
            s.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(g) = groups.get(i) else { break };
                    let done = if spent.load(Ordering::Relaxed) {
                        Done::Waits(spent_why.lock().unwrap().clone())
                    } else {
                        match media_of(g, cuts) {
                            Err(e) => Done::Failed(e),
                            Ok((m, vs, ss)) => {
                                let state = g.kept.as_ref().map_or_else(|| g.piece.state(), |k| k.state.clone());
                                let call = Call { state, questions: g.questions.clone(), media: m };
                                let estimate = limits::estimate(vs, ss, call.media.stills.len(), call.state.len(), call.questions.len());
                                if o.dry {
                                    Done::Would(would(g, &call, estimate))
                                } else {
                                    match (o.ask)(&call) {
                                        Ok(a) => Done::Asked(a, kept(&call.media), call.state),
                                        Err(Fail::Spent(why)) => {
                                            spent.store(true, Ordering::Relaxed);
                                            *spent_why.lock().unwrap() = why.clone();
                                            Done::Waits(why)
                                        }
                                        Err(f) => Done::Failed(f.to_string()),
                                    }
                                }
                            }
                        }
                    };
                    if tx.send((i, done)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, done) in rx {
            told(&groups[i], done, &mut sum, out);
        }
    });
    sum.clips = cuts.lock().unwrap().values().filter(|c| c.is_ok()).count();
    sum
}

/// The clips a call sent, kept by hash for `raea ask try`.
fn kept(m: &Media) -> Vec<String> {
    let each = |v: &[Vec<u8>], ext: &str| v.iter().map(|b| record::keep_media(b, ext)).collect::<Vec<_>>();
    [each(&m.videos, "mp4"), each(&m.stills, "jpg"), each(&m.sounds, "mp3")].concat()
}

fn would(g: &Group, call: &Call, estimate: f64) -> Value {
    let sizes = |v: &[Vec<u8>]| v.iter().map(Vec::len).collect::<Vec<_>>();
    json!({
        "file": g.piece.file.display().to_string(), "from": g.from, "to": g.to,
        "questions": call.questions.iter().map(|q| json!({"id": q.id, "ask": q.ask, "type": q.kind})).collect::<Vec<_>>(),
        "state": call.state,
        "media": {"videos": sizes(&call.media.videos), "stills": sizes(&call.media.stills), "sounds": sizes(&call.media.sounds)},
        "estimate_usd": estimate,
    })
}

/// A finished call's answers told, kept and counted. When the voice question went along, the variant that doesn't
/// apply (sound where no one speaks, sound-silent where someone does) is set aside.
fn told(g: &Group, done: Done, sum: &mut Summary, out: &mut dyn FnMut(Event)) {
    let file = g.piece.file.display().to_string();
    // the voice question only decides between variants: it isn't told
    let asked: Vec<&Question> = g.questions.iter().filter(|q| q.id != VOICE).collect();
    let base = |q: &Question| Answer { file: file.clone(), id: q.id.clone(), version: q.version(), from: g.from, to: g.to, calibrated: q.calibrated(), ..Default::default() };
    match done {
        Done::Would(v) => {
            sum.calls += 1;
            sum.estimate += v["estimate_usd"].as_f64().unwrap_or(0.0);
            out(Event::Would(&v));
        }
        Done::Failed(why) | Done::Waits(why) if g.questions.is_empty() => drop(why),
        Done::Failed(why) => asked.iter().for_each(|q| out(Event::Answer(&Answer { not_asked: Some(why.clone()), ..base(q) }))),
        Done::Waits(why) => {
            sum.waiting += asked.len();
            asked.iter().for_each(|q| out(Event::Answer(&Answer { waits: Some(format!("waits for the pool: {why}")), ..base(q) })));
        }
        Done::Asked(a, clips, state) => {
            sum.calls += 1;
            sum.asked += asked.len();
            sum.cost += a.cost;
            sum.pool_left = a.pool_left.or(sum.pool_left);
            sum.routes.insert(a.route.clone());
            let project = g.piece.project.as_ref().map(|p| p.display().to_string());
            let call = record::call(&file, project.as_deref(), g.from, g.to, &state, &clips, &a.route, a.cost, a.cached);
            let heard = a.answers.get(VOICE).and_then(|v| v["noul"].as_f64());
            for q in g.questions.iter().filter(|q| q.id != VOICE) {
                let Some(ans) = a.answers.get(&q.id) else {
                    out(Event::Answer(&Answer { not_asked: Some("no answer came back for it".into()), ..base(q) }));
                    continue;
                };
                if let (Some(p), Some(v)) = (heard, q.when.voice) {
                    if v != (p >= 0.5) {
                        let who = if p >= 0.5 { "someone speaks" } else { "no one speaks" };
                        out(Event::Answer(&Answer { not_asked: Some(format!("set aside: {who} ({p:.2})")), ..base(q) }));
                        continue;
                    }
                }
                let r = q.read(ans);
                let flagged = q.flagged(&r);
                sum.answered += 1;
                sum.flagged += usize::from(flagged);
                sum.flagged_calibrated += usize::from(flagged && q.calibrated());
                let id = record::answer(&call, &file, q, &r, flagged);
                out(Event::Answer(&Answer {
                    answer: Some(id),
                    says: Some(r.says),
                    value: Some(r.value),
                    problem: r.problem,
                    choice: r.choice,
                    flagged,
                    route: Some(a.route.clone()),
                    cached: a.cached,
                    ..base(q)
                }));
            }
        }
    }
}
