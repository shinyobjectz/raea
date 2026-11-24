//! A flow run step by step where route() sends each one. Every step's output is kept under its key in the run's
//! folder (`<key>.<ext>`, indexed in gen.jsonl), so a step whose inputs haven't changed is never made twice. A made
//! step is quoted first and refused when it would pass the run's cap (--max-cents, else RAEA_GEN_MAX_CENTS, else
//! 100); its output stays a `job_…` for the next made step to read without a download and upload. `plan` routes and
//! quotes without sending anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value as J, json};

use crate::comfy::server::Comfy;
use crate::fal::Fal;
use crate::flow::{Flow, Step, index, reference, substitute};
use crate::made::{Made, outputs};
use crate::route::{Backend, Policy, made_body, route};

/// A model the host runs on this machine (raea's sao): the step, its inputs' files, where to write.
pub trait Local {
    fn names(&self) -> Vec<String>;
    fn make(&self, name: &str, step: &Step, dir: &Path, key: &str) -> Result<PathBuf, String>;
}

#[derive(Clone, Debug, Default)]
pub struct Made1 {
    pub files: Vec<PathBuf>,
    pub job: Option<String>,
    pub url: Option<String>,
}

pub struct Run<'a> {
    pub dir: PathBuf,
    pub base: PathBuf,
    pub policy: Policy,
    pub max_cents: u64,
    pub local: Option<&'a dyn Local>,
    pub stall: Duration,
}

impl<'a> Run<'a> {
    pub fn new(dir: PathBuf, base: PathBuf) -> Run<'a> {
        let max_cents = crate::setting("RAEA_GEN_MAX_CENTS").and_then(|c| c.parse().ok()).unwrap_or(100);
        Run { dir, base, policy: Policy::default(), max_cents, local: None, stall: Duration::from_secs(600) }
    }

    fn index(&self) -> BTreeMap<String, Made1> {
        std::fs::read_to_string(self.dir.join("gen.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<J>(l).ok()).filter_map(|l| {
            let files: Vec<PathBuf> = l["files"].as_array()?.iter().filter_map(|f| f.as_str()).map(|f| self.dir.join(f)).collect();
            (!files.is_empty() && files.iter().all(|f| f.exists())).then(|| (l["key"].as_str().unwrap_or_default().to_string(), Made1 { files, job: l["job"].as_str().map(str::to_string), url: None }))
        }).collect()
    }

    fn keep(&self, key: &str, id: &str, s: &Step, b: &Backend, m: &Made1, cents: u64) {
        // a load is its file, read again each run
        if *b == Backend::Load {
            return;
        }
        let names: Vec<String> = m.files.iter().filter_map(|f| f.strip_prefix(&self.dir).ok().map(|p| p.display().to_string())).collect();
        let line = json!({"key": key, "step": id, "op": s.op, "backend": b.to_string(), "files": names, "job": m.job, "cents": cents, "t": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())});
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("gen.jsonl")) {
            let _ = std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes());
        }
        if cents > 0 {
            let spend = crate::made::cached("spend.jsonl");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(spend) {
                let _ = std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes());
            }
        }
    }

    /// What each step would do and cost, sending nothing that runs or bills (made's /v1/cost is free).
    pub fn plan(&self, flow: &Flow) -> Result<J, String> {
        let made = Made::from_settings().ok();
        let cached = self.index();
        let keys = flow.keys_in(Some(&self.base))?;
        let (mut rows, mut total) = (Vec::new(), 0u64);
        // a text step's words aren't written yet: a stand-in prices the prompts that take them
        let words = |r: &str| flow.steps.get(r).filter(|t| t.op == "text.generate").map(|_| format!("(the words of step {r})"));
        for id in flow.order()? {
            let s = &Step { inputs: substitute(&J::Object(flow.steps[&id].inputs.clone()), &words).as_object().cloned().unwrap_or_default(), ..flow.steps[&id].clone() };
            let (b, note) = route(s, &self.policy)?;
            let mut row = json!({"step": id, "op": s.op, "runs_on": b.to_string(), "from": s.from});
            if let Some(n) = note {
                row["note"] = json!(n);
            }
            if cached.contains_key(&keys[&id]) {
                row["cached"] = json!(true);
            } else if let (Backend::Made(m), Some(made)) = (&b, &made) {
                let model = made.model(m)?;
                // a quote reads the media's count, not its bytes: a stand-in URL prices it
                let (body, fit) = made_body(s, &model, &|_| Ok("https://www.made.app/favicon.ico".into()))?;
                let by_length = model["inputSecondsPricing"]["usdPerSecondByResolution"].as_object().map(|r| {
                    let res = s.inputs.get("resolution").and_then(|v| v.as_str()).or(model["defaultResolution"].as_str()).unwrap_or("");
                    let per = r.get(res).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    format!("priced by the input's length ({res}: ${per:.4} a second); made reads it when the run uploads it, so the run quotes it exactly before sending")
                });
                match by_length.map_or_else(|| made.cost(&body), Err) {
                    Ok((c, p)) if p < c && crate::direct::possible(&model) && crate::setting("FAL_KEY").is_some() && crate::setting("RAEA_GEN_VIA").as_deref() != Some("made") => {
                        total += p;
                        row["cents"] = json!(p);
                        row["note"] = json!(format!("made would charge {c}¢ (a whole credit); fal's own {p}¢, so it goes to fal directly"));
                    }
                    Ok((c, p)) => {
                        total += c;
                        row["cents"] = json!(c);
                        // made bills whole credits: what that adds over the provider's own price, shown, not hidden
                        if c > p {
                            row["provider_cents"] = json!(p);
                            row["rounding_cents"] = json!(c - p);
                        }
                    }
                    Err(e) => row["quote"] = json!(e),
                }
                if !fit.is_empty() {
                    row["fitted"] = json!(fit);
                }
            } else if let Backend::Fish(_) = &b {
                // Fish bills the words' bytes: known before anything is sent
                let c = crate::fish::quote(s.inputs.get("text").and_then(|t| t.as_str()).unwrap_or(""));
                total += c;
                row["cents"] = json!(c);
            } else if let Backend::Tripo(m) = &b {
                // a plan uploads nothing: each picture input counts as one view
                let views = ["image", "refs"].iter().map(|k| match s.inputs.get(*k) { Some(J::Array(a)) => a.len(), Some(_) => 1, None => 0 }).sum::<usize>();
                let media: Vec<String> = (0..views).map(|k| format!("https://example.invalid/view{k}.png")).collect();
                match crate::tripo::quote(m, &s.inputs, &media) {
                    Ok(Some(c)) => { let cents = (c as f64 * crate::tripo::cents_per_credit()).ceil() as u64; total += cents; row["cents"] = json!(cents); row["credits"] = json!(c); }
                    Ok(None) => row["quote"] = json!("Tripo publishes no estimate for this model (never zero)"),
                    Err(e) => row["quote"] = json!(e),
                }
            } else if let Backend::Fal(e) = &b {
                if let Ok((c, unit)) = Fal::from_settings().and_then(|f| f.price(e)) {
                    row["cents_per"] = json!(format!("{c:.1} per {unit}"));
                    total += c.ceil() as u64;
                }
            }
            rows.push(row);
        }
        Ok(json!({"steps": rows, "cents": total, "cap_cents": self.max_cents}))
    }

    /// The flow made: each output step's files.
    pub fn run(&self, flow: &Flow) -> Result<J, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let keys = flow.keys_in(Some(&self.base))?;
        let cached = self.index();
        let mut done: BTreeMap<String, Made1> = BTreeMap::new();
        let (mut spent, mut rows) = (0u64, Vec::new());
        let mut made: Option<Made> = None;
        for id in flow.order()? {
            let s = &flow.steps[&id];
            let key = &keys[&id];
            let (b, note) = route(s, &self.policy)?;
            if let Some(m) = cached.get(key) {
                rows.push(json!({"step": id, "runs_on": b.to_string(), "cached": true, "files": m.files}));
                done.insert(id, m.clone());
                continue;
            }
            // a reference to words another step wrote (text.generate) reads as those words
            let s = &inline_text(s, &done);
            // a batch (several pictures from one step) goes to a model as each of its pictures
            let spread;
            let s = if matches!(b, Backend::Made(_) | Backend::Fal(_) | Backend::Replicate(_) | Backend::OpenRouter(_) | Backend::Modal(_)) { spread = spread_batches(s, &done); &spread } else { s };
            eprintln!("raea gen: {id} ({}) on {b}{}", s.op, note.as_deref().map(|n| format!(" — {n}")).unwrap_or_default());
            let (out, cents) = match &b {
                Backend::Load => (self.load(s)?, 0),
                Backend::Native => {
                    let file = |v: &J| -> Result<PathBuf, String> {
                        let r = reference(v).ok_or(format!("step `{id}`: {v} isn't a step's output"))?;
                        let m = done.get(r).ok_or(format!("step `{id}`: {r} hasn't run"))?;
                        match (m.files.first(), &m.url) {
                            (Some(f), _) => Ok(f.clone()),
                            (None, Some(u)) => save_all(std::slice::from_ref(u), &self.dir, &format!("{key}-in")).map(|f| f[0].clone()),
                            _ => Err(format!("step `{id}`: {r} made no file")),
                        }
                    };
                    (Made1 { files: vec![crate::native::run(s, &file, &self.dir, key)?], ..Default::default() }, 0)
                }
                Backend::Made(m) => {
                    if made.is_none() {
                        made = Some(Made::from_settings()?);
                    }
                    self.made(made.as_ref().unwrap(), m, s, key, &done, &mut spent)?
                }
                Backend::Fal(e) => (self.fal(e, &s, key, &done)?, 0),
                Backend::Tripo(m) => {
                    // the credits are known before sending (tripo make --dry-run): held to the run's cap like a made step
                    let media = self.tripo_media(s, &done)?;
                    if let Some(c) = crate::tripo::quote(m, &s.inputs, &media).ok().flatten() {
                        let charge = (c as f64 * crate::tripo::cents_per_credit()).ceil() as u64;
                        if spent + charge > self.max_cents {
                            return Err(format!("{b} would cost {charge}¢ ({c} credits), past this run's cap of {}¢ ({spent}¢ spent): raise --max-cents or RAEA_GEN_MAX_CENTS", self.max_cents));
                        }
                    }
                    match self.tripo(m, s, key, &done) {
                        Ok((m1, c)) => { spent += c; (m1, c) }
                        // no credits or no key: nothing was submitted, so the same model on fal (FAL_KEY) runs instead,
                        // unless RAEA_TRIPO_FALLBACK=0
                        Err(crate::tripo::Fail::Credits(e) | crate::tripo::Fail::Auth(e)) if crate::setting("FAL_KEY").is_some() && crate::setting("RAEA_TRIPO_FALLBACK").as_deref() != Some("0") => {
                            let views = media.len();
                            let twin = crate::tripo::fal_twin(m, views);
                            eprintln!("raea gen: {id}: {e}\nraea gen: {id} runs on fal:{twin} instead (RAEA_TRIPO_FALLBACK=0 stops this)");
                            let mut m1 = self.fal(&twin, s, key, &done)?;
                            m1.job = Some(format!("fal:{twin}"));
                            (m1, 0)
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                }
                Backend::Replicate(m) => (self.replicate(m, &s, key, &done)?, 0),
                Backend::OpenRouter(m) => self.openrouter(m, &s, key, &done)?,
                Backend::Modal(m) => self.modal(m, &s, key, &done)?,
                Backend::Fish(m) => {
                    // the words' price is known before sending: held to the run's cap like a made step
                    let charge = crate::fish::quote(s.inputs.get("text").and_then(|t| t.as_str()).unwrap_or(""));
                    if spent + charge > self.max_cents {
                        return Err(format!("{b} would cost {charge}¢, past this run's cap of {}¢ ({spent}¢ spent): raise --max-cents or RAEA_GEN_MAX_CENTS", self.max_cents));
                    }
                    let (m1, c) = self.fish(m, &s, key)?;
                    spent += c;
                    (m1, c)
                }
                Backend::Comfy => (self.comfy(&id, s, key, &done)?, 0),
                Backend::Local(n) => {
                    let l = self.local.filter(|l| l.names().contains(n)).ok_or(format!("step `{id}` wants local:{n}, which this host doesn't run"))?;
                    let resolved = Step { inputs: substitute(&J::Object(s.inputs.clone()), &|r| done.get(r).and_then(|m| m.files.first()).map(|f| f.display().to_string())).as_object().cloned().unwrap_or_default(), ..s.clone() };
                    (Made1 { files: vec![l.make(n, &resolved, &self.dir, key)?], ..Default::default() }, 0)
                }
            };
            self.keep(key, &id, s, &b, &out, cents);
            // a made model sent to fal directly (GN3) says where it really ran
            let (runs_on, note) = match out.job.as_deref().filter(|j| j.starts_with("fal:")) {
                Some(j) => (j.to_string(), Some(format!("{b} sent to fal directly: made rounds up to a whole credit"))),
                None => (b.to_string(), note),
            };
            rows.push(json!({"step": id, "runs_on": runs_on, "cents": cents, "files": out.files, "note": note}));
            done.insert(id, out);
        }
        // each output under its own name too (<dir>/clip.mp4), so a composition can name it: staticFile("gen/clip.mp4")
        let mut outs = serde_json::Map::new();
        for o in &flow.out {
            let files = done.get(o).map(|m| m.files.clone()).unwrap_or_default();
            outs.insert(o.clone(), json!(name_output(&self.dir, o, &files)?));
        }
        Ok(json!({"out": outs, "steps": rows, "cents": spent}))
    }

    fn load(&self, s: &Step) -> Result<Made1, String> {
        if let Some(u) = s.inputs.get("url").and_then(|u| u.as_str()) {
            return Ok(Made1 { url: Some(u.into()), ..Default::default() });
        }
        let p = s.inputs.get("path").and_then(|p| p.as_str()).ok_or("media.load needs a path or url")?;
        let f = if Path::new(p).is_absolute() { PathBuf::from(p) } else { self.base.join(p) };
        if !f.is_file() {
            return Err(format!("media.load: no file {}", f.display()));
        }
        Ok(Made1 { files: vec![f], ..Default::default() })
    }

    fn made(&self, made: &Made, m: &str, s: &Step, key: &str, done: &BTreeMap<String, Made1>, spent: &mut u64) -> Result<(Made1, u64), String> {
        let model = made.model(m)?;
        // each media as made reads it (a job, a URL, or an upload), uploaded once: the quote and the job share it
        let sent = std::cell::RefCell::new(BTreeMap::<String, String>::new());
        let media = |v: &J| -> Result<String, String> {
            if let Some(m) = sent.borrow().get(&v.to_string()) {
                return Ok(m.clone());
            }
            let r = reference(v).ok_or("not a reference")?;
            let up = done.get(r).ok_or(format!("@{r} isn't made yet"))?;
            // a job or URL stands for the whole output; one picture of a batch goes up as its file
            let m = match up.job.as_ref().or(up.url.as_ref()).filter(|_| index(v).is_none()) {
                Some(j) => j.clone(),
                None => made.upload(file_of(done, v).ok_or(format!("{v} names no file of @{r}"))?)?,
            };
            sent.borrow_mut().insert(v.to_string(), m.clone());
            Ok(m)
        };
        // priced first. A quote reads most media's count, not their bytes, so a stand-in prices them; a model priced
        // by its input's length (Seedance's video edit: per second of the source) is quoted with the real input,
        // which made has to read (a stand-in answers "couldn't read the source video")
        let by_length = model["inputSecondsPricing"].is_object();
        let stand_in = |_: &J| -> Result<String, String> { Ok("https://www.made.app/favicon.ico".into()) };
        let (probe, notes) = if by_length { made_body(s, &model, &media)? } else { made_body(s, &model, &stand_in)? };
        for n in notes {
            eprintln!("raea gen:   {n}");
        }
        let (cents, provider) = made.cost(&probe)?;
        let direct = provider < cents && crate::direct::possible(&model) && crate::setting("RAEA_GEN_VIA").as_deref() != Some("made") && crate::setting("FAL_KEY").is_some();
        let charge = if direct { provider } else { cents };
        if *spent + charge > self.max_cents {
            return Err(format!("{m} would cost {charge}¢, past this run's cap of {}¢ ({}¢ spent): raise --max-cents or RAEA_GEN_MAX_CENTS", self.max_cents, *spent));
        }
        if direct {
            eprintln!("raea gen:   made charges {cents}¢ for this (a whole credit), fal {provider}¢: sent to fal directly");
            let fal = crate::fal::Fal::from_settings()?;
            let url = |v: &J| -> Result<String, String> {
                let up = done.get(reference(v).ok_or("not a reference")?).ok_or("not made yet")?;
                if index(v).is_some() {
                    return fal.upload(file_of(&done, v).ok_or("no such picture in the batch")?);
                }
                match (&up.url, up.files.first()) {
                    (Some(u), _) => Ok(u.clone()),
                    (None, Some(f)) => fal.upload(f),
                    _ => Err("made nothing".into()),
                }
            };
            let (body, _) = made_body(s, &model, &url)?;
            let (endpoint, input) = crate::direct::body(&model, &body)?;
            let resp = fal.run(&endpoint, &input, self.stall)?;
            let mut urls = Vec::new();
            crate::fal::urls(&resp, &mut urls);
            let files = if urls.is_empty() { vec![] } else { save_all(&urls, &self.dir, key)? };
            if files.is_empty() {
                return Err(format!("fal {endpoint} answered with no files"));
            }
            *spent += provider;
            return Ok((Made1 { files, job: Some(format!("fal:{endpoint}")), url: urls.first().cloned() }, provider));
        }
        let (mut body, _) = made_body(s, &model, &media)?;
        body["request_id"] = json!(format!("raea-{key}"));
        body["max_cost_cents"] = json!(cents);
        let job = made.create(&body)?;
        let jid = job["id"].as_str().ok_or_else(|| format!("made.app: no job id in {job}"))?.to_string();
        *spent += cents;
        let fin = made.wait(&jid, self.stall)?;
        let outs = outputs(&fin);
        let files = if outs.is_empty() { vec![] } else { save_all(&outs.iter().map(|o| o.0.clone()).collect::<Vec<_>>(), &self.dir, key)? };
        if files.is_empty() {
            return Err(format!("made.app job {jid} succeeded with no assets"));
        }
        Ok((Made1 { files, job: Some(jid), url: outs.first().map(|o| o.0.clone()) }, fin["costCents"].as_u64().unwrap_or(cents)))
    }

    /// The step alone as a ComfyUI graph: what it reads uploaded and loaded, the step written as nodes, its output saved.
    fn comfy(&self, id: &str, s: &Step, key: &str, done: &BTreeMap<String, Made1>) -> Result<Made1, String> {
        let server = Comfy::from_settings();
        if !server.wake(std::time::Duration::from_secs(600)) {
            return Err(format!("step `{id}` runs on ComfyUI, and nothing answers at {} (raea gen comfy setup; or RAEA_COMFY_URL for a hosted one)", server.url));
        }
        let mut mini = Flow::default();
        for need in s.needs() {
            let up = done.get(&need).ok_or(format!("@{need} isn't made yet"))?;
            let f = up.files.first().ok_or(format!("@{need} has no file for ComfyUI"))?;
            let name = server.upload(f)?;
            mini.steps.insert(need, Step::new("media.load", json!({"path": name})));
        }
        let mut me = s.clone();
        me.model = None;
        mini.steps.insert(id.to_string(), me);
        mini.out = vec![id.to_string()];
        let graph = crate::comfy::export(&mini)?;
        let pid = server.queue(&graph)?;
        let entry = server.wait(&pid, self.stall)?;
        let tmp = self.dir.join(format!(".{key}"));
        let _ = std::fs::create_dir_all(&tmp);
        // a step giving several slots saved each as save0, save0.1, …: its files in that order (@step.N)
        let n = s.inputs.get("slots").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(1).max(1);
        let mut got = server.fetch(&entry, "save0", &tmp)?;
        for j in 1..n {
            got.extend(server.fetch(&entry, &format!("save0.{j}"), &tmp)?);
        }
        let mut files = Vec::new();
        for (k, f) in got.into_iter().enumerate() {
            let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("bin").to_string();
            let dest = self.dir.join(format!("{}.{ext}", if k == 0 { key.to_string() } else { format!("{key}-{k}") }));
            std::fs::rename(&f, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
            files.push(dest);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(Made1 { files, ..Default::default() })
    }
}

/// The file a reference names: `@step` the step's main file, `@step.N` its Nth.
pub(crate) fn file_of<'a>(done: &'a BTreeMap<String, Made1>, v: &J) -> Option<&'a PathBuf> {
    done.get(reference(v)?)?.files.get(index(v).unwrap_or(0))
}

/// A step with each plain reference to a batch (a step whose files are all pictures, more than one) spread into one
/// reference per picture; a mesh with its textures isn't a batch.
pub(crate) fn spread_batches(s: &Step, done: &BTreeMap<String, Made1>) -> Step {
    let pictures = |r: &str| done.get(r).map(|m| m.files.len()).filter(|&n| n > 1 && done[r].files.iter().all(|f| matches!(f.extension().and_then(|e| e.to_str()), Some("png" | "jpg" | "jpeg" | "webp")))).unwrap_or(0);
    let spread = |v: &J| -> Vec<J> {
        match (reference(v), index(v)) {
            (Some(r), None) if pictures(r) > 1 => (0..pictures(r)).map(|i| json!(format!("@{r}.{i}"))).collect(),
            _ => vec![v.clone()],
        }
    };
    let inputs = s.inputs.iter().map(|(k, v)| {
        let v = match v {
            J::Array(a) => J::Array(a.iter().flat_map(spread).collect()),
            v => match spread(v) { one if one.len() == 1 => one[0].clone(), many => J::Array(many) },
        };
        (k.clone(), v)
    }).collect();
    Step { inputs, ..s.clone() }
}

/// A step with each reference to a .txt output replaced by its text.
fn inline_text(s: &Step, done: &BTreeMap<String, Made1>) -> Step {
    let text = |r: &str| done.get(r).and_then(|m| m.files.first()).filter(|f| f.extension().is_some_and(|e| e == "txt")).and_then(|f| std::fs::read_to_string(f).ok());
    Step { inputs: substitute(&J::Object(s.inputs.clone()), &text).as_object().cloned().unwrap_or_default(), ..s.clone() }
}

/// An output's files under the output's name: one file as `<name>.<ext>`, several (a mesh and its textures) as
/// `<name>/` holding them, and a GLB among them also as `<name>.glb`. Replaced on every run, so the name always holds the latest result.
#[cfg(test)]
pub(crate) fn name_output_for_test(dir: &Path, name: &str, files: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    name_output(dir, name, files)
}

fn name_output(dir: &Path, name: &str, files: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let copy = |from: &Path, to: &Path| {
        // a name that already is the file (a link to it, or the file itself) is left alone: copying a file onto itself
        // truncates it to nothing (11-05: four Tripo models lost their step files so). A link elsewhere is replaced.
        if let (Ok(a), Ok(b)) = (dunce::canonicalize(from), dunce::canonicalize(to)) {
            if a == b {
                return Ok(to.to_path_buf());
            }
        }
        if to.is_symlink() {
            let _ = std::fs::remove_file(to);
        }
        std::fs::copy(from, to).map(|_| to.to_path_buf()).map_err(|e| format!("{}: {e}", to.display()))
    };
    match files {
        [] => Ok(vec![]),
        [one] if one.parent() == Some(dir) => {
            let to = dir.join(format!("{name}.{}", one.extension().and_then(|e| e.to_str()).unwrap_or("bin")));
            Ok(vec![copy(one, &to)?])
        }
        many => {
            let sub = dir.join(name);
            let _ = std::fs::remove_dir_all(&sub);
            std::fs::create_dir_all(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
            let mut out: Vec<PathBuf> = many.iter().map(|f| copy(f, &sub.join(f.file_name().unwrap_or_default()))).collect::<Result<_, _>>()?;
            // a GLB holds its own textures: it also goes as <name>.glb, the file a scene names (gen/kite.glb)
            if let Some(glb) = many.first().filter(|f| f.extension().is_some_and(|e| e == "glb")) {
                out.insert(0, copy(glb, &dir.join(format!("{name}.glb")))?);
            }
            Ok(out)
        }
    }
}

/// A step's files from their URLs: one file is `<key>.<ext>`; several stay together in `<key>/` under their own names
/// (an OBJ's material and textures find each other by name), the main one first (a GLB, else an OBJ, else the first).
pub(crate) fn save_all(urls: &[String], dir: &Path, key: &str) -> Result<Vec<PathBuf>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let urls: Vec<&String> = urls.iter().filter(|u| seen.insert(crate::http::redact(u).to_string())).collect();
    if urls.len() == 1 {
        return Ok(vec![crate::openrouter::save(urls[0], &dir.join(key))?]);
    }
    let sub = dir.join(key);
    std::fs::create_dir_all(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
    let mut files = Vec::new();
    for (k, u) in urls.iter().enumerate() {
        let name = Path::new(crate::http::redact(u)).file_name().and_then(|n| n.to_str()).map(|n| n.to_string()).unwrap_or(format!("file{k}"));
        let stem = sub.join(Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("file"));
        let f = crate::openrouter::save(u, &stem)?;
        // made and fal name some outputs by their content type only; keep the URL's own extension
        let want = sub.join(&name);
        if f != want && Path::new(&name).extension().is_some() {
            let _ = std::fs::rename(&f, &want);
            files.push(want);
        } else {
            files.push(f);
        }
    }
    let rank = |p: &PathBuf| match p.extension().and_then(|e| e.to_str()) { Some("glb") => 0, Some("gltf") => 1, Some("obj") => 2, Some("mp4" | "mov" | "webm") => 3, _ => 4 };
    files.sort_by_key(rank);
    Ok(files)
}
