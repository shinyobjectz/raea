//! React on jitless V8 inside Bevy (engine/html/docs/bevy.feature, B2-B3). A JS thread owns the isolate and runs a bundle
//! (React, react-reconciler, bevy-react's bridge, the app); each React commit arrives as one op batch (bevy-react's
//! wire protocol) over a channel, and a Bevy system applies it to entities: every host element an entity, the tree as
//! Bevy's hierarchy, its kind, props and text as components. No deno_core: our V8 provides `__bevyHost` itself.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bevy::prelude::*;

pub mod animate;
/// ZeroHeap Z4: per-frame update ops read in place
mod rawops;
/// ZeroHeap Z1: the counting allocator raea-motion installs (RAEA_ZEROHEAP=1)
pub mod zeroheap;
pub mod audio;
pub mod bake;
/// `raea-motion ci ...`: the CI gates' sound measurements (bounce against live, clicks, ducking)
pub mod ci;
pub mod ecs;
/// exported games: `raea-motion export-game` and the `raea-game` player (sdk/react/docs/game-export.md)
pub mod game;
pub mod live;
pub mod motion;
pub mod play;
pub mod native;
use serde_json::Value;

const PRELUDE: &str = include_str!("../js/src/prelude.js");

/// The id React gives the root container (bevy-react's ROOT_ID).
pub const ROOT_ID: u64 = 0;

/// A React host element (or text node): its id on the wire.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReactNode(pub u64);

/// The element's kind as React named it (`div`, `span`, `mesh`, ...); text nodes are `#text`.
#[derive(Component, Debug, Clone)]
pub struct ReactKind(pub String);

/// Its props as last applied (deltas merged in).
#[derive(Component, Debug, Clone, Default)]
pub struct ReactProps(pub serde_json::Map<String, Value>);

/// Its text, for text nodes and single-string elements.
#[derive(Component, Debug, Clone)]
pub struct ReactText(pub String);

/// The React root: the entity every top-level element hangs from.
#[derive(Component, Debug)]
pub struct ReactRoot;

/// One commit's ops, as the bridge flushed them (JSON).
pub(crate) struct Batch(String);

#[derive(Resource)]
struct Inbox(Mutex<Receiver<Batch>>);

/// Bevy to React: events for the bridge's `op_next_event` (bevy-react's Outbound messages, as JSON).
#[derive(Resource)]
pub struct ReactOutbox(Mutex<Sender<String>>);

impl ReactOutbox {
    /// Any Outbound message (`{"t": "uiEvent", ...}`, `{"t": "elementEvent", ...}`, `{"t": "event", ...}`).
    pub fn send(&self, msg: Value) {
        let _ = self.0.lock().unwrap().send(msg.to_string());
    }
    /// A message already written as JSON
    pub fn send_raw(&self, msg: String) {
        let _ = self.0.lock().unwrap().send(msg);
    }
    /// A click on a React element, routed to its `onClick`.
    pub fn click(&self, node: ReactNode) {
        self.send(serde_json::json!({"t": "uiEvent", "event": {"id": node.0, "kind": "click"}}));
    }
}

/// Wire ids to entities.
#[derive(Resource, Default)]
pub struct ReactEntities {
    pub by_id: HashMap<u64, Entity>,
    /// how many commits and ops have been applied
    pub commits: usize,
    pub ops: usize,
}

/// Runs a React bundle on its own JS thread and applies its commits to the World each frame.
pub struct ReactPlugin {
    pub bundle: PathBuf,
}

impl ReactPlugin {
    pub fn new(bundle: impl Into<PathBuf>) -> Self {
        ReactPlugin { bundle: bundle.into() }
    }
}

/// Commit batches as they're applied, when present (bake.rs records a composition's commits by frame).
#[derive(Resource, Default)]
pub struct CommitTap(pub Vec<String>);

/// One commit applied per update, the rest held in order (a bake stepping a range of frames: React renders them back
/// to back, and each frame's commit is still applied, read and recorded on its own; lightspeed T3).
#[derive(Resource, Default)]
pub struct OneBatch(pub(crate) std::collections::VecDeque<Batch>);

/// React's place in the ECS with no JS behind it: recorded commits are fed in (ReplayFeed) and applied as React's
/// would be. A baked timeline plays through this, nothing of V8 running.
pub struct ReplayPlugin;

/// Where a replay's commit batches go in.
#[derive(Resource)]
pub struct ReplayFeed(Mutex<Sender<Batch>>);

impl ReplayFeed {
    pub fn feed(&self, json: String) {
        let _ = self.0.lock().unwrap().send(Batch(json));
    }
}

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = channel();
        // events have no one to go to: sent into a channel nobody reads
        let (out_tx, _) = channel::<String>();
        app.insert_resource(ReactOutbox(Mutex::new(out_tx))).insert_resource(ReplayFeed(Mutex::new(tx)));
        let root = app.world_mut().spawn((ReactRoot, ReactNode(ROOT_ID), ReactKind("#root".into()))).id();
        let mut ents = ReactEntities::default();
        ents.by_id.insert(ROOT_ID, root);
        app.insert_resource(Inbox(Mutex::new(rx))).insert_resource(ents).add_systems(PreUpdate, apply_commits);
        add_animate(app);
    }
}

/// Declared animations (motion's <Animate>, raea-animate) sampled at the frame clock once React's commits are in.
fn add_animate(app: &mut App) {
    app.add_plugins(raea_animate::bevy::AnimatePlugin).add_systems(PreUpdate, animate::sample_animations.after(apply_commits));
}

/// The tree under React's root as ops that build it from nothing (create, then append, parents first): a baked
/// timeline's keyframe, which a seek replays after removing what's there.
pub fn snapshot_ops(world: &World) -> Vec<Value> {
    let Some(&root) = world.get_resource::<ReactEntities>().and_then(|e| e.by_id.get(&ROOT_ID)) else { return Vec::new() };
    let mut out = Vec::new();
    fn walk(world: &World, e: Entity, out: &mut Vec<Value>) {
        let Some(kids) = world.get::<Children>(e) else { return };
        let parent = world.get::<ReactNode>(e).map(|n| n.0).unwrap_or(ROOT_ID);
        for k in kids.iter() {
            let Some(id) = world.get::<ReactNode>(k).map(|n| n.0) else { continue };
            let kind = world.get::<ReactKind>(k).map(|k| k.0.clone()).unwrap_or_default();
            let text = world.get::<ReactText>(k).map(|t| t.0.clone());
            match kind.as_str() {
                "#text" => out.push(serde_json::json!({"op": "createText", "id": id, "text": text.unwrap_or_default()})),
                "#span" => out.push(serde_json::json!({"op": "createTextSpan", "id": id, "text": text.unwrap_or_default()})),
                _ => {
                    let props = world.get::<ReactProps>(k).map(|p| Value::Object(p.0.clone())).unwrap_or(Value::Object(Default::default()));
                    let mut op = serde_json::json!({"op": "create", "id": id, "kind": kind, "props": props});
                    if let Some(t) = text {
                        op["text"] = Value::String(t);
                    }
                    out.push(op);
                }
            }
            walk(world, k, out);
            out.push(serde_json::json!({"op": "append", "parent": parent, "child": id}));
        }
    }
    walk(world, root, &mut out);
    out
}

impl Plugin for ReactPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = channel();
        let (out_tx, out_rx) = channel::<String>();
        let bundle = self.bundle.clone();
        std::thread::Builder::new()
            .name("raea-react js".into())
            .spawn(move || {
                // JavaScriptCore, the system's engine: renders page-identical to V8's (retired), the JIT on with the
                // entitlement tools/link-jit.sh signs in
                js_thread_jsc(bundle, tx, out_rx)
            })
            .expect("the JS thread");
        app.insert_resource(ReactOutbox(Mutex::new(out_tx)));
        let root = app.world_mut().spawn((ReactRoot, ReactNode(ROOT_ID), ReactKind("#root".into()))).id();
        let mut ents = ReactEntities::default();
        ents.by_id.insert(ROOT_ID, root);
        app.insert_resource(Inbox(Mutex::new(rx))).insert_resource(ents).add_systems(PreUpdate, apply_commits);
        add_animate(app);
    }
}


thread_local! {
    static FLUSH: std::cell::RefCell<Option<Sender<Batch>>> = const { std::cell::RefCell::new(None) };
    static T0: Instant = Instant::now();
}




/// Commits React has flushed so far, and a wake for whoever waits on the next (a render waits for its frame's commit
/// instead of polling with sleeps).
static FLUSHED: (Mutex<u64>, std::sync::Condvar) = (Mutex::new(0), std::sync::Condvar::new());

/// Where a frame's step goes: nanoseconds the JS thread ran (timers, events, microtasks), nanoseconds Bevy spent
/// applying commits (JSON parse, entities, the page mirror), and bytes of commit JSON sent. Read as deltas per frame.
pub static JS_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// This thread at macOS's user-interactive QoS (the class a game's frame threads get): under load from other processes a
/// default-QoS JS thread was preempted for ~30 ms at a time, and a 120 fps game missed frames (pong: 8 in 36,000 with a
/// load average of 4-10). Live play's JS thread and the thread that steps its frames take it.
pub fn interactive_thread() {
    #[cfg(target_vendor = "apple")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
        }
        const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
        unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
    }
}

/// Nanoseconds the JS thread spent giving JavaScriptCore's free pages back (RAEA_JSC_RELEASE_S).
pub static RELEASE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Nanoseconds the JS thread spent in young collections after frames (RAEA_JSC_EDEN), reported by the playtest.
pub static EDEN_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static APPLY_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FLUSH_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Rust allocations (count, bytes) applying React's ops on Bevy's side, and taking a flush's batch on the JS thread
/// (ZeroHeap Z1/Z4: RAEA_ZEROHEAP=1 under raea-motion; 0 otherwise)
pub static APPLY_ALLOCS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static APPLY_ALLOC_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static FLUSH_ALLOCS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many commits React has flushed.
/// RAEA_STARTUP_TRACE=<the spawner's clock, ms since the Unix epoch>: each mark printed with its time since the spawn
/// (lightspeed F5: where a process's time to its first frame goes). Unset, a mark costs one cached env read.
pub fn startup_mark(what: &str) {
    static SPAWN: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    if let Some(t0) = SPAWN.get_or_init(|| std::env::var("RAEA_STARTUP_TRACE").ok().and_then(|v| v.parse().ok())) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1e3).unwrap_or(0.0);
        eprintln!("raea startup: {:7.1} ms  {what}  (gpu {:.1} MB)", now - t0, raea::quads::gpu_allocated_mb().unwrap_or(-1.0));
    }
}

pub fn flushed() -> u64 {
    *FLUSHED.0.lock().unwrap()
}

/// Block until React flushes a commit after the `seen`-th, or `timeout` passes; the count now.
pub fn wait_flush(seen: u64, timeout: Duration) -> u64 {
    let (n, cv) = &FLUSHED;
    let t0 = Instant::now();
    let g = n.lock().unwrap();
    let (g, r) = cv.wait_timeout_while(g, timeout, |c| *c <= seen).unwrap();
    FLUSH_WAIT_NS.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    if r.timed_out() {
        FLUSH_TIMEOUTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    *g
}

/// Time spent waiting on React's flushes (wait_flush), in nanoseconds: what a bake's step waited on the JS thread
pub static FLUSH_WAIT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Waits for a flush that ran out their time (wait_flush): a step that waited on React for nothing
pub static FLUSH_TIMEOUTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ---- the JS thread on JavaScriptCore: the same prelude, shims, bundle and loop as on V8

#[cfg(target_vendor = "apple")]
mod jsc_host {
    use super::*;
    use raea_native::jsc;
    use std::ffi::c_void;

    type Ctx = *const c_void;
    type Obj = *mut c_void;
    type Val = *const c_void;

    pub(super) unsafe extern "C" fn flush(ctx: Ctx, _f: Obj, _t: Obj, argc: usize, argv: *const Val, _e: *mut Val) -> Val {
        let a0 = zeroheap::here().0;
        let json = unsafe { jsc::arg(ctx, argc, argv, 0) };
        let len = json.len() as u64;
        FLUSH.with(|f| {
            if let Some(tx) = f.borrow().as_ref() {
                let _ = tx.send(Batch(json));
            }
        });
        FLUSH_BYTES.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
        FLUSH_ALLOCS.fetch_add(zeroheap::here().0 - a0, std::sync::atomic::Ordering::Relaxed);
        let (n, cv) = &FLUSHED;
        *n.lock().unwrap() += 1;
        cv.notify_all();
        jsc::undefined(ctx)
    }

    pub(super) unsafe extern "C" fn log(ctx: Ctx, _f: Obj, _t: Obj, argc: usize, argv: *const Val, _e: *mut Val) -> Val {
        let level = unsafe { jsc::arg(ctx, argc, argv, 0) };
        let text = unsafe { jsc::arg(ctx, argc, argv, 1) };
        match level.as_str() {
            "2" => error!("[js] {text}"),
            "1" => warn!("[js] {text}"),
            _ => info!("[js] {text}"),
        }
        jsc::undefined(ctx)
    }

    pub(super) unsafe extern "C" fn now(ctx: Ctx, _f: Obj, _t: Obj, _argc: usize, _argv: *const Val, _e: *mut Val) -> Val {
        jsc::number(ctx, T0.with(|t| t.elapsed().as_secs_f64() * 1e3))
    }
}

/// The JS thread on JavaScriptCore: the prelude, the shims, the bundle, then timers and Bevy's events, as js_thread.
/// JSC runs promise jobs as each call returns, so there are no microtask checkpoints to make.
#[cfg(target_vendor = "apple")]
fn js_thread_jsc(bundle: PathBuf, tx: Sender<Batch>, events: Receiver<String>) {
    use raea_native::jsc::{self, Arg};
    FLUSH.with(|f| *f.borrow_mut() = Some(tx));
    let code = match std::fs::read_to_string(&bundle) {
        Ok(c) => c,
        Err(e) => {
            error!("raea-react: {}: {e}", bundle.display());
            return;
        }
    };
    interactive_thread();
    let ctx = jsc::Context::new();
    ctx.set_fn("__raea_flush", jsc_host::flush);
    ctx.set_fn("__raea_log", jsc_host::log);
    ctx.set_fn("__raea_now", jsc_host::now);
    native::register();
    for (src, name) in [(PRELUDE, "prelude.js")] {
        if let Err(e) = ctx.eval(src, name) {
            error!("raea-react: {name}: {e}");
            return;
        }
    }
    if let Err(e) = jsc::install(&ctx) {
        error!("raea-react: {e}");
        return;
    }
    if let Err(e) = ctx.eval(&code, "bundle.js") {
        error!("raea-react: bundle.js: {e}");
        return;
    }
    // RAEA_JSC_EDEN=N: a young collection after every Nth event's commit (1: each), at the frame's boundary, where the
    // JS thread waits for the next frame anyway, so the collections JavaScriptCore would start on allocation, inside a
    // frame's render, don't come
    let eden_every: u64 = std::env::var("RAEA_JSC_EDEN").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut delivered: u64 = 0;
    let release_every: f64 = std::env::var("RAEA_JSC_RELEASE_S").ok().and_then(|v| v.parse().ok()).unwrap_or(5.0);
    let mut released = Instant::now();
    loop {
        let busy = Instant::now();
        let now = T0.with(|t| t.elapsed().as_secs_f64() * 1e3);
        let next = match ctx.call_num("__raea_timers", &[Arg::Num(now)]) {
            None => break,
            Some(Ok(n)) if n.is_finite() => n,
            Some(Ok(_)) => -1.0,
            Some(Err(e)) => {
                error!("raea-react: a timer threw: {e}");
                -1.0
            }
        };
        JS_NS.fetch_add(busy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        let wait = if next < 0.0 { Duration::from_millis(500) } else { Duration::from_millis(next.clamp(0.0, 500.0) as u64) };
        let msg = match events.recv_timeout(wait) {
            Ok(json) => Some(json),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => None,
        };
        let busy = Instant::now();
        let arg = match &msg {
            Some(json) => Arg::Str(json),
            None => Arg::Null,
        };
        match ctx.call_num("__raea_deliver", &[arg]) {
            None => break,
            Some(Err(e)) => error!("raea-react: an event threw: {e}"),
            Some(Ok(_)) => {}
        }
        JS_NS.fetch_add(busy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        // a young collection after the frame when RAEA_JSC_EDEN is set (off: each cost ~0.5 ms and held memory no
        // flatter in pong; every 16th frame left 1 collection of 360 started by allocation inside a frame, where 71
        // were, at 2.4 ms of the JS thread each), then JavaScriptCore's GC timers, after the frame's commit is out (Bevy
        // applies and presents it meanwhile): the timers start most collections, on the collector thread, while the
        // JS thread waits for the next frame
        delivered += 1;
        if eden_every > 0 && delivered % eden_every == 0 {
            let t = Instant::now();
            ctx.collect_young();
            EDEN_NS.fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        jsc::pump();
        // every few seconds, JavaScriptCore's free pages back to the system (RAEA_JSC_RELEASE_S; 0 never)
        if release_every > 0.0 && released.elapsed().as_secs_f64() >= release_every {
            if let Some(ms) = jsc::release_free_memory() {
                RELEASE_NS.fetch_add((ms * 1e6) as u64, std::sync::atomic::Ordering::Relaxed);
            }
            released = Instant::now();
        }
        if msg.is_none() {
            break;
        }
    }
}






/// `map[k] = v`: an existing key's value set in place (a string's buffer reused), so a changed field doesn't allocate its
/// key again (ZeroHeap Z4); a new key inserted
fn put(map: &mut serde_json::Map<String, Value>, k: &str, v: &Value) {
    match map.get_mut(k) {
        Some(slot) => slot.clone_from(v),
        None => {
            map.insert(k.to_string(), v.clone());
        }
    }
}

fn merge(props: &mut serde_json::Map<String, Value>, delta: &Value, unset: Option<&Value>) {
    if let Some(d) = delta.as_object() {
        for (k, v) in d {
            if k == "style" {
                if let (Some(old), Some(new)) = (props.get_mut("style").and_then(|s| s.as_object_mut()), v.as_object()) {
                    for (sk, sv) in new {
                        put(old, sk, sv);
                    }
                    continue;
                }
            }
            put(props, k, v);
        }
    }
    for k in unset.and_then(|u| u.as_array()).into_iter().flatten().filter_map(|k| k.as_str()) {
        props.remove(k);
    }
}

/// Every commit since the last frame, in order: React's host elements become entities.
struct ApplyTimer(Instant, (u64, u64));
impl Drop for ApplyTimer {
    fn drop(&mut self) {
        APPLY_NS.fetch_add(self.0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        let (n, b) = zeroheap::here();
        APPLY_ALLOCS.fetch_add(n - self.1.0, std::sync::atomic::Ordering::Relaxed);
        APPLY_ALLOC_BYTES.fetch_add(b - self.1.1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A composition's thrown error, as its error boundary rendered it (data-raea-error), seen as its commit was applied:
/// settle reads it without scanning every entity.
pub(crate) static COMPOSITION_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn apply_commits(
    mut commands: Commands,
    inbox: Res<Inbox>,
    mut ents: ResMut<ReactEntities>,
    mut props: Query<&mut ReactProps>,
    mut texts: Query<&mut ReactText>,
    mut mirror: Option<NonSendMut<DomMirror>>,
    mut tap: Option<ResMut<CommitTap>>,
    mut one: Option<ResMut<OneBatch>>,
) {
    let t_apply = Instant::now();
    let _count = ApplyTimer(t_apply, zeroheap::here());
    let batches: Vec<Batch> = match one.as_mut() {
        Some(q) => {
            q.0.extend(inbox.0.lock().unwrap().try_iter());
            q.0.pop_front().into_iter().collect()
        }
        None => inbox.0.lock().unwrap().try_iter().collect(),
    };
    // props and text of entities spawned in this pass: commands are deferred, so an entity created here doesn't exist
    // for the queries until the pass ends, and an update to it in a later batch of the same pass was lost. A baked
    // seek feeds the keyframe (every node created) and the frames after it at once, and its videos came out a frame
    // behind (they read their source time from these props); the page mirror, applied directly, was right
    let mut fresh_props: HashMap<Entity, ReactProps> = HashMap::new();
    let mut fresh_text: HashMap<Entity, ReactText> = HashMap::new();
    for Batch(json) in batches {
        if let Some(t) = tap.as_mut() {
            t.0.push(json.clone());
        }
        // its updates read where they lie in the JSON, every other op parsed whole (rawops: ZeroHeap Z4)
        let Some(ops) = rawops::ops(&json) else {
            warn!("raea-react: a commit that isn't an op array");
            continue;
        };
        ents.commits += 1;
        if let Some(m) = mirror.as_deref_mut() {
            // a commit that sets what's there already (a component re-rendering each frame with settled values)
            // leaves the page as it was: nothing to style, lay out or paint again
            if mirror_ops_raw(m, &ops) {
                m.dirty = true;
            }
        }
        for opx in &ops {
            ents.ops += 1;
            let op = match opx {
                rawops::Op::Full(v) => v,
                rawops::Op::Update(u) => {
                    let mut error = None;
                    rawops::each_entry(u.props, |k, v| {
                        if k == "data-raea-error" {
                            error = serde_json::from_str::<String>(v.get()).ok();
                        }
                    });
                    if let Some(e) = error {
                        *COMPOSITION_ERROR.lock().unwrap() = Some(e);
                    }
                    if let Some(&e) = ents.by_id.get(&u.id) {
                        if let Some(p) = fresh_props.get_mut(&e) {
                            rawops::merge_update(&mut p.0, u);
                        } else if let Ok(mut p) = props.get_mut(e) {
                            rawops::merge_update(&mut p.0, u);
                        }
                    }
                    continue;
                }
            };
            if let Some(e) = op["props"].get("data-raea-error").and_then(|v| v.as_str()) {
                *COMPOSITION_ERROR.lock().unwrap() = Some(e.to_string());
            }
            let id = |k: &str| op[k].as_u64();
            match op["op"].as_str().unwrap_or("") {
                "reset" => {
                    let root = ents.by_id[&ROOT_ID];
                    for (i, e) in ents.by_id.drain() {
                        if i != ROOT_ID {
                            commands.entity(e).despawn();
                        }
                    }
                    ents.by_id.insert(ROOT_ID, root);
                }
                "create" => {
                    let mut p = ReactProps::default();
                    merge(&mut p.0, &op["props"], None);
                    let e = commands.spawn((ReactNode(id("id").unwrap_or(0)), ReactKind(op["kind"].as_str().unwrap_or("").into()))).id();
                    fresh_props.insert(e, p);
                    if let Some(t) = op["text"].as_str() {
                        fresh_text.insert(e, ReactText(t.into()));
                    }
                    ents.by_id.insert(id("id").unwrap_or(0), e);
                }
                "createText" | "createTextSpan" => {
                    let kind = if op["op"] == "createText" { "#text" } else { "#span" };
                    let e = commands.spawn((ReactNode(id("id").unwrap_or(0)), ReactKind(kind.into()))).id();
                    fresh_text.insert(e, ReactText(op["text"].as_str().unwrap_or("").into()));
                    ents.by_id.insert(id("id").unwrap_or(0), e);
                }
                "append" => {
                    if let (Some(p), Some(c)) = (id("parent").and_then(|i| ents.by_id.get(&i)), id("child").and_then(|i| ents.by_id.get(&i))) {
                        commands.entity(*p).add_child(*c);
                    }
                }
                "insert" => {
                    if let (Some(&p), Some(&c), Some(&b)) = (
                        id("parent").and_then(|i| ents.by_id.get(&i)),
                        id("child").and_then(|i| ents.by_id.get(&i)),
                        id("before").and_then(|i| ents.by_id.get(&i)),
                    ) {
                        commands.queue(move |world: &mut World| {
                            let kids: Vec<Entity> = world.get::<Children>(p).map(|ks| ks.iter().filter(|e| *e != c).collect()).unwrap_or_default();
                            let at = kids.iter().position(|e| *e == b).unwrap_or(kids.len());
                            world.entity_mut(p).insert_children(at, &[c]);
                        });
                    }
                }
                "remove" => {
                    if let Some(c) = id("child").and_then(|i| ents.by_id.remove(&i)) {
                        // the subtree's ids go with it (React never puts a removed node back): only the child's was
                        // forgotten, and a live game's map of ids grew with every node it ever removed
                        commands.queue(move |world: &mut World| {
                            let mut stack = vec![c];
                            let mut gone = Vec::new();
                            while let Some(e) = stack.pop() {
                                if let Some(kids) = world.get::<Children>(e) {
                                    stack.extend(kids.iter());
                                }
                                if let Some(n) = world.get::<ReactNode>(e) {
                                    gone.push((n.0, e));
                                }
                            }
                            let mut ents = world.resource_mut::<ReactEntities>();
                            for (i, e) in gone {
                                if ents.by_id.get(&i) == Some(&e) {
                                    ents.by_id.remove(&i);
                                }
                            }
                            if let Ok(em) = world.get_entity_mut(c) {
                                em.despawn();
                            }
                        });
                    }
                }
                "update" => {
                    if let Some(&e) = id("id").and_then(|i| ents.by_id.get(&i)) {
                        let mut apply = |p: &mut ReactProps| {
                            merge(&mut p.0, &op["props"], op.get("unset"));
                            // style keys React dropped (a transform going to nothing), as the DOM mirror does
                            if let (Some(style), Some(gone)) = (p.0.get_mut("style").and_then(|s| s.as_object_mut()), op["styleUnset"].as_array()) {
                                for k in gone.iter().filter_map(|k| k.as_str()) {
                                    style.remove(k);
                                }
                            }
                        };
                        if let Some(p) = fresh_props.get_mut(&e) {
                            apply(p);
                        } else if let Ok(mut p) = props.get_mut(e) {
                            apply(&mut p);
                        }
                    }
                }
                "updateText" => {
                    if let Some(&e) = id("id").and_then(|i| ents.by_id.get(&i)) {
                        let text = op["text"].as_str().unwrap_or("").to_string();
                        if let Some(t) = fresh_text.get_mut(&e) {
                            t.0 = text;
                        } else if let Ok(mut t) = texts.get_mut(e) {
                            t.0 = text;
                        }
                    }
                }
                other => warn!("raea-react: unknown op {other:?}"),
            }
        }
    }
    // this pass's new entities get their props and text as the last ops left them (an entity removed again in the same
    // pass is gone: try_insert skips it)
    for (e, p) in fresh_props {
        commands.entity(e).try_insert(p);
    }
    for (e, t) in fresh_text {
        commands.entity(e).try_insert(t);
    }
}

/// The tree under the React root as indented lines (kind, text, a few props): for tests and debugging.
pub fn dump(world: &mut World) -> String {
    let mut q = world.query_filtered::<Entity, With<ReactRoot>>();
    let Some(root) = q.iter(world).next() else { return String::new() };
    let mut out = String::new();
    fn walk(world: &World, e: Entity, depth: usize, out: &mut String) {
        let kind = world.get::<ReactKind>(e).map(|k| k.0.clone()).unwrap_or_default();
        let text = world.get::<ReactText>(e).map(|t| format!(" {:?}", t.0)).unwrap_or_default();
        let props = world.get::<ReactProps>(e).map(|p| {
            let keys: Vec<String> = p.0.iter().filter(|(k, _)| *k != "children").map(|(k, v)| format!("{k}={v}")).collect();
            if keys.is_empty() { String::new() } else { format!(" [{}]", keys.join(" ")) }
        }).unwrap_or_default();
        out.push_str(&format!("{}{kind}{text}{props}\n", "  ".repeat(depth)));
        if let Some(kids) = world.get::<Children>(e) {
            for k in kids.iter() {
                walk(world, k, depth + 1, out);
            }
        }
    }
    walk(world, root, 0, &mut out);
    out
}

// ---- the DOM mirror (bevy.feature B3, first cut): React's ops also drive a Blitz document ----

/// Kinds that are 3D or otherwise not DOM; they stay entities only.
const NOT_DOM: &[&str] = &["mesh", "light", "camera", "text3d", "surface", "#root"];

/// The page React's DOM entities are mirrored into: raea::dom (Stylo, Taffy, Parley, Vello behind it).
pub struct DomMirror {
    pub dom: raea::dom::Dom,
    /// the composition's frame rate, so a frame's number is its time on the page (at_frame)
    pub fps: f64,
    /// each timed clip's box (a <Sequence>'s: data-raea-start, its absolute start frame): its CSS animations run from
    /// there (Dom::set_animation_clips), handed to the page when one changes and at the first style
    clips: HashMap<raea::dom::NodeId, f64>,
    clips_dirty: bool,
    clips_set: bool,
    nodes: HashMap<u64, raea::dom::NodeId>,
    /// each page node's React id, so a removed subtree's ids can be forgotten
    ids: HashMap<raea::dom::NodeId, u64>,
    /// each node's props so far (update ops are deltas; style is written whole)
    props: HashMap<u64, serde_json::Map<String, Value>>,
    kinds: HashMap<u64, String>,
    /// each text node's text, so an update that sets the same text changes nothing
    texts: HashMap<u64, String>,
    /// React changed the page since the systems last ran
    pub dirty: bool,
    /// a rendered composition's page (raea-motion): the edit's folder, which `<img src>` resolves from; its
    /// `<surface>` is the page itself, a `<video>` is an image whose source the renderer sets each frame, and
    /// `<audio>` draws nothing
    pub page: Option<std::path::PathBuf>,
    /// a commit touched what decides the page's fonts (a <style> element's text, an inline font): the renderer loads
    /// fonts again only then (every frame, re-reading the whole stylesheet, cost 7 ms a frame)
    pub fonts_dirty: bool,
    /// each parent's children by React id, in React's order, those with no page node too (an `<audio>` on a rendered
    /// page): an insert before one of those goes before the next sibling that has a node
    order: HashMap<u64, Vec<u64>>,
}

impl DomMirror {
    pub fn new(width: u32, height: u32, dir: &std::path::Path) -> DomMirror {
        let mut dom = raea::dom::Dom::new(width, height, dir);
        // a page's body has no margin here (the comparison page sets it to 0 too)
        let body = dom.body();
        dom.set_attribute(body, "style", "margin: 0");
        let mut nodes = HashMap::new();
        nodes.insert(ROOT_ID, body);
        DomMirror { dom, fps: 30.0, clips: HashMap::new(), clips_dirty: false, clips_set: false, ids: nodes.iter().map(|(k, v)| (*v, *k)).collect(), nodes, props: HashMap::new(), kinds: HashMap::new(), texts: HashMap::new(), dirty: true, page: None, fonts_dirty: true, order: HashMap::new() }
    }

    /// The page styled at frame `f`'s time from now on (f / fps s): its CSS animations and transitions where they are
    /// at that frame. Render, bake and play set it as they step a frame.
    /// The clips' starts (s) handed to the page when they changed, and at the first call (where the page's elements
    /// then are taken as there from the start): before a frame is styled
    pub fn sync_clips(&mut self) {
        if self.page.is_some() && (!self.clips_set || self.clips_dirty) {
            let fps = self.fps.max(1e-9);
            let clips = self.clips.iter().map(|(n, s)| (*n, s / fps)).collect();
            self.dom.set_animation_clips(clips);
            self.clips_set = true;
            self.clips_dirty = false;
        }
    }

    /// Node `n`'s clip start (frames: a <Sequence>'s data-raea-start), or none
    fn set_clip(&mut self, n: raea::dom::NodeId, start: Option<f64>) {
        let changed = match start {
            Some(s) => self.clips.insert(n, s) != Some(s),
            None => self.clips.remove(&n).is_some(),
        };
        self.clips_dirty |= changed;
    }

    pub fn at_frame(&mut self, f: i64) {
        if f >= 0 && self.fps > 0.0 {
            self.dom.set_time(f as f64 / self.fps);
        }
    }

    /// A rendered composition's page (see `page`).
    pub fn new_page(width: u32, height: u32, dir: &std::path::Path) -> DomMirror {
        let mut m = DomMirror::new(width, height, dir);
        m.page = Some(dir.to_path_buf());
        m
    }

    /// The page node a React wire id is mirrored to.
    pub fn node(&self, id: u64) -> Option<raea::dom::NodeId> {
        self.nodes.get(&id).copied()
    }
}

/// CSS properties that take a bare number (React adds `px` to the rest).
const UNITLESS: &[&str] = &[
    "animationIterationCount", "aspectRatio", "borderImageOutset", "borderImageSlice", "borderImageWidth", "columnCount", "columns", "flex",
    "flexGrow", "flexPositive", "flexShrink", "flexNegative", "flexOrder", "gridArea", "gridRow", "gridRowEnd", "gridRowSpan", "gridRowStart",
    "gridColumn", "gridColumnEnd", "gridColumnSpan", "gridColumnStart", "fontWeight", "lineClamp", "lineHeight", "opacity", "order", "orphans",
    "scale", "tabSize", "widows", "zIndex", "zoom", "fillOpacity", "floodOpacity", "stopOpacity", "strokeDasharray", "strokeDashoffset",
    "strokeMiterlimit", "strokeOpacity", "strokeWidth",
];

/// A React style object as CSS text, the way react-dom writes it.
thread_local! {
    /// a moved box's left and top as CSS text, and its whole style's (mirror_update_raw's compositor-only motion)
    static MOVE_TEXT: std::cell::RefCell<(String, String, String)> = const { std::cell::RefCell::new((String::new(), String::new(), String::new())) };
}

/// A style length (left/top) as style_text writes it, into `out`: false for a value that isn't a number or a string
fn css_len(v: Option<&Value>, out: &mut String) -> bool {
    use std::fmt::Write as _;
    out.clear();
    match v {
        Some(Value::Number(n)) if n.as_f64() != Some(0.0) => {
            let _ = write!(out, "{n}px");
        }
        Some(Value::Number(n)) => {
            let _ = write!(out, "{n}");
        }
        Some(Value::String(s)) => out.push_str(s),
        _ => return false,
    }
    true
}

/// A style object set as the element's style attribute, its text written into a buffer kept per thread (ZeroHeap: a
/// String a style update)
fn set_style(dom: &mut raea::dom::Dom, node: raea::dom::NodeId, style: &serde_json::Map<String, Value>) {
    thread_local! {
        static BUF: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    BUF.with_borrow_mut(|b| {
        b.clear();
        style_text_into(style, b);
        dom.set_attribute(node, "style", b);
    })
}

fn style_text_into(style: &serde_json::Map<String, Value>, out: &mut String) {
    use std::fmt::Write as _;
    // written into one string (ZeroHeap Z4: a style update made a string per field, a Vec and the join)
    for (k, v) in style.iter().filter(|(_, v)| !v.is_null()) {
        if !out.is_empty() {
            out.push_str("; ");
        }
        // a custom property (--brand-scale) keeps its name and takes a number as it is, as react-dom writes it
        let custom = k.starts_with("--");
        if custom {
            out.push_str(k);
        } else {
            for ch in k.chars() {
                if ch.is_ascii_uppercase() {
                    out.push('-');
                    out.push(ch.to_ascii_lowercase());
                } else {
                    out.push(ch);
                }
            }
        }
        out.push_str(": ");
        match v {
            Value::Number(n) if !custom && !UNITLESS.contains(&k.as_str()) && n.as_f64() != Some(0.0) => {
                let _ = write!(out, "{n}px");
            }
            Value::String(s) => out.push_str(s),
            other => {
                let _ = write!(out, "{other}");
            }
        }
    }
}

/// A prop's attribute name, as react-dom names it: its aliases (strokeWidth is stroke-width, htmlFor is for: the SVG
/// presentation attributes Blitz reads by their own names) and the xlink:/xml: attributes; any other prop is its name
/// (viewBox and the SVG attributes that are camelCase themselves included)
fn attr_name(k: &str) -> &str {
    // react-dom-client's getAttributeAlias map, then its xlink*/xml* cases; sorted, for the binary search
    const ALIASES: &[(&str, &str)] = &[
    ("accentHeight", "accent-height"), ("acceptCharset", "accept-charset"),
    ("alignmentBaseline", "alignment-baseline"), ("arabicForm", "arabic-form"), ("baselineShift", "baseline-shift"),
    ("capHeight", "cap-height"), ("clipPath", "clip-path"), ("clipRule", "clip-rule"),
    ("colorInterpolation", "color-interpolation"), ("colorInterpolationFilters", "color-interpolation-filters"),
    ("colorProfile", "color-profile"), ("colorRendering", "color-rendering"), ("crossOrigin", "crossorigin"),
    ("dominantBaseline", "dominant-baseline"), ("enableBackground", "enable-background"),
    ("fillOpacity", "fill-opacity"), ("fillRule", "fill-rule"), ("floodColor", "flood-color"),
    ("floodOpacity", "flood-opacity"), ("fontFamily", "font-family"), ("fontSize", "font-size"),
    ("fontSizeAdjust", "font-size-adjust"), ("fontStretch", "font-stretch"), ("fontStyle", "font-style"),
    ("fontVariant", "font-variant"), ("fontWeight", "font-weight"), ("glyphName", "glyph-name"),
    ("glyphOrientationHorizontal", "glyph-orientation-horizontal"),
    ("glyphOrientationVertical", "glyph-orientation-vertical"), ("horizAdvX", "horiz-adv-x"),
    ("horizOriginX", "horiz-origin-x"), ("htmlFor", "for"), ("httpEquiv", "http-equiv"),
    ("imageRendering", "image-rendering"), ("letterSpacing", "letter-spacing"), ("lightingColor", "lighting-color"),
    ("markerEnd", "marker-end"), ("markerMid", "marker-mid"), ("markerStart", "marker-start"),
    ("overlinePosition", "overline-position"), ("overlineThickness", "overline-thickness"),
    ("paintOrder", "paint-order"), ("panose-1", "panose-1"), ("pointerEvents", "pointer-events"),
    ("renderingIntent", "rendering-intent"), ("shapeRendering", "shape-rendering"), ("stopColor", "stop-color"),
    ("stopOpacity", "stop-opacity"), ("strikethroughPosition", "strikethrough-position"),
    ("strikethroughThickness", "strikethrough-thickness"), ("strokeDasharray", "stroke-dasharray"),
    ("strokeDashoffset", "stroke-dashoffset"), ("strokeLinecap", "stroke-linecap"),
    ("strokeLinejoin", "stroke-linejoin"), ("strokeMiterlimit", "stroke-miterlimit"),
    ("strokeOpacity", "stroke-opacity"), ("strokeWidth", "stroke-width"), ("textAnchor", "text-anchor"),
    ("textDecoration", "text-decoration"), ("textRendering", "text-rendering"),
    ("transformOrigin", "transform-origin"), ("underlinePosition", "underline-position"),
    ("underlineThickness", "underline-thickness"), ("unicodeBidi", "unicode-bidi"), ("unicodeRange", "unicode-range"),
    ("unitsPerEm", "units-per-em"), ("vAlphabetic", "v-alphabetic"), ("vHanging", "v-hanging"),
    ("vIdeographic", "v-ideographic"), ("vMathematical", "v-mathematical"), ("vectorEffect", "vector-effect"),
    ("vertAdvY", "vert-adv-y"), ("vertOriginX", "vert-origin-x"), ("vertOriginY", "vert-origin-y"),
    ("wordSpacing", "word-spacing"), ("writingMode", "writing-mode"), ("xHeight", "x-height"),
    ("xlinkActuate", "xlink:actuate"), ("xlinkArcrole", "xlink:arcrole"), ("xlinkHref", "xlink:href"),
    ("xlinkRole", "xlink:role"), ("xlinkShow", "xlink:show"), ("xlinkTitle", "xlink:title"),
    ("xlinkType", "xlink:type"), ("xmlBase", "xml:base"), ("xmlLang", "xml:lang"), ("xmlSpace", "xml:space"),
    ("xmlnsXlink", "xmlns:xlink"),
    ];
    match ALIASES.binary_search_by(|(a, _)| (*a).cmp(k)) {
        Ok(i) => ALIASES[i].1,
        Err(_) => k,
    }
}

/// Props as attributes, the way react-dom sets them: className is class, style is CSS text, others by attr_name; handlers, children
/// and keys aren't attributes.
pub(crate) fn apply_props(dom: &mut raea::dom::Dom, node: raea::dom::NodeId, props: &serde_json::Map<String, Value>) {
    for (k, v) in props {
        if k == "children" || k == "key" || (k.starts_with("on") && k.chars().nth(2).is_some_and(|c| c.is_ascii_uppercase())) {
            continue;
        }
        match (k.as_str(), v) {
            ("style", Value::Object(o)) => set_style(dom, node, o),
            ("className", Value::String(s)) => dom.set_attribute(node, "class", s),
            (_, Value::String(s)) => dom.set_attribute(node, attr_name(k), s),
            (_, Value::Number(n)) => dom.set_attribute(node, attr_name(k), &n.to_string()),
            (_, Value::Bool(true)) => dom.set_attribute(node, attr_name(k), ""),
            _ => {}
        }
    }
}

/// A node's props as a rendered page draws them: a video's own source and data attributes stay off the image the
/// renderer fills each frame; an image's relative source resolves from the edit's folder.
fn page_props(page: &Option<std::path::PathBuf>, kind: &str, p: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let mut p = p.clone();
    // a 3D part's description (read from the entities by the 3D: motion::read3d) draws nothing on the page
    if p.contains_key("data-3d") {
        p.remove("data-props");
        p.remove("data-shape");
    }
    let Some(dir) = page else { return p };
    match kind {
        "video" => p.retain(|k, _| !k.starts_with("data-") && k != "src"),
        "img" => {
            if let Some(src) = p.get("src").and_then(|v| v.as_str()).map(str::to_string) {
                if !src.contains("://") {
                    let rel = src.trim_start_matches('/');
                    let f = if std::path::Path::new(&src).is_absolute() && std::path::Path::new(&src).exists() { std::path::PathBuf::from(&src) } else { dir.join(rel) };
                    p.insert("src".into(), Value::String(format!("file://{}", f.display())));
                }
            }
        }
        _ => {}
    }
    p
}

/// One prop of a node onto its page node, as page_props then apply_props would set it (`all` is the node's props,
/// for the data-3d check): an update sets only what it changed.
fn apply_prop(dom: &mut raea::dom::Dom, node: raea::dom::NodeId, page: &Option<std::path::PathBuf>, kind: &str, all: &serde_json::Map<String, Value>, k: &str, v: &Value) {
    if k == "children" || k == "key" || (k.starts_with("on") && k.chars().nth(2).is_some_and(|c| c.is_ascii_uppercase())) {
        return;
    }
    if all.contains_key("data-3d") && (k == "data-props" || k == "data-shape") {
        return;
    }
    // the frame marker's bookkeeping, read from React's props (settle, animate.rs), never from the page: written as
    // attributes each frame, the marker's parent (the whole composition) was restyled every frame
    if matches!(k, "data-raea-frame" | "data-raea-config" | "data-raea-done") {
        return;
    }
    if let Some(dir) = page {
        if kind == "video" && (k.starts_with("data-") || k == "src") {
            return;
        }
        if kind == "img" && k == "src" {
            if let Some(src) = v.as_str().filter(|s| !s.contains("://")) {
                let rel = src.trim_start_matches('/');
                let f = if std::path::Path::new(src).is_absolute() && std::path::Path::new(src).exists() { std::path::PathBuf::from(src) } else { dir.join(rel) };
                dom.set_attribute(node, "src", &format!("file://{}", f.display()));
                return;
            }
        }
    }
    match (k, v) {
        ("style", Value::Object(o)) => set_style(dom, node, o),
        ("className", Value::String(s)) => dom.set_attribute(node, "class", s),
        (_, Value::String(s)) => dom.set_attribute(node, attr_name(k), s),
        (_, Value::Number(n)) => dom.set_attribute(node, attr_name(k), &n.to_string()),
        (_, Value::Bool(true)) => dom.set_attribute(node, attr_name(k), ""),
        _ => {}
    }
}

/// mirror_ops over a batch read by rawops: an update read in place, any other op as before; true when any changed the page
fn mirror_ops_raw(m: &mut DomMirror, ops: &[rawops::Op]) -> bool {
    let mut changed = false;
    for op in ops {
        changed |= match op {
            rawops::Op::Full(v) => mirror_ops(m, std::slice::from_ref(v)),
            rawops::Op::Update(u) => mirror_update_raw(m, u),
        };
    }
    changed
}

/// The mirror's update (mirror_ops' "update"), read in place: merged key by key, the same tests for nothing new, a shown
/// key and a 3D part's description, then only the changed attributes set (in two passes over the delta: the first
/// merges and marks which entries changed, up to 64; the second sets them)
fn mirror_update_raw(m: &mut DomMirror, u: &rawops::Update) -> bool {
    // what can bring a new font: a stylesheet, or an inline font property that changed. An update carries the whole
    // style object, so a caption whose letters' opacity moved re-sent its font-family every frame, and each frame then
    // hashed every stylesheet for nothing (the Rome film's 1.5 MB of inlined faces, ~1 ms a frame)
    let mut font = u.kind == "style";
    let Some(n) = m.nodes.get(&u.id).copied() else {
        rawops::each_entry(u.props, |k, v| {
            if k == "style" && rawops::is_object(v) {
                rawops::each_entry(v, |sk, _| font |= sk.starts_with("font"));
            }
        });
        m.fonts_dirty |= font;
        return false;
    };
    let all = m.props.entry(u.id).or_default();
    let hidden = |k: &str| matches!(k, "data-raea-frame" | "data-raea-config" | "data-raea-done");
    let undrawn = |k: &str| matches!(k, "data-props" | "data-shape");
    let (mut any, mut shown, mut drawn, mut style, mut three) = (false, false, false, false, false);
    let mut note = |k: &str, any: &mut bool, shown: &mut bool, drawn: &mut bool, three: &mut bool| {
        *any = true;
        *shown |= !hidden(k);
        *drawn |= !undrawn(k);
        *three |= k == "data-3d";
    };
    let (mut bits, mut nk) = (0u64, 0usize);
    // which style keys changed: only left/top is a move (Dom::move_box), anything else a restyle
    let (mut moved_left, mut moved_top, mut restyled) = (false, false, false);
    rawops::each_entry(u.props, |k, v| {
        let i = nk;
        nk += 1;
        if k == "style" && rawops::is_object(v) {
            if let Some(old) = all.get_mut("style").and_then(|s| s.as_object_mut()) {
                rawops::each_entry(v, |sk, sv| {
                    if rawops::put_raw(old, sk, sv) {
                        style = true;
                        font |= sk.starts_with("font");
                        match sk {
                            "left" => moved_left = true,
                            "top" => moved_top = true,
                            _ => restyled = true,
                        }
                    }
                });
                return;
            }
        }
        if rawops::put_raw(all, k, v) {
            // a style object where there was none: any font property in it is new
            if k == "style" && rawops::is_object(v) {
                rawops::each_entry(v, |sk, _| font |= sk.starts_with("font"));
            }
            note(k, &mut any, &mut shown, &mut drawn, &mut three);
            if i < 64 {
                bits |= 1 << i;
            }
        }
    });
    if let Some(un) = u.unset {
        rawops::each_str(un, |k| {
            if all.remove(k).is_some() {
                note(k, &mut any, &mut shown, &mut drawn, &mut three);
            }
        });
    }
    if let (Some(st), Some(gone)) = (all.get_mut("style").and_then(|s| s.as_object_mut()), u.style_unset) {
        rawops::each_str(gone, |k| {
            if st.remove(k).is_some() {
                style = true;
                restyled = true;
            }
        });
    }
    // nothing but its left/top changed: moved where it's laid out, not restyled (compositor-only motion)
    let only_moved = style && !any && !restyled && (moved_left || moved_top);
    if style {
        note("style", &mut any, &mut shown, &mut drawn, &mut three);
    }
    if !any {
        return false;
    }
    // its clip start (a <Sequence>'s box), fields apart from the props this still borrows
    let clip = all.get("data-raea-start").and_then(Value::as_f64);
    m.fonts_dirty |= font;
    m.clips_dirty |= match clip {
        Some(s) => m.clips.insert(n, s) != Some(s),
        None => m.clips.remove(&n).is_some(),
    };
    // only a 3D part's description changed (every frame a game moves it): nothing on the page changes
    if all.contains_key("data-3d") && !drawn {
        return false;
    }
    if let Some(un) = u.unset {
        rawops::each_str(un, |k| m.dom.clear_attribute(n, if k == "className" { "class" } else { k }));
    }
    if only_moved && u.unset.is_none() {
        if let Some(st) = all.get("style").and_then(Value::as_object) {
            let done = MOVE_TEXT.with_borrow_mut(|(l, t, text)| {
                let (lt, tt) = (moved_left && css_len(st.get("left"), l), moved_top && css_len(st.get("top"), t));
                if (moved_left && !lt) || (moved_top && !tt) {
                    return false;
                }
                text.clear();
                style_text_into(st, text);
                m.dom.move_box(n, lt.then_some(l.as_str()), tt.then_some(t.as_str()), text)
            });
            if done {
                return shown;
            }
        }
    }
    let kind = m.kinds.get(&u.id).map(String::as_str).unwrap_or("");
    // more keys than the bits hold, or data-3d itself changed (what page_props drops depends on it): every prop
    if nk > 64 || three {
        apply_props(&mut m.dom, n, &page_props(&m.page, kind, all));
    } else {
        let mut i = 0usize;
        rawops::each_entry(u.props, |k, _| {
            if bits & (1 << i) != 0 {
                if let Some(v) = all.get(k) {
                    apply_prop(&mut m.dom, n, &m.page, kind, all, k, v);
                }
            }
            i += 1;
        });
        if style {
            if let Some(v) = all.get("style") {
                apply_prop(&mut m.dom, n, &m.page, kind, all, "style", v);
            }
        }
    }
    shown
}

/// Apply one commit's ops to the mirror (in order, like the entities).
/// React's ops onto the mirrored page; true when any of them changed it.
pub fn mirror_ops(m: &mut DomMirror, ops: &[Value]) -> bool {
    let mut changed = false;
    for op in ops {
        // structure always changes the page; updates only when they set something new (checked below)
        if !matches!(op["op"].as_str(), Some("update" | "updateText") | None) {
            changed = true;
        }
        // what can bring a new font: a stylesheet, or an inline font property
        let touches_font = |p: &Value| p["style"].as_object().is_some_and(|st| st.keys().any(|k| k.starts_with("font")));
        if op["kind"].as_str() == Some("style") || touches_font(&op["props"]) || (op["op"] == "updateText" && op["id"].as_u64().is_some_and(|i| m.kinds.get(&i).is_some_and(|k| k == "style"))) {
            m.fonts_dirty = true;
        }
        let id = |k: &str| op[k].as_u64();
        match op["op"].as_str().unwrap_or("") {
            "create" => {
                let (Some(i), Some(kind)) = (id("id"), op["kind"].as_str()) else { continue };
                m.kinds.insert(i, kind.to_string());
                // a rendered page: the surface is the page, a video an image, sound draws nothing
                if m.page.is_some() && kind == "surface" {
                    let n = m.dom.create_element("div");
                    m.dom.set_attribute(n, "style", "position: absolute; inset: 0; overflow: hidden");
                    m.nodes.insert(i, n);
                m.ids.insert(n, i);
                    continue;
                }
                if NOT_DOM.contains(&kind) || (m.page.is_some() && kind == "audio") {
                    continue;
                }
                let n = m.dom.create_element(if m.page.is_some() && kind == "video" { "img" } else { kind });
                if let Some(p) = op["props"].as_object() {
                    apply_props(&mut m.dom, n, &page_props(&m.page, kind, p));
                    if let Some(s) = p.get("data-raea-start").and_then(Value::as_f64) {
                        m.set_clip(n, Some(s));
                    }
                    m.props.insert(i, p.clone());
                }
                if let Some(t) = op["text"].as_str() {
                    let tn = m.dom.create_text(t);
                    m.dom.append(n, tn);
                }
                m.nodes.insert(i, n);
                m.ids.insert(n, i);
            }
            "createText" | "createTextSpan" => {
                let Some(i) = id("id") else { continue };
                let text = op["text"].as_str().unwrap_or("");
                let n = m.dom.create_text(text);
                m.nodes.insert(i, n);
                m.ids.insert(n, i);
                m.texts.insert(i, text.to_string());
                changed = true;
            }
            "append" => {
                if let (Some(pi), Some(ci)) = (id("parent"), id("child")) {
                    for kids in m.order.values_mut() {
                        kids.retain(|k| *k != ci);
                    }
                    m.order.entry(pi).or_default().push(ci);
                }
                if let (Some(p), Some(c)) = (id("parent").and_then(|i| m.nodes.get(&i).copied()), id("child").and_then(|i| m.nodes.get(&i).copied())) {
                    m.dom.append(p, c);
                }
            }
            "insert" => {
                let (Some(pi), Some(ci), Some(bi)) = (id("parent"), id("child"), id("before")) else { continue };
                for kids in m.order.values_mut() {
                    kids.retain(|k| *k != ci);
                }
                let kids = m.order.entry(pi).or_default();
                let at = kids.iter().position(|k| *k == bi).unwrap_or(kids.len());
                kids.insert(at, ci);
                // before the first sibling after it that has a page node (`before` itself may have none: an <audio> on
                // a rendered page; the lite runtime inserts before one, React happened not to), else at the end
                let anchor = kids[at + 1..].iter().find_map(|k| m.nodes.get(k).copied());
                if let Some(c) = m.nodes.get(&ci).copied() {
                    match (anchor, m.nodes.get(&pi).copied()) {
                        (Some(b), _) => m.dom.insert_before(b, c),
                        (None, Some(p)) => m.dom.append(p, c),
                        (None, None) => {}
                    }
                }
            }
            "remove" => {
                // React never puts a removed node back: it and its subtree are freed, and their ids forgotten (only
                // detached, a live game's page grew ~100 KB a frame)
                if let (Some(pi), Some(ci)) = (id("parent"), id("child")) {
                    if let Some(kids) = m.order.get_mut(&pi) {
                        kids.retain(|k| *k != ci);
                    }
                    // the subtree's own child lists go with it
                    let mut stack = vec![ci];
                    while let Some(k) = stack.pop() {
                        if let Some(kids) = m.order.remove(&k) {
                            stack.extend(kids);
                        }
                    }
                }
                if let Some(c) = id("child").and_then(|i| m.nodes.get(&i).copied()) {
                    for n in m.dom.drop(c) {
                        m.set_clip(n, None);
                        if let Some(i) = m.ids.remove(&n) {
                            m.nodes.remove(&i);
                            m.props.remove(&i);
                            m.kinds.remove(&i);
                            m.texts.remove(&i);
                        }
                    }
                }
            }
            "update" => {
                let Some(i) = id("id") else { continue };
                let Some(n) = m.nodes.get(&i).copied() else { continue };
                let all = m.props.entry(i).or_default();
                // merge's effect, key by key, without copying the props (ZeroHeap Z4: an update cloned the node's props
                // four times to compare them): which keys this delta changed, and whether a drawn or shown one did
                let hidden = |k: &str| matches!(k, "data-raea-frame" | "data-raea-config" | "data-raea-done");
                let undrawn = |k: &str| matches!(k, "data-props" | "data-shape");
                let (mut any, mut shown, mut drawn, mut style) = (false, false, false, false);
                let mut note = |k: &str| {
                    any = true;
                    shown |= !hidden(k);
                    drawn |= !undrawn(k);
                };
                // the keys to set again, up to 32 (more set every prop, as before)
                let mut keys: [&str; 32] = [""; 32];
                let mut nk = 0usize;
                let delta = op["props"].as_object();
                for (k, v) in delta.into_iter().flatten() {
                    if k == "style" {
                        if let (Some(old), Some(new)) = (all.get_mut("style").and_then(|s| s.as_object_mut()), v.as_object()) {
                            for (sk, sv) in new {
                                if old.get(sk) != Some(sv) {
                                    put(old, sk, sv);
                                    style = true;
                                }
                            }
                            continue;
                        }
                    }
                    if all.get(k) != Some(v) {
                        put(all, k, v);
                        note(k);
                        if nk < keys.len() {
                            keys[nk] = k;
                        }
                        nk += 1;
                    }
                }
                let unset = op.get("unset");
                for k in unset.and_then(|u| u.as_array()).into_iter().flatten().filter_map(|k| k.as_str()) {
                    if all.remove(k).is_some() {
                        note(k);
                    }
                }
                if let (Some(st), Some(gone)) = (all.get_mut("style").and_then(|s| s.as_object_mut()), op["styleUnset"].as_array()) {
                    for k in gone.iter().filter_map(|k| k.as_str()) {
                        if st.remove(k).is_some() {
                            style = true;
                        }
                    }
                }
                if style {
                    note("style");
                }
                if !any {
                    continue;
                }
                // its clip start (a <Sequence>'s box), fields apart from the props this still borrows
                let clip = all.get("data-raea-start").and_then(Value::as_f64);
                m.clips_dirty |= match clip {
                    Some(s) => m.clips.insert(n, s) != Some(s),
                    None => m.clips.remove(&n).is_some(),
                };
                // only a 3D part's description changed (every frame a game moves it): nothing on the page changes
                if all.contains_key("data-3d") && !drawn {
                    continue;
                }
                if shown {
                    changed = true;
                }
                for k in unset.and_then(|u| u.as_array()).into_iter().flatten().filter_map(|k| k.as_str()) {
                    m.dom.clear_attribute(n, if k == "className" { "class" } else { k });
                }
                let kind = m.kinds.get(&i).map(String::as_str).unwrap_or("");
                // more keys than the list holds, or data-3d itself changed (what page_props drops depends on it): every prop
                if nk > keys.len() || keys[..nk.min(keys.len())].contains(&"data-3d") || unset.and_then(|u| u.as_array()).is_some_and(|u| u.iter().any(|k| k == "data-3d")) {
                    apply_props(&mut m.dom, n, &page_props(&m.page, kind, all));
                } else {
                    for k in &keys[..nk] {
                        if let Some(v) = all.get(*k) {
                            apply_prop(&mut m.dom, n, &m.page, kind, all, k, v);
                        }
                    }
                    if style {
                        if let Some(v) = all.get("style") {
                            apply_prop(&mut m.dom, n, &m.page, kind, all, "style", v);
                        }
                    }
                }
            }
            "updateText" => {
                let (Some(i), text) = (id("id"), op["text"].as_str().unwrap_or("")) else { continue };
                if m.texts.get(&i).is_some_and(|t| t == text) {
                    continue;
                }
                if let Some(n) = m.nodes.get(&i).copied() {
                    m.dom.set_text(n, text);
                    m.texts.insert(i, text.to_string());
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

// ---- style, layout, text and paint as Bevy systems (bevy.feature B3) ----

/// A DOM entity's node in the page: Blitz's arena is the style and layout storage behind the entities.
#[derive(Component, Debug, Clone, Copy)]
pub struct DomNode(pub raea::dom::NodeId);

/// The page's last picture (RGBA8 over white) and what each system cost the last time it ran.
#[derive(Resource, Default)]
pub struct DomFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// frames the page was re-resolved and repainted on (React changed it)
    pub painted: usize,
    pub style_ms: f64,
    pub layout_ms: f64,
    pub raster_ms: f64,
}

/// React's DOM host elements as a page: the DomMirror (non-send: Blitz's document isn't Send) fed by apply_commits,
/// and three systems after it, each over what changed: Stylo (style), Taffy + Parley (layout and text), Vello (raster).
/// They run only on frames React changed the page; Blitz's damage flags keep each to the changed subtrees.
pub struct DomPlugin {
    pub width: u32,
    pub height: u32,
    pub dir: PathBuf,
}

impl Plugin for DomPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(DomMirror::new(self.width, self.height, &self.dir));
        app.insert_resource(DomFrame { width: self.width, height: self.height, ..Default::default() });
        app.add_systems(PreUpdate, tag_dom_nodes.after(apply_commits));
        app.add_systems(Update, (dom_style, dom_layout, dom_raster).chain().run_if(page_changed));
    }
}

/// The page changed and nothing else paints it: a render's retained page (`page` set) is painted by its own snapshot,
/// so painting it here too drew every frame twice (2026-10-17: 1190 paints for 595 frames).
fn page_changed(m: Option<NonSend<DomMirror>>) -> bool {
    m.is_some_and(|m| (m.dirty || m.dom.is_animating()) && m.page.is_none())
}

/// Every React entity with a page node gets its DomNode.
fn tag_dom_nodes(mut commands: Commands, m: NonSend<DomMirror>, q: Query<(Entity, &ReactNode), Without<DomNode>>) {
    for (e, n) in &q {
        if let Some(id) = m.node(n.0) {
            commands.entity(e).insert(DomNode(id));
        }
    }
}

fn dom_style(mut m: NonSendMut<DomMirror>, mut f: ResMut<DomFrame>) {
    let t0 = Instant::now();
    m.sync_clips();
    let t = m.dom.time();
    m.dom.style(t);
    f.style_ms = t0.elapsed().as_secs_f64() * 1e3;
}

fn dom_layout(mut m: NonSendMut<DomMirror>, mut f: ResMut<DomFrame>) {
    let t0 = Instant::now();
    let t = m.dom.time();
    m.dom.layout(t);
    f.layout_ms = t0.elapsed().as_secs_f64() * 1e3;
}

fn dom_raster(mut m: NonSendMut<DomMirror>, mut f: ResMut<DomFrame>) {
    let t0 = Instant::now();
    let mut rgba = std::mem::take(&mut f.rgba);
    m.dom.raster(&mut rgba);
    f.rgba = rgba;
    f.raster_ms = t0.elapsed().as_secs_f64() * 1e3;
    f.painted += 1;
    m.dirty = false;
}
