//! raea's machine-learning runtime: ONNX models on ONNX Runtime (`ort`), the same pin and prebuilt runtime as void's
//! voice, raea/meaning/embed and raea/perception.
//!
//! Three layers:
//! - `session`: one ort session the way raea builds them (full graph optimisation, a few intra-op threads, CPU unless
//!   asked otherwise). void's voice models and Pocket TTS are built with it.
//! - the model registry (`load`, `Model::run`, `release`): one session per model file and options per process, shared
//!   by every caller (a render's second React, a game and its sound), counted, and freed a while after the last
//!   release. A composition reaches it through the `ml.*` native calls (@raea/motion/ml).
//! - jobs (`request`, `result`, `poll`): a run started on a worker now and joined later. A job is named by what it
//!   computes (the model and the input bytes), so joining it is a pure function of its inputs: a game may fold a job's
//!   result into its state and a render of the play folds the same one. `poll` (is it done yet?) depends on timing and
//!   is display-only.
//!
//! Determinism: a CPU session gives the same outputs for the same inputs run after run on one machine (ort's CPU
//! kernels split work by rows, not by the sums inside them); Core ML (feature `coreml`) does not promise that, so its
//! models say `deterministic: false` and their results are for display only.
//!
//! With the `tts` feature: Pocket TTS (pocket.rs), its word clock (words.rs) and the speech service games use
//! (speech.rs, @raea/motion/speech).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue};
use ort::value::{DynValue, Tensor as OrtTensor, TensorElementType, ValueType};

pub use half;
pub mod models;
#[cfg(feature = "tts")]
pub mod pocket;
#[cfg(feature = "tts")]
pub mod speech;
pub mod words;

/// Where a session runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Provider {
    /// ONNX Runtime's CPU kernels: the same outputs for the same inputs, run after run
    #[default]
    Cpu,
    /// Core ML (the GPU and the Neural Engine): faster for some models, display-only (not bit-exact)
    CoreMl,
}

/// How a session is built.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Options {
    pub provider: Provider,
    /// intra-op threads; None: the machine's cores, at most 4
    pub threads: Option<usize>,
    /// graph optimisation: 0 none, 1 basic, 2 extended, 3 all (default)
    pub optimization: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options { provider: Provider::Cpu, threads: None, optimization: 3 }
    }
}

impl Options {
    pub fn deterministic(&self) -> bool {
        self.provider == Provider::Cpu
    }
}

/// One ort session the way raea builds them (ai/meaning/embed): full graph optimisation, a few intra-op threads.
pub fn session(file: &Path) -> anyhow::Result<Session> {
    session_with(file, &Options::default())
}

/// One ort session with these options.
pub fn session_with(file: &Path, o: &Options) -> anyhow::Result<Session> {
    let threads = o.threads.unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2)).max(1);
    let level = match o.optimization {
        0 => GraphOptimizationLevel::Disable,
        1 => GraphOptimizationLevel::Level1,
        2 => GraphOptimizationLevel::Level2,
        _ => GraphOptimizationLevel::Level3,
    };
    let provider = o.provider;
    (|| -> ort::Result<Session> {
        let mut b = Session::builder()?.with_optimization_level(level)?.with_intra_threads(threads)?;
        if provider == Provider::CoreMl {
            #[cfg(feature = "coreml")]
            {
                b = b.with_execution_providers([ort::ep::CoreML::default().build()])?;
            }
        }
        b.commit_from_file(file)
    })()
    .map_err(|e| anyhow!("{}: {e}", file.display()))
    .and_then(|s| {
        if provider == Provider::CoreMl && !cfg!(feature = "coreml") {
            bail!("{}: this build has no Core ML (raea-ml's coreml feature); use the CPU", file.display())
        }
        Ok(s)
    })
}

/// A tensor's element type, as TypeScript's typed arrays name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DType {
    F32,
    F64,
    F16,
    I64,
    I32,
    I16,
    I8,
    U8,
    U16,
    U32,
    U64,
    Bool,
}

impl DType {
    pub fn name(self) -> &'static str {
        match self {
            DType::F32 => "float32",
            DType::F64 => "float64",
            DType::F16 => "float16",
            DType::I64 => "int64",
            DType::I32 => "int32",
            DType::I16 => "int16",
            DType::I8 => "int8",
            DType::U8 => "uint8",
            DType::U16 => "uint16",
            DType::U32 => "uint32",
            DType::U64 => "uint64",
            DType::Bool => "bool",
        }
    }

    pub fn parse(s: &str) -> anyhow::Result<DType> {
        Ok(match s {
            "float32" | "f32" => DType::F32,
            "float64" | "f64" => DType::F64,
            "float16" | "f16" => DType::F16,
            "int64" | "i64" => DType::I64,
            "int32" | "i32" => DType::I32,
            "int16" | "i16" => DType::I16,
            "int8" | "i8" => DType::I8,
            "uint8" | "u8" => DType::U8,
            "uint16" | "u16" => DType::U16,
            "uint32" | "u32" => DType::U32,
            "uint64" | "u64" => DType::U64,
            "bool" => DType::Bool,
            other => bail!("tensor type {other:?} (float32, float64, float16, int64, int32, int16, int8, uint8, uint16, uint32, uint64, bool)"),
        })
    }

    pub fn size(self) -> usize {
        match self {
            DType::F64 | DType::I64 | DType::U64 => 8,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F16 | DType::I16 | DType::U16 => 2,
            DType::I8 | DType::U8 | DType::Bool => 1,
        }
    }

    fn of(t: TensorElementType) -> Option<DType> {
        Some(match t {
            TensorElementType::Float32 => DType::F32,
            TensorElementType::Float64 => DType::F64,
            TensorElementType::Float16 => DType::F16,
            TensorElementType::Int64 => DType::I64,
            TensorElementType::Int32 => DType::I32,
            TensorElementType::Int16 => DType::I16,
            TensorElementType::Int8 => DType::I8,
            TensorElementType::Uint8 => DType::U8,
            TensorElementType::Uint16 => DType::U16,
            TensorElementType::Uint32 => DType::U32,
            TensorElementType::Uint64 => DType::U64,
            TensorElementType::Bool => DType::Bool,
            _ => return None,
        })
    }
}

/// A tensor as plain data: its type, shape and little-endian bytes (float16 as its bits).
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub dtype: DType,
    pub shape: Vec<i64>,
    pub bytes: Vec<u8>,
}

impl Tensor {
    pub fn f32(shape: Vec<i64>, data: &[f32]) -> Tensor {
        Tensor { dtype: DType::F32, shape, bytes: data.iter().flat_map(|x| x.to_le_bytes()).collect() }
    }

    pub fn i64(shape: Vec<i64>, data: &[i64]) -> Tensor {
        Tensor { dtype: DType::I64, shape, bytes: data.iter().flat_map(|x| x.to_le_bytes()).collect() }
    }

    pub fn as_f32(&self) -> Option<Vec<f32>> {
        (self.dtype == DType::F32).then(|| self.bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    pub fn len(&self) -> usize {
        self.shape.iter().map(|&d| d.max(0) as usize).product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn to_ort(&self) -> anyhow::Result<DynValue> {
        let n = self.len();
        anyhow::ensure!(self.bytes.len() == n * self.dtype.size(), "a {} tensor of shape {:?} holds {} bytes, not {}", self.dtype.name(), self.shape, self.bytes.len(), n * self.dtype.size());
        let shape = self.shape.clone();
        macro_rules! of {
            ($t:ty, $k:expr) => {{
                let v: Vec<$t> = self.bytes.chunks_exact($k).map(|c| <$t>::from_le_bytes(c.try_into().unwrap())).collect();
                OrtTensor::from_array((shape, v))?.into_dyn()
            }};
        }
        Ok(match self.dtype {
            DType::F32 => of!(f32, 4),
            DType::F64 => of!(f64, 8),
            DType::I64 => of!(i64, 8),
            DType::I32 => of!(i32, 4),
            DType::I16 => of!(i16, 2),
            DType::I8 => of!(i8, 1),
            DType::U8 => of!(u8, 1),
            DType::U16 => of!(u16, 2),
            DType::U32 => of!(u32, 4),
            DType::U64 => of!(u64, 8),
            DType::F16 => {
                let v: Vec<half::f16> = self.bytes.chunks_exact(2).map(|c| half::f16::from_bits(u16::from_le_bytes([c[0], c[1]]))).collect();
                OrtTensor::from_array((shape, v))?.into_dyn()
            }
            DType::Bool => {
                let v: Vec<bool> = self.bytes.iter().map(|b| *b != 0).collect();
                OrtTensor::from_array((shape, v))?.into_dyn()
            }
        })
    }

    fn from_ort(v: &DynValue) -> anyhow::Result<Tensor> {
        let ValueType::Tensor { ty, .. } = v.dtype() else { bail!("an output that isn't a tensor ({:?})", v.dtype()) };
        let dtype = DType::of(*ty).ok_or_else(|| anyhow!("output tensors of {ty:?} aren't supported"))?;
        macro_rules! of {
            ($t:ty) => {{
                let (s, d) = v.try_extract_tensor::<$t>()?;
                (s.iter().copied().collect::<Vec<i64>>(), d.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())
            }};
        }
        let (shape, bytes) = match dtype {
            DType::F32 => of!(f32),
            DType::F64 => of!(f64),
            DType::I64 => of!(i64),
            DType::I32 => of!(i32),
            DType::I16 => of!(i16),
            DType::I8 => of!(i8),
            DType::U8 => of!(u8),
            DType::U16 => of!(u16),
            DType::U32 => of!(u32),
            DType::U64 => of!(u64),
            DType::F16 => {
                let (s, d) = v.try_extract_tensor::<half::f16>()?;
                (s.iter().copied().collect(), d.iter().flat_map(|x| x.to_bits().to_le_bytes()).collect())
            }
            DType::Bool => {
                let (s, d) = v.try_extract_tensor::<bool>()?;
                (s.iter().copied().collect(), d.iter().map(|b| *b as u8).collect())
            }
        };
        Ok(Tensor { dtype, shape, bytes })
    }
}

/// One of a model's inputs or outputs: its name, element type ("float32"...; "other" for non-tensors) and shape (-1
/// where it's dynamic).
#[derive(Clone, Debug)]
pub struct Io {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<i64>,
}

fn io_of(name: &str, t: &ValueType) -> Io {
    match t {
        ValueType::Tensor { ty, shape, .. } => Io { name: name.into(), dtype: DType::of(*ty).map(|d| d.name().to_string()).unwrap_or_else(|| format!("{ty:?}")), shape: shape.iter().copied().collect() },
        other => Io { name: name.into(), dtype: format!("{other:?}"), shape: Vec::new() },
    }
}

/// A loaded model: one session, shared.
pub struct Model {
    pub id: u64,
    pub file: PathBuf,
    pub options: Options,
    pub inputs: Vec<Io>,
    pub outputs: Vec<Io>,
    session: Mutex<Session>,
    /// runs so far and their total time (the registry's stats)
    runs: Mutex<(u64, Duration)>,
}

impl Model {
    pub fn deterministic(&self) -> bool {
        self.options.deterministic()
    }

    /// One run: named inputs in, every output by name (in the model's order) out. `wanted` names a subset to return.
    pub fn run(&self, feeds: &[(String, Tensor)], wanted: Option<&[String]>) -> anyhow::Result<Vec<(String, Tensor)>> {
        let t = Instant::now();
        let mut inputs: Vec<(std::borrow::Cow<str>, SessionInputValue)> = Vec::with_capacity(feeds.len());
        for (name, v) in feeds {
            anyhow::ensure!(self.inputs.iter().any(|i| &i.name == name), "{} has no input {name:?} (its inputs: {})", self.file.display(), self.inputs.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(", "));
            inputs.push((std::borrow::Cow::Owned(name.clone()), v.to_ort().with_context(|| format!("input {name}"))?.into()));
        }
        let mut session = self.session.lock().unwrap();
        let mut out = session.run(inputs).map_err(|e| anyhow!("{}: {e}", self.file.display()))?;
        let mut got = Vec::new();
        for o in &self.outputs {
            if wanted.is_some_and(|w| !w.contains(&o.name)) {
                continue;
            }
            if let Some(v) = out.remove(&o.name) {
                got.push((o.name.clone(), Tensor::from_ort(&v).with_context(|| format!("output {}", o.name))?));
            }
        }
        drop(out);
        drop(session);
        let mut r = self.runs.lock().unwrap();
        r.0 += 1;
        r.1 += t.elapsed();
        Ok(got)
    }

    pub fn stats(&self) -> (u64, Duration) {
        *self.runs.lock().unwrap()
    }
}

/// The registry's entry for a model: the model, who holds it, and when it was last let go.
struct Held {
    model: Arc<Model>,
    refs: usize,
    idle_since: Option<Instant>,
}

#[derive(Default)]
struct Registry {
    by_key: HashMap<(PathBuf, Options), u64>,
    by_id: HashMap<u64, Held>,
    next: u64,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// How long a model nobody holds stays loaded (a game reloading it on its next run reuses it): RAEA_ML_IDLE_S, 30.
fn idle_limit() -> Duration {
    Duration::from_secs_f64(std::env::var("RAEA_ML_IDLE_S").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0))
}

/// Frees the models nobody has held for the idle limit.
fn sweep(r: &mut Registry) {
    let limit = idle_limit();
    let gone: Vec<u64> = r.by_id.iter().filter(|(_, h)| h.refs == 0 && h.idle_since.is_some_and(|t| t.elapsed() >= limit)).map(|(id, _)| *id).collect();
    for id in gone {
        if let Some(h) = r.by_id.remove(&id) {
            r.by_key.retain(|_, v| *v != id);
            drop(h);
        }
    }
}

/// The model at `file` with `options`: the session already loaded for them, or a new one. Each load is held until
/// its `release`; a model nobody holds is freed after the idle limit.
pub fn load(file: &Path, options: &Options) -> anyhow::Result<Arc<Model>> {
    let file = file.canonicalize().with_context(|| format!("{}: no such model", file.display()))?;
    let key = (file.clone(), options.clone());
    {
        let mut r = registry().lock().unwrap();
        sweep(&mut r);
        if let Some(id) = r.by_key.get(&key).copied()
            && let Some(h) = r.by_id.get_mut(&id)
        {
            h.refs += 1;
            h.idle_since = None;
            return Ok(h.model.clone());
        }
    }
    // built outside the lock (a big model takes a while; other models stay usable meanwhile)
    let session = session_with(&file, options)?;
    let inputs = session.inputs().iter().map(|o| io_of(o.name(), o.dtype())).collect();
    let outputs = session.outputs().iter().map(|o| io_of(o.name(), o.dtype())).collect();
    let mut r = registry().lock().unwrap();
    // another caller may have loaded it meanwhile: theirs is kept
    if let Some(id) = r.by_key.get(&key).copied()
        && let Some(h) = r.by_id.get_mut(&id)
    {
        h.refs += 1;
        h.idle_since = None;
        return Ok(h.model.clone());
    }
    r.next += 1;
    let id = r.next;
    let model = Arc::new(Model { id, file, options: options.clone(), inputs, outputs, session: Mutex::new(session), runs: Mutex::new((0, Duration::ZERO)) });
    r.by_key.insert(key, id);
    r.by_id.insert(id, Held { model: model.clone(), refs: 1, idle_since: None });
    Ok(model)
}

/// A loaded model by its id (the handle a composition holds).
pub fn get(id: u64) -> anyhow::Result<Arc<Model>> {
    let r = registry().lock().unwrap();
    r.by_id.get(&id).map(|h| h.model.clone()).ok_or_else(|| anyhow!("no model {id} loaded (released?)"))
}

/// One hold on a model let go; with none left it's freed after the idle limit (or at once with `now`).
pub fn release(id: u64, now: bool) {
    let mut r = registry().lock().unwrap();
    if let Some(h) = r.by_id.get_mut(&id) {
        h.refs = h.refs.saturating_sub(1);
        if h.refs == 0 {
            h.idle_since = Some(if now { Instant::now() - idle_limit() } else { Instant::now() });
        }
    }
    sweep(&mut r);
}

/// Every loaded model: (id, file, holds, runs, total run time).
pub fn loaded() -> Vec<(u64, PathBuf, usize, u64, Duration)> {
    let mut r = registry().lock().unwrap();
    sweep(&mut r);
    let mut v: Vec<_> = r.by_id.values().map(|h| {
        let (n, t) = h.model.stats();
        (h.model.id, h.model.file.clone(), h.refs, n, t)
    }).collect();
    v.sort_by_key(|x| x.0);
    v
}

// --- jobs: a run on a worker, named by what it computes -----------------------------------------------------------

type Outputs = Vec<(String, Tensor)>;

enum JobState {
    Running,
    Done(Result<Arc<Outputs>, String>),
}

struct Job {
    state: Mutex<JobState>,
    ready: Condvar,
    /// when it was last asked for (finished jobs are let go a while after)
    touched: Mutex<Instant>,
}

#[derive(Default)]
struct Jobs {
    map: HashMap<String, Arc<Job>>,
    queue: std::collections::VecDeque<(String, Arc<Model>, Vec<(String, Tensor)>)>,
}

struct Pool {
    jobs: Mutex<Jobs>,
    work: Condvar,
}

fn pool() -> &'static Arc<Pool> {
    static P: OnceLock<Arc<Pool>> = OnceLock::new();
    P.get_or_init(|| {
        let p = Arc::new(Pool { jobs: Mutex::new(Jobs::default()), work: Condvar::new() });
        // two workers: a heavy job doesn't hold up every other model's
        for i in 0..2 {
            let p = p.clone();
            std::thread::Builder::new().name(format!("raea-ml-{i}")).spawn(move || worker(&p)).expect("ml worker");
        }
        p
    })
}

fn worker(p: &Pool) {
    loop {
        let (key, model, feeds) = {
            let mut j = p.jobs.lock().unwrap();
            loop {
                if let Some(x) = j.queue.pop_front() {
                    break x;
                }
                j = p.work.wait(j).unwrap();
            }
        };
        let r = model.run(&feeds, None).map(Arc::new).map_err(|e| format!("{e:#}"));
        let job = p.jobs.lock().unwrap().map.get(&key).cloned();
        if let Some(job) = job {
            *job.state.lock().unwrap() = JobState::Done(r);
            job.ready.notify_all();
        }
    }
}

/// The name of a run: the model's file and options and every input's name, type, shape and bytes (FNV-1a, 128 bits).
pub fn job_key(model: &Model, feeds: &[(String, Tensor)]) -> String {
    let mut a: u64 = 0xcbf29ce484222325;
    let mut b: u64 = 0x84222325cbf29ce4;
    let mut eat = |bytes: &[u8]| {
        for &x in bytes {
            a = (a ^ x as u64).wrapping_mul(0x100000001b3);
            b = (b ^ x as u64).wrapping_mul(0x100000001b3).rotate_left(5);
        }
    };
    eat(model.file.to_string_lossy().as_bytes());
    eat(format!("{:?}", model.options).as_bytes());
    let mut sorted: Vec<&(String, Tensor)> = feeds.iter().collect();
    sorted.sort_by(|x, y| x.0.cmp(&y.0));
    for (name, t) in sorted {
        eat(name.as_bytes());
        eat(t.dtype.name().as_bytes());
        for d in &t.shape {
            eat(&d.to_le_bytes());
        }
        eat(&t.bytes);
    }
    format!("{a:016x}{b:016x}")
}

/// Starts a run on a worker (or finds the one already started for the same model and inputs) and returns its name.
pub fn request(model: Arc<Model>, feeds: Vec<(String, Tensor)>) -> String {
    let key = job_key(&model, &feeds);
    let p = pool();
    let mut j = p.jobs.lock().unwrap();
    // finished jobs nobody asked for in a minute are let go
    j.map.retain(|_, job| !matches!(*job.state.lock().unwrap(), JobState::Done(_)) || job.touched.lock().unwrap().elapsed() < Duration::from_secs(60));
    if let Some(job) = j.map.get(&key) {
        *job.touched.lock().unwrap() = Instant::now();
        return key;
    }
    j.map.insert(key.clone(), Arc::new(Job { state: Mutex::new(JobState::Running), ready: Condvar::new(), touched: Mutex::new(Instant::now()) }));
    j.queue.push_back((key.clone(), model, feeds));
    drop(j);
    p.work.notify_one();
    key
}

/// A job's outputs, waiting for it to finish (a pure function of what was requested: safe to fold into a game).
pub fn result(key: &str) -> anyhow::Result<Arc<Outputs>> {
    let job = pool().jobs.lock().unwrap().map.get(key).cloned().ok_or_else(|| anyhow!("no job {key} (request it first; finished jobs are kept a minute after their last use)"))?;
    *job.touched.lock().unwrap() = Instant::now();
    let mut s = job.state.lock().unwrap();
    loop {
        match &*s {
            JobState::Done(r) => return r.clone().map_err(|e| anyhow!(e)),
            JobState::Running => s = job.ready.wait(s).unwrap(),
        }
    }
}

/// A job's outputs if it has finished, without waiting. Display-only: whether it's done by a given frame depends on
/// the machine and what else it's doing, so a game must not fold this into its state.
pub fn poll(key: &str) -> Option<anyhow::Result<Arc<Outputs>>> {
    let job = pool().jobs.lock().unwrap().map.get(key).cloned()?;
    let s = job.state.lock().unwrap();
    match &*s {
        JobState::Done(r) => Some(r.clone().map_err(|e| anyhow!(e))),
        JobState::Running => None,
    }
}
