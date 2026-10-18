//! React on jitless V8 inside Bevy (engine/docs/bevy.feature, B2-B3). A JS thread owns the isolate and runs a bundle
//! (React, react-reconciler, bevy-react's bridge, the app); each React commit arrives as one op batch (bevy-react's
//! wire protocol) over a channel, and a Bevy system applies it to entities: every host element an entity, the tree as
//! Bevy's hierarchy, its kind, props and text as components. No deno_core: our V8 provides `__bevyHost` itself.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bevy::prelude::*;

pub mod audio;
pub mod bake;
pub mod ecs;
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
    }
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

/// How many commits React has flushed.
pub fn flushed() -> u64 {
    *FLUSHED.0.lock().unwrap()
}

/// Block until React flushes a commit after the `seen`-th, or `timeout` passes; the count now.
pub fn wait_flush(seen: u64, timeout: Duration) -> u64 {
    let (n, cv) = &FLUSHED;
    let g = n.lock().unwrap();
    let (g, _) = cv.wait_timeout_while(g, timeout, |c| *c <= seen).unwrap();
    *g
}

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
        let json = unsafe { jsc::arg(ctx, argc, argv, 0) };
        let len = json.len() as u64;
        FLUSH.with(|f| {
            if let Some(tx) = f.borrow().as_ref() {
                let _ = tx.send(Batch(json));
            }
        });
        FLUSH_BYTES.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
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
    let eden = std::env::var("RAEA_JSC_EDEN").as_deref() == Ok("1");
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
        // a young collection after the frame when RAEA_JSC_EDEN=1 (it cost ~0.5 ms a frame and held memory no flatter in
        // pong), then JavaScriptCore's GC timers, after
        // the frame's commit is out (Bevy applies and presents it meanwhile)
        if eden {
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






fn merge(props: &mut serde_json::Map<String, Value>, delta: &Value, unset: Option<&Value>) {
    if let Some(d) = delta.as_object() {
        for (k, v) in d {
            if k == "style" {
                if let (Some(old), Some(new)) = (props.get_mut("style").and_then(|s| s.as_object_mut()), v.as_object()) {
                    for (sk, sv) in new {
                        old.insert(sk.clone(), sv.clone());
                    }
                    continue;
                }
            }
            props.insert(k.clone(), v.clone());
        }
    }
    for k in unset.and_then(|u| u.as_array()).into_iter().flatten().filter_map(|k| k.as_str()) {
        props.remove(k);
    }
}

/// Every commit since the last frame, in order: React's host elements become entities.
struct ApplyTimer(Instant);
impl Drop for ApplyTimer {
    fn drop(&mut self) {
        APPLY_NS.fetch_add(self.0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
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
) {
    let t_apply = Instant::now();
    let _count = ApplyTimer(t_apply);
    let batches: Vec<Batch> = inbox.0.lock().unwrap().try_iter().collect();
    for Batch(json) in batches {
        if let Some(t) = tap.as_mut() {
            t.0.push(json.clone());
        }
        let Ok(Value::Array(ops)) = serde_json::from_str::<Value>(&json) else {
            warn!("raea-react: a commit that isn't an op array");
            continue;
        };
        ents.commits += 1;
        if let Some(m) = mirror.as_deref_mut() {
            // a commit that sets what's there already (a component re-rendering each frame with settled values)
            // leaves the page as it was: nothing to style, lay out or paint again
            if mirror_ops(m, &ops) {
                m.dirty = true;
            }
        }
        for op in &ops {
            ents.ops += 1;
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
                    let mut e = commands.spawn((ReactNode(id("id").unwrap_or(0)), ReactKind(op["kind"].as_str().unwrap_or("").into()), p));
                    if let Some(t) = op["text"].as_str() {
                        e.insert(ReactText(t.into()));
                    }
                    ents.by_id.insert(id("id").unwrap_or(0), e.id());
                }
                "createText" | "createTextSpan" => {
                    let kind = if op["op"] == "createText" { "#text" } else { "#span" };
                    let e = commands.spawn((ReactNode(id("id").unwrap_or(0)), ReactKind(kind.into()), ReactText(op["text"].as_str().unwrap_or("").into()))).id();
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
                        if let Ok(mut p) = props.get_mut(e) {
                            merge(&mut p.0, &op["props"], op.get("unset"));
                            // style keys React dropped (a transform going to nothing), as the DOM mirror does
                            if let (Some(style), Some(gone)) = (p.0.get_mut("style").and_then(|s| s.as_object_mut()), op["styleUnset"].as_array()) {
                                for k in gone.iter().filter_map(|k| k.as_str()) {
                                    style.remove(k);
                                }
                            }
                        }
                    }
                }
                "updateText" => {
                    if let Some(&e) = id("id").and_then(|i| ents.by_id.get(&i)) {
                        if let Ok(mut t) = texts.get_mut(e) {
                            t.0 = op["text"].as_str().unwrap_or("").into();
                        }
                    }
                }
                other => warn!("raea-react: unknown op {other:?}"),
            }
        }
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
        DomMirror { dom, ids: nodes.iter().map(|(k, v)| (*v, *k)).collect(), nodes, props: HashMap::new(), kinds: HashMap::new(), texts: HashMap::new(), dirty: true, page: None, fonts_dirty: true, order: HashMap::new() }
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

fn kebab(k: &str) -> String {
    let mut out = String::new();
    for ch in k.chars() {
        if ch.is_ascii_uppercase() {
            out.push('-');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A React style object as CSS text, the way react-dom writes it.
fn style_text(style: &serde_json::Map<String, Value>) -> String {
    style
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| {
            // a custom property (--brand-scale) keeps its name and takes a number as it is, as react-dom writes it
            let custom = k.starts_with("--");
            let val = match v {
                Value::Number(n) if !custom && !UNITLESS.contains(&k.as_str()) && n.as_f64() != Some(0.0) => format!("{n}px"),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{}: {val}", if custom { k.clone() } else { kebab(k) })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Props as attributes, the way react-dom sets them: className is class, style is CSS text; handlers, children
/// and keys aren't attributes.
pub(crate) fn apply_props(dom: &mut raea::dom::Dom, node: raea::dom::NodeId, props: &serde_json::Map<String, Value>) {
    for (k, v) in props {
        if k == "children" || k == "key" || (k.starts_with("on") && k.chars().nth(2).is_some_and(|c| c.is_ascii_uppercase())) {
            continue;
        }
        match (k.as_str(), v) {
            ("style", Value::Object(o)) => dom.set_attribute(node, "style", &style_text(o)),
            ("className", Value::String(s)) => dom.set_attribute(node, "class", s),
            (_, Value::String(s)) => dom.set_attribute(node, k, s),
            (_, Value::Number(n)) => dom.set_attribute(node, k, &n.to_string()),
            (_, Value::Bool(true)) => dom.set_attribute(node, k, ""),
            _ => {}
        }
    }
}

/// A node's props as a rendered page draws them: a video's own source and data attributes stay off the image the
/// renderer fills each frame; an image's relative source resolves from the edit's folder.
fn page_props(page: &Option<std::path::PathBuf>, kind: &str, p: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let Some(dir) = page else { return p.clone() };
    let mut p = p.clone();
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
                let before = all.clone();
                merge(all, &op["props"], op.get("unset"));
                if let (Some(style), Some(gone)) = (all.get_mut("style").and_then(|s| s.as_object_mut()), op["styleUnset"].as_array()) {
                    for k in gone.iter().filter_map(|k| k.as_str()) {
                        style.remove(k);
                    }
                }
                // the frame marker's own attributes (data-raea-frame each frame) draw nothing
                let shown = |p: &serde_json::Map<String, Value>| p.iter().filter(|(k, _)| !matches!(k.as_str(), "data-raea-frame" | "data-raea-config" | "data-raea-done")).map(|(k, v)| (k.clone(), v.clone())).collect::<serde_json::Map<_, _>>();
                if *all == before {
                    continue;
                }
                if shown(all) != shown(&before) {
                    changed = true;
                }
                let all = all.clone();
                for k in op["unset"].as_array().into_iter().flatten().filter_map(|k| k.as_str()) {
                    m.dom.clear_attribute(n, if k == "className" { "class" } else { k });
                }
                let kind = m.kinds.get(&i).cloned().unwrap_or_default();
                apply_props(&mut m.dom, n, &page_props(&m.page, &kind, &all));
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
    m.is_some_and(|m| m.dirty && m.page.is_none())
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
    m.dom.style(0.0);
    f.style_ms = t0.elapsed().as_secs_f64() * 1e3;
}

fn dom_layout(mut m: NonSendMut<DomMirror>, mut f: ResMut<DomFrame>) {
    let t0 = Instant::now();
    m.dom.layout(0.0);
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
