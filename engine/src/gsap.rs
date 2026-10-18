//! GSAP timeline semantics evaluated natively.
//!
//! The JS shim (gsap_shim.js) records the authored tree with GSAP's own placement rules. Here the
//! tree becomes per-(target, property) tracks of contributions, and every frame is evaluated as
//! a pure function of time with animato tweens:
//!
//! * a contribution is "started" once its parent chain has reached its start; the started one
//!   whose value changed last wins (GSAP renders children in start order and the last write
//!   sticks), `overwrite: "auto"` cuts the overlapped part of earlier tweens;
//! * `to()` start values are recorded lazily from whatever the property held at the tween's
//!   first render (the previous contribution's value), `from()` ends at the value the property
//!   had when the tween was created, `immediateRender` tweens own the state before they start.

use crate::ease::{self, Ease};
use crate::value::{self, CssValue, Raw};
use animato::{Interpolate, Tween};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

// ---------------------------------------------------------------------------------------------
// Tree as sent by the shim

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    Timeline(TimelineNode),
    Tween(TweenNode),
    Call(CallNode),
}

fn one() -> f64 {
    1.0
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineNode {
    pub id: u64,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub repeat: i32,
    #[serde(default)]
    pub yoyo: bool,
    #[serde(default)]
    pub repeat_delay: f64,
    #[serde(default = "one")]
    pub time_scale: f64,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub callbacks: HashMap<String, u64>,
    #[serde(default)]
    pub children: Vec<Node>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TweenNode {
    pub id: u64,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub duration: f64,
    pub mode: String,
    #[serde(default)]
    pub creation: u64,
    #[serde(default)]
    pub ease: Value,
    #[serde(default)]
    pub immediate_render: bool,
    #[serde(default)]
    pub repeat: i32,
    #[serde(default)]
    pub yoyo: bool,
    #[serde(default)]
    pub repeat_delay: f64,
    #[serde(default = "one")]
    pub time_scale: f64,
    #[serde(default)]
    pub overwrite: Value,
    #[serde(default)]
    pub callbacks: HashMap<String, u64>,
    /// `clearProps` ("all" or a comma list): inline styles removed when the tween completes
    #[serde(default)]
    pub clear_props: Option<String>,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub target: TargetRef,
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub to: Map<String, Value>,
    #[serde(default)]
    pub from: Option<Map<String, Value>>,
    #[serde(default)]
    pub base: Map<String, Value>,
    #[serde(default)]
    pub attr_base: Map<String, Value>,
    #[serde(default)]
    pub inline: Map<String, Value>,
    #[serde(default)]
    pub keyframes: Option<Vec<Keyframe>>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TargetRef {
    #[serde(default)]
    pub el: Option<u64>,
    #[serde(default)]
    pub obj: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Keyframe {
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub delay: f64,
    #[serde(default)]
    pub ease: Value,
    #[serde(default)]
    pub props: Map<String, Value>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CallNode {
    pub id: u64,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub callback: Option<u64>,
}

impl Node {
    pub fn id(&self) -> u64 {
        match self {
            Node::Timeline(t) => t.id,
            Node::Tween(t) => t.id,
            Node::Call(c) => c.id,
        }
    }
    pub fn start(&self) -> f64 {
        match self {
            Node::Timeline(t) => t.start,
            Node::Tween(t) => t.start,
            Node::Call(c) => c.start,
        }
    }
    pub fn set_start(&mut self, s: f64) {
        match self {
            Node::Timeline(t) => t.start = s,
            Node::Tween(t) => t.start = s,
            Node::Call(c) => c.start = s,
        }
    }
    /// Find a node anywhere in the tree by id.
    pub fn find(&self, id: u64) -> Option<&Node> {
        if self.id() == id {
            return Some(self);
        }
        if let Node::Timeline(t) = self {
            for c in &t.children {
                if let Some(n) = c.find(id) {
                    return Some(n);
                }
            }
        }
        None
    }
    pub fn total_duration(&self) -> f64 {
        match self {
            Node::Timeline(t) => total(t.duration / t.time_scale.max(1e-9), t.repeat, t.repeat_delay),
            Node::Tween(t) => {
                let span = tween_span(t);
                total(span / t.time_scale.max(1e-9), t.repeat, t.repeat_delay)
            }
            Node::Call(_) => 0.0,
        }
    }
}

fn tween_span(t: &TweenNode) -> f64 {
    let stagger = t.items.iter().map(|i| i.offset).fold(0.0, f64::max);
    stagger + t.duration
}

fn total(dur: f64, repeat: i32, rdelay: f64) -> f64 {
    if repeat < 0 {
        f64::INFINITY
    } else {
        dur * (repeat as f64 + 1.0) + rdelay * repeat as f64
    }
}

/// Position inside one iteration for a time >= 0 (GSAP repeat / yoyo / repeatDelay).
pub fn fold(time: f64, dur: f64, repeat: i32, yoyo: bool, rdelay: f64) -> f64 {
    if repeat == 0 || dur <= 0.0 {
        return time.min(dur.max(0.0));
    }
    let cycle = dur + rdelay;
    let total = total(dur, repeat, rdelay);
    if time >= total {
        return if yoyo && repeat % 2 == 1 { 0.0 } else { dur };
    }
    let iter = (time / cycle).floor();
    let mut local = time - iter * cycle;
    if local > dur {
        local = dur;
    }
    if yoyo && (iter as i64) % 2 == 1 {
        local = dur - local;
    }
    local
}

// ---------------------------------------------------------------------------------------------
// Scene: the tree flattened into tracks

const EPS: f64 = 1e-9;

/// One timeline level between the render clock and a tween.
#[derive(Clone, Debug)]
struct Seg {
    start: f64,
    ts: f64,
    dur: f64,
    repeat: i32,
    yoyo: bool,
    rdelay: f64,
    /// Seeked directly by the runtime (`tl.totalTime(t)`): local = parent time.
    root: bool,
    /// A paused timeline nobody seeks: frozen at 0.
    frozen: bool,
    /// The timeline this level is (`u64::MAX` for the renderer's own levels): what a script's `tl.time(x)` names.
    id: u64,
}

impl Seg {
    fn map(&self, parent: f64) -> Option<f64> {
        if self.frozen {
            return Some(0.0);
        }
        let raw = if self.root { parent } else { (parent - self.start) * self.ts };
        if raw < -EPS {
            return None;
        }
        Some(fold(raw.max(0.0), self.dur, self.repeat, self.yoyo, self.rdelay))
    }
}

/// Local time inside the innermost timeline of a chain, None while any level hasn't started.
fn chain_time(chain: &[Seg], t: f64) -> Option<f64> {
    let mut local = t;
    for seg in chain {
        local = seg.map(local)?;
    }
    Some(local)
}

/// (origin, scale): global = origin + local / scale for the first iteration of a chain.
fn chain_origin(chain: &[Seg]) -> (f64, f64) {
    let mut origin = 0.0;
    let mut scale = 1.0;
    for seg in chain {
        if seg.frozen {
            return (0.0, 0.0);
        }
        if !seg.root {
            origin += seg.start / scale;
            scale *= seg.ts.max(1e-9);
        }
    }
    (origin, scale)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    El(u64),
    Obj(u64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    To,
    From,
    FromTo,
    Set,
    /// clearProps: from this instant the property is not set inline
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Overwrite {
    None,
    Auto,
    All,
}

struct Contribution {
    chain: Rc<[Seg]>,
    tween_start: f64,
    tween_ts: f64,
    span: f64,
    repeat: i32,
    yoyo: bool,
    rdelay: f64,
    offset: f64,
    dur: f64,
    ease: Ease,
    mode: Mode,
    raw_to: Option<Raw>,
    raw_from: Option<Raw>,
    immediate: bool,
    order: usize,
    creation: (u64, usize),
    overwrite: Overwrite,
    first_start: f64,
    first_end: f64,
    killed_at: f64,
    /// value the property held when the tween was created (GSAP's from() end value)
    creation_value: Option<CssValue>,
    /// resolved (start, end) once known
    curve: Option<Curve>,
}

enum Curve {
    Animato(Tween<CssValue>),
    Custom(Ease, CssValue, CssValue),
}

impl Curve {
    fn new(ease: &Ease, start: CssValue, end: CssValue) -> Curve {
        let (start, end) = neutralize(start, end);
        match ease {
            Ease::Animato(e) => Curve::Animato(Tween::new(start, end).duration(1.0).easing(e.clone()).build()),
            other => Curve::Custom(other.clone(), start, end),
        }
    }
    fn at(&mut self, p: f32) -> CssValue {
        match self {
            Curve::Animato(t) => {
                t.seek(p);
                t.value()
            }
            Curve::Custom(e, a, b) => a.lerp(b, e.apply(p)),
        }
    }
    fn start(&self) -> CssValue {
        match self {
            Curve::Animato(t) => t.start.clone(),
            Curve::Custom(_, a, _) => a.clone(),
        }
    }
}

/// `none` <-> function list: tween from/to the neutral form of the other side.
fn neutralize(start: CssValue, end: CssValue) -> (CssValue, CssValue) {
    let is_none = |v: &CssValue| matches!(v, CssValue::Discrete { to, .. } if to == "none" || to.is_empty());
    if is_none(&start) {
        if let Some(n) = value::neutral_like(&end) {
            return (n, end);
        }
    }
    if is_none(&end) {
        if let Some(n) = value::neutral_like(&start) {
            let none = end;
            let _ = none;
            return (start, n);
        }
    }
    if let Some(aligned) = value::align_colors(&start, &end) {
        return (aligned, end);
    }
    (start, end)
}

impl Contribution {
    /// A copy without the resolved curve (clearProps adds one per cleared track).
    fn fresh(&self) -> Contribution {
        Contribution {
            chain: self.chain.clone(),
            tween_start: self.tween_start,
            tween_ts: self.tween_ts,
            span: self.span,
            repeat: self.repeat,
            yoyo: self.yoyo,
            rdelay: self.rdelay,
            offset: self.offset,
            dur: self.dur,
            ease: self.ease.clone(),
            mode: self.mode,
            raw_to: self.raw_to.clone(),
            raw_from: self.raw_from.clone(),
            immediate: self.immediate,
            order: self.order,
            creation: self.creation,
            overwrite: self.overwrite,
            first_start: self.first_start,
            first_end: self.first_end,
            killed_at: self.killed_at,
            creation_value: self.creation_value.clone(),
            curve: None,
        }
    }

    /// (progress, time the value last changed) or None when not started / overwritten.
    fn state(&self, t: f64) -> Option<(f32, f64)> {
        if t > self.killed_at + EPS {
            return None;
        }
        let parent = chain_time(&self.chain, t)?;
        let raw = (parent - self.tween_start) * self.tween_ts;
        if raw - self.offset < -EPS {
            return None;
        }
        let folded = fold(raw.max(0.0), self.span, self.repeat, self.yoyo, self.rdelay);
        let local = folded - self.offset;
        let p = if self.dur <= 0.0 { 1.0 } else { (local / self.dur).clamp(0.0, 1.0) };
        Some((p as f32, t.min(self.first_end)))
    }
}

pub struct Track {
    pub target: Target,
    pub prop: String,
    pub base: CssValue,
    /// the element's own inline value for the property (restored while nothing animates it)
    pub inline: Option<String>,
    contributions: Vec<Contribution>,
    /// index into contributions of the one that owns the pre-start state
    pre: Option<usize>,
}

impl Track {
    fn winner(&self, t: f64, before: Option<(f64, usize)>) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32, f64, usize)> = None;
        for (i, c) in self.contributions.iter().enumerate() {
            if let Some(limit) = before {
                if (c.first_start, c.order) >= limit {
                    continue;
                }
            }
            let Some((p, changed)) = c.state(t) else { continue };
            let better = match best {
                None => true,
                Some((_, _, bc, bo)) => changed > bc + EPS || ((changed - bc).abs() <= EPS && c.order > bo),
            };
            if better {
                best = Some((i, p, changed, c.order));
            }
        }
        best.map(|(i, p, _, _)| (i, p))
    }

    fn ensure_curve(&mut self, i: usize) {
        if self.contributions[i].curve.is_some() {
            return;
        }
        let mode = self.contributions[i].mode;
        let creation = self.contributions[i].creation_value.clone().unwrap_or_else(|| self.base.clone());
        let (start, end) = match mode {
            Mode::To | Mode::Set => {
                // to() with startAt starts from the startAt value instead of the live one
                let before = self.value_before(i);
                let start = match &self.contributions[i].raw_from {
                    Some(r) => r.resolve(&before),
                    None => before,
                };
                let end = match &self.contributions[i].raw_to {
                    Some(r) => r.resolve(&start),
                    None => start.clone(),
                };
                if mode == Mode::Set { (end.clone(), end) } else { (start, end) }
            }
            Mode::From => {
                let start = match &self.contributions[i].raw_from {
                    Some(r) => r.resolve(&creation),
                    None => creation.clone(),
                };
                (start, creation)
            }
            Mode::Clear => (self.base.clone(), self.base.clone()),
            Mode::FromTo => {
                let start = match &self.contributions[i].raw_from {
                    Some(r) => r.resolve(&creation),
                    None => creation.clone(),
                };
                // a property only in the from-vars is GSAP's startAt: set, then held
                let end = match &self.contributions[i].raw_to {
                    Some(r) => r.resolve(&start),
                    None => start.clone(),
                };
                (start, end)
            }
        };
        let ease = self.contributions[i].ease.clone();
        self.contributions[i].curve = Some(Curve::new(&ease, start, end));
    }

    /// What the property held when contribution `i` first rendered.
    fn value_before(&mut self, i: usize) -> CssValue {
        let at = self.contributions[i].first_start;
        let limit = (at, self.contributions[i].order);
        if !at.is_finite() {
            return self.contributions[i].creation_value.clone().unwrap_or_else(|| self.base.clone());
        }
        match self.winner(at, Some(limit)) {
            Some((j, p)) => {
                self.ensure_curve(j);
                self.contributions[j].curve.as_mut().unwrap().at(p)
            }
            None => self.contributions[i].creation_value.clone().unwrap_or_else(|| self.base.clone()),
        }
    }

    /// Replace the base value (e.g. transform components decomposed from the computed
    /// transform once layout exists) and re-derive what depends on it.
    pub fn set_base(&mut self, base: CssValue) {
        self.base = base;
        for c in &mut self.contributions {
            c.curve = None;
        }
        resolve_creation_values(self);
    }

    /// Value at time t; None means "not animated: restore the element's own style".
    pub fn value_at(&mut self, t: f64) -> Option<CssValue> {
        if let Some((i, p)) = self.winner(t, None) {
            if self.contributions[i].mode == Mode::Clear {
                // `__inline` reports clearProps "all" so the element's authored inline styles go too
                return if self.prop == "__inline" { Some(CssValue::discrete("clear")) } else { None };
            }
            self.ensure_curve(i);
            return Some(self.contributions[i].curve.as_mut().unwrap().at(p));
        }
        let pre = self.pre?;
        self.ensure_curve(pre);
        Some(self.contributions[pre].curve.as_ref().unwrap().start())
    }
}

/// Callback bookkeeping for one tween / timeline / call marker.
struct CallbackSite {
    /// the shim's id of the tween / timeline (its callbacks run with `this` = it)
    anim: u64,
    chain: Rc<[Seg]>,
    start: f64,
    ts: f64,
    span: f64,
    repeat: i32,
    yoyo: bool,
    rdelay: f64,
    on_start: Option<u64>,
    on_update: Option<u64>,
    on_complete: Option<u64>,
    call: Option<u64>,
}

impl CallbackSite {
    /// (started, progress) at t
    fn at(&self, t: f64) -> (bool, f64) {
        let (started, p, _, _) = self.times(t);
        (started, p)
    }

    /// (started, progress, time in the iteration, total time) at t
    fn times(&self, t: f64) -> (bool, f64, f64, f64) {
        let Some(parent) = chain_time(&self.chain, t) else { return (false, 0.0, 0.0, 0.0) };
        let raw = (parent - self.start) * self.ts;
        if raw < -EPS {
            return (false, 0.0, 0.0, 0.0);
        }
        if self.span <= 0.0 {
            return (true, 1.0, 0.0, 0.0);
        }
        let end = total(self.span, self.repeat, self.rdelay);
        let folded = fold(raw.max(0.0), self.span, self.repeat, self.yoyo, self.rdelay);
        let done = raw >= end - EPS;
        let time = if done { if self.yoyo && self.repeat % 2 == 1 { 0.0 } else { self.span } } else { folded };
        (true, if done { 1.0 } else { (folded / self.span).clamp(0.0, 1.0) }, time, raw.max(0.0).min(end))
    }
}

pub struct Scene {
    pub tracks: Vec<Track>,
    callbacks: Vec<CallbackSite>,
}

pub struct FrameUpdate {
    /// (target, prop, value or None to restore)
    pub values: Vec<(usize, Option<CssValue>)>,
    /// (callback, animation, its time, progress, total time) in firing order
    pub callbacks: Vec<(u64, u64, f64, f64, f64)>,
}

/// How a top-level timeline is driven.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    /// `tl.totalTime(min(t, duration))` by the runtime
    Root,
    /// nested by the runtime into the root at a host start
    NestedAt(f64),
    /// unpaused, runs on GSAP's global clock (= render time)
    Free,
    /// paused and never seeked
    Frozen,
}

pub struct Builder {
    tracks: BTreeMap<(Target, String), Track>,
    callbacks: Vec<CallbackSite>,
    order: usize,
    pub warnings: Vec<String>,
    /// clearProps: (target, props or None for "all", the clear contribution to add to each track)
    clears: Vec<(Target, Option<Vec<String>>, Contribution)>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder { tracks: BTreeMap::new(), callbacks: Vec::new(), order: 0, warnings: Vec::new(), clears: Vec::new() }
    }

    /// Add a top-level animation (timeline / tween / call) driven as `drive`.
    pub fn add_top(&mut self, node: &Node, drive: Drive, root_duration: f64) {
        let base_chain: Vec<Seg> = match drive {
            Drive::Root => vec![Seg {
                start: 0.0,
                ts: 1.0,
                dur: f64::INFINITY,
                repeat: 0,
                yoyo: false,
                rdelay: 0.0,
                root: true,
                frozen: false,
                id: u64::MAX,
            }],
            Drive::NestedAt(_) | Drive::Free => vec![],
            Drive::Frozen => vec![Seg {
                start: 0.0,
                ts: 1.0,
                dur: 0.0,
                repeat: 0,
                yoyo: false,
                rdelay: 0.0,
                root: false,
                frozen: true,
                id: u64::MAX,
            }],
        };
        let _ = root_duration;
        match (drive, node) {
            (Drive::Root, Node::Timeline(tl)) => {
                // The runtime seeks the root itself: its repeat/yoyo apply, its start doesn't.
                let chain: Rc<[Seg]> = Rc::from(vec![Seg {
                    start: 0.0,
                    ts: 1.0,
                    dur: tl.duration,
                    repeat: tl.repeat,
                    yoyo: tl.yoyo,
                    rdelay: tl.repeat_delay,
                    root: true,
                    frozen: false,
                    id: tl.id,
                }]);
                self.add_timeline_callbacks(tl, &Rc::from(base_chain), 0.0);
                for child in &tl.children {
                    self.add_node(child, &chain);
                }
            }
            (Drive::NestedAt(at), n) => {
                let mut n = n.clone();
                n.set_start(at);
                self.add_node(&n, &Rc::from(base_chain));
            }
            (_, n) => self.add_node(n, &Rc::from(base_chain)),
        }
    }

    fn add_timeline_callbacks(&mut self, tl: &TimelineNode, chain: &Rc<[Seg]>, start: f64) {
        if tl.callbacks.is_empty() {
            return;
        }
        self.callbacks.push(CallbackSite {
            anim: tl.id,
            chain: chain.clone(),
            start,
            ts: tl.time_scale,
            span: tl.duration,
            repeat: tl.repeat,
            yoyo: tl.yoyo,
            rdelay: tl.repeat_delay,
            on_start: tl.callbacks.get("onStart").copied(),
            on_update: tl.callbacks.get("onUpdate").copied(),
            on_complete: tl.callbacks.get("onComplete").copied(),
            call: None,
        });
    }

    fn add_node(&mut self, node: &Node, chain: &Rc<[Seg]>) {
        match node {
            Node::Timeline(tl) => {
                self.add_timeline_callbacks(tl, chain, tl.start);
                let mut inner: Vec<Seg> = chain.to_vec();
                inner.push(Seg {
                    start: tl.start,
                    ts: tl.time_scale,
                    dur: tl.duration,
                    repeat: tl.repeat,
                    yoyo: tl.yoyo,
                    rdelay: tl.repeat_delay,
                    root: false,
                    frozen: false,
                    id: tl.id,
                });
                let inner: Rc<[Seg]> = Rc::from(inner);
                let mut children: Vec<&Node> = tl.children.iter().collect();
                // GSAP keeps children sorted by start; ties keep insertion order.
                children.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap_or(std::cmp::Ordering::Equal));
                for c in children {
                    self.add_node(c, &inner);
                }
            }
            Node::Call(c) => {
                if let Some(cb) = c.callback {
                    self.callbacks.push(CallbackSite {
                        anim: c.id,
                        chain: chain.clone(),
                        start: c.start,
                        ts: 1.0,
                        span: 0.0,
                        repeat: 0,
                        yoyo: false,
                        rdelay: 0.0,
                        on_start: None,
                        on_update: None,
                        on_complete: None,
                        call: Some(cb),
                    });
                }
            }
            Node::Tween(tw) => self.add_tween(tw, chain),
        }
    }

    fn add_tween(&mut self, tw: &TweenNode, chain: &Rc<[Seg]>) {
        let span = tween_span(tw);
        if !tw.callbacks.is_empty() {
            self.callbacks.push(CallbackSite {
                anim: tw.id,
                chain: chain.clone(),
                start: tw.start,
                ts: tw.time_scale,
                span,
                repeat: tw.repeat,
                yoyo: tw.yoyo,
                rdelay: tw.repeat_delay,
                on_start: tw.callbacks.get("onStart").copied(),
                on_update: tw.callbacks.get("onUpdate").copied(),
                on_complete: tw.callbacks.get("onComplete").copied(),
                call: None,
            });
        }
        let mode = match tw.mode.as_str() {
            "from" => Mode::From,
            "fromTo" => Mode::FromTo,
            "set" => Mode::Set,
            _ => Mode::To,
        };
        let overwrite = match &tw.overwrite {
            Value::Bool(true) => Overwrite::All,
            Value::String(s) if s == "auto" => Overwrite::Auto,
            Value::String(s) if s == "true" => Overwrite::All,
            _ => Overwrite::None,
        };
        let tween_ease = parse_ease(&tw.ease);
        let (origin, scale) = chain_origin(chain);
        for (item_index, item) in tw.items.iter().enumerate() {
            let target = match (item.target.el, item.target.obj) {
                (Some(e), _) => Target::El(e),
                (_, Some(o)) => Target::Obj(o),
                _ => continue,
            };
            // keyframes become consecutive "to" steps inside the tween
            let steps: Vec<(f64, f64, Ease, Map<String, Value>, Mode, Option<Map<String, Value>>)> = match &item.keyframes {
                Some(kfs) if !kfs.is_empty() => {
                    let mut cum = 0.0;
                    kfs.iter()
                        .map(|k| {
                            cum += k.delay;
                            let s = (cum, k.duration, if k.ease.is_null() { tween_ease.clone() } else { parse_ease(&k.ease) }, k.props.clone(), Mode::To, None);
                            cum += k.duration;
                            s
                        })
                        .collect()
                }
                // from(): the authored vars are the start values, the end is whatever the
                // property holds at creation
                _ if mode == Mode::From => vec![(0.0, tw.duration, tween_ease.clone(), Map::new(), mode, Some(item.to.clone()))],
                _ => vec![(0.0, tw.duration, tween_ease.clone(), item.to.clone(), mode, item.from.clone())],
            };
            for (step_offset, step_dur, step_ease, to, step_mode, from) in steps {
                let mut props: Vec<String> = to.keys().cloned().collect();
                if let Some(f) = &from {
                    for k in f.keys() {
                        if !props.contains(k) {
                            props.push(k.clone());
                        }
                    }
                }
                for prop in props {
                    if prop == "attr" {
                        let to_attrs = to.get("attr").and_then(Value::as_object).cloned().unwrap_or_default();
                        let from_attrs = from.as_ref().and_then(|f| f.get("attr")).and_then(Value::as_object).cloned();
                        let mut names: Vec<String> = to_attrs.keys().cloned().collect();
                        if let Some(fa) = &from_attrs {
                            for k in fa.keys() {
                                if !names.contains(k) {
                                    names.push(k.clone());
                                }
                            }
                        }
                        for name in names {
                            let key = format!("attr:{name}");
                            let base_raw = item.attr_base.get(&name).cloned().unwrap_or(Value::Null);
                            self.push(
                                target, &key, &base_raw, None, chain, tw, item, item_index, step_offset, step_dur, &step_ease,
                                step_mode, to_attrs.get(&name), from_attrs.as_ref().and_then(|f| f.get(&name)), overwrite, origin, scale,
                            );
                        }
                        continue;
                    }
                    // `scale` drives both axes
                    let keys: Vec<&str> = if prop == "scale" { vec!["scaleX", "scaleY"] } else { vec![prop.as_str()] };
                    for key in keys {
                        let base_raw = item.base.get(&prop).cloned().unwrap_or(Value::Null);
                        let inline = item.inline.get(&prop).and_then(Value::as_str).map(str::to_string);
                        self.push(
                            target, key, &base_raw, inline, chain, tw, item, item_index, step_offset, step_dur, &step_ease, step_mode,
                            to.get(&prop), from.as_ref().and_then(|f| f.get(&prop)), overwrite, origin, scale,
                        );
                    }
                }
            }
        }
        if let Some(cp) = &tw.clear_props {
            // GSAP clears when the tween completes; "all" also empties the element's style attribute
            let props = if cp == "all" || cp == "true" {
                None
            } else {
                Some(cp.split(',').map(|p| camel(p.trim())).filter(|p| !p.is_empty()).collect::<Vec<_>>())
            };
            self.order += 1;
            let (origin, scale) = chain_origin(chain);
            for (item_index, item) in tw.items.iter().enumerate() {
                let Some(el) = item.target.el else { continue };
                let offset = item.offset + tw.duration;
                let local = tw.start + offset / tw.time_scale.max(1e-9);
                let first_start = if scale == 0.0 { f64::INFINITY } else { origin + local / scale };
                let clear = Contribution {
                    chain: chain.clone(),
                    tween_start: tw.start,
                    tween_ts: tw.time_scale.max(1e-9),
                    span: tween_span(tw).max(offset),
                    repeat: 0,
                    yoyo: false,
                    rdelay: 0.0,
                    offset,
                    dur: 0.0,
                    ease: tween_ease.clone(),
                    mode: Mode::Clear,
                    raw_to: None,
                    raw_from: None,
                    immediate: false,
                    order: self.order,
                    creation: (tw.creation, item_index),
                    overwrite: Overwrite::None,
                    first_start,
                    first_end: first_start,
                    killed_at: f64::INFINITY,
                    creation_value: None,
                    curve: None,
                };
                self.clears.push((Target::El(el), props.clone(), clear));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        target: Target,
        prop: &str,
        base_raw: &Value,
        inline: Option<String>,
        chain: &Rc<[Seg]>,
        tw: &TweenNode,
        item: &Item,
        item_index: usize,
        step_offset: f64,
        step_dur: f64,
        ease: &Ease,
        mode: Mode,
        to: Option<&Value>,
        from: Option<&Value>,
        overwrite: Overwrite,
        origin: f64,
        scale: f64,
    ) {
        let key = (target, prop.to_string());
        // plain objects hold raw numbers: no CSS units, no angle conversion
        let parse_as = if matches!(target, Target::Obj(_)) { "--object" } else { prop };
        let track = self.tracks.entry(key).or_insert_with(|| Track {
            target,
            prop: prop.to_string(),
            base: base_value(parse_as, base_raw),
            inline: inline.clone(),
            contributions: Vec::new(),
            pre: None,
        });
        if track.inline.is_none() {
            track.inline = inline;
        }
        let offset = item.offset + step_offset;
        let local_start = tw.start + offset / tw.time_scale.max(1e-9);
        let span = tween_span(tw);
        let local_end = if tw.repeat != 0 {
            tw.start + total(span, tw.repeat, tw.repeat_delay) / tw.time_scale.max(1e-9)
        } else {
            tw.start + (offset + step_dur) / tw.time_scale.max(1e-9)
        };
        let (first_start, first_end) = if scale == 0.0 {
            if local_start <= EPS { (0.0, 0.0) } else { (f64::INFINITY, f64::INFINITY) }
        } else {
            (origin + local_start / scale, origin + local_end / scale)
        };
        let nested_repeat = chain.iter().any(|s| s.repeat != 0);
        self.order += 1;
        track.contributions.push(Contribution {
            chain: chain.clone(),
            tween_start: tw.start,
            tween_ts: tw.time_scale.max(1e-9),
            span: if item.keyframes.is_some() { span.max(offset + step_dur) } else { span },
            repeat: tw.repeat,
            yoyo: tw.yoyo,
            rdelay: tw.repeat_delay,
            offset,
            dur: step_dur,
            ease: ease.clone(),
            mode,
            raw_to: to.map(|v| value::parse_raw(parse_as, v)),
            raw_from: from.map(|v| value::parse_raw(parse_as, v)),
            immediate: tw.immediate_render
                && (matches!(mode, Mode::From | Mode::FromTo | Mode::Set) || (mode == Mode::To && from.is_some()))
                && item.keyframes.is_none(),
            order: self.order,
            creation: (tw.creation, item_index),
            overwrite,
            first_start,
            first_end: if nested_repeat { f64::INFINITY } else { first_end },
            killed_at: f64::INFINITY,
            creation_value: None,
            curve: None,
        });
    }

    pub fn finish(mut self, duration: f64) -> Scene {
        for (target, props, clear) in std::mem::take(&mut self.clears) {
            let names: Vec<String> = match &props {
                None => {
                    let mut all: Vec<String> = self.tracks.keys().filter(|(t, _)| *t == target).map(|(_, p)| p.clone()).collect();
                    all.push("__inline".into());
                    all
                }
                Some(list) => list
                    .iter()
                    .flat_map(|p| {
                        let transform = ["x", "y", "z", "xPercent", "yPercent", "rotation", "rotationX", "rotationY", "skewX", "skewY", "scaleX", "scaleY", "transformPerspective"];
                        if p == "transform" { transform.iter().map(|s| s.to_string()).collect() } else { vec![p.clone()] }
                    })
                    .filter(|p| self.tracks.contains_key(&(target, p.clone())))
                    .collect(),
            };
            for name in names {
                let track = self.tracks.entry((target, name.clone())).or_insert_with(|| Track {
                    target,
                    prop: name.clone(),
                    base: CssValue::discrete(""),
                    inline: None,
                    contributions: Vec::new(),
                    pre: None,
                });
                track.contributions.push(clear.fresh());
            }
        }
        let mut tracks: Vec<Track> = self.tracks.into_values().collect();
        for track in &mut tracks {
            resolve_creation_values(track);
            resolve_overwrites(track);
            track.pre = track
                .contributions
                .iter()
                .enumerate()
                .filter(|(_, c)| c.immediate)
                .min_by(|(_, a), (_, b)| (a.first_start, a.order).partial_cmp(&(b.first_start, b.order)).unwrap())
                .map(|(i, _)| i);
        }
        let _ = duration;
        Scene { tracks, callbacks: self.callbacks }
    }
}

fn parse_ease(v: &Value) -> Ease {
    match v {
        Value::String(s) => ease::parse(s),
        Value::Object(o) => match o.get("samples").and_then(Value::as_array) {
            Some(a) => Ease::Sampled(a.iter().filter_map(Value::as_f64).map(|f| f as f32).collect()),
            None => ease::default_ease(),
        },
        _ => ease::default_ease(),
    }
}

fn base_value(prop: &str, raw: &Value) -> CssValue {
    match prop {
        "scaleX" | "scaleY" => CssValue::num(1.0, ""),
        "x" | "y" | "z" => CssValue::num(0.0, "px"),
        "xPercent" | "yPercent" | "transformPerspective" => CssValue::num(0.0, ""),
        "rotation" | "rotationX" | "rotationY" | "skewX" | "skewY" => CssValue::num(0.0, "deg"),
        _ => match raw {
            Value::Null => CssValue::discrete(""),
            v => match value::parse_raw(prop, v) {
                Raw::Abs(c) => c,
                Raw::Rel(_, c) => c,
            },
        },
    }
}

/// GSAP's from() reads the property at creation time, after earlier immediate renders.
fn resolve_creation_values(track: &mut Track) {
    let mut idx: Vec<usize> = (0..track.contributions.len()).collect();
    idx.sort_by_key(|&i| track.contributions[i].creation);
    let mut current = track.base.clone();
    for i in idx {
        let c = &mut track.contributions[i];
        c.creation_value = Some(current.clone());
        if c.immediate {
            let v = match (c.mode, &c.raw_from, &c.raw_to) {
                (Mode::Set, _, Some(to)) => to.resolve(&current),
                (_, Some(from), _) => from.resolve(&current),
                _ => continue,
            };
            current = v;
        }
    }
}

/// `overwrite: "auto"` cuts overlapping earlier tweens at the newcomer's start;
/// `overwrite: true` removes every earlier-created tween of the target property.
fn resolve_overwrites(track: &mut Track) {
    let n = track.contributions.len();
    for b in 0..n {
        let (ow, b_start, b_creation) = {
            let c = &track.contributions[b];
            (c.overwrite, c.first_start, c.creation)
        };
        if ow == Overwrite::None || !b_start.is_finite() {
            continue;
        }
        for a in 0..n {
            if a == b {
                continue;
            }
            let ac = &mut track.contributions[a];
            match ow {
                Overwrite::All if ac.creation < b_creation => ac.killed_at = f64::NEG_INFINITY,
                Overwrite::Auto if ac.first_start < b_start && ac.first_end > b_start => {
                    ac.killed_at = ac.killed_at.min(b_start)
                }
                _ => {}
            }
        }
    }
}

impl Scene {
    /// A script seeking timeline `anim` to its local time `local` while the page runs (`tl.time(x)`): for each
    /// track, the render time at which that happens (the timeline's first iteration), or None for tracks the
    /// timeline doesn't drive. GSAP renders only that timeline's children; a track it shares with another timeline
    /// is evaluated whole at that time.
    pub fn seek_times(&self, anim: u64, local: f64) -> Vec<Option<f64>> {
        self.tracks
            .iter()
            .map(|track| {
                track.contributions.iter().find_map(|c| {
                    let k = c.chain.iter().position(|s| s.id == anim)?;
                    let (origin, scale) = chain_origin(&c.chain[..k]);
                    if scale == 0.0 {
                        return None;
                    }
                    let seg = &c.chain[k];
                    let parent = if seg.root { local } else { seg.start + local / seg.ts.max(1e-9) };
                    Some(origin + parent / scale)
                })
            })
            .collect()
    }

    /// Evaluate every track at `t` (and the callbacks crossed since `prev`).
    pub fn frame(&mut self, prev: f64, t: f64) -> FrameUpdate {
        let mut values = Vec::with_capacity(self.tracks.len());
        for (i, track) in self.tracks.iter_mut().enumerate() {
            values.push((i, track.value_at(t)));
        }
        let mut fired: Vec<(f64, usize, (u64, u64, f64, f64, f64))> = Vec::new();
        for (order, site) in self.callbacks.iter().enumerate() {
            let (was_started, was_p) = site.at(prev);
            let (started, p, time, tt) = site.times(t);
            let at = |cb: u64| (cb, site.anim, time, p, tt);
            if let Some(cb) = site.call {
                if started && !was_started {
                    fired.push((t, order, at(cb)));
                }
                continue;
            }
            if started && !was_started {
                if let Some(cb) = site.on_start {
                    fired.push((t, order * 3, at(cb)));
                }
            }
            let changed = started && (p != was_p || !was_started);
            if changed {
                if let Some(cb) = site.on_update {
                    fired.push((t, order * 3 + 1, at(cb)));
                }
            }
            if started && p >= 1.0 && was_p < 1.0 {
                if let Some(cb) = site.on_complete {
                    fired.push((t, order * 3 + 2, at(cb)));
                }
            }
        }
        fired.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        FrameUpdate { values, callbacks: fired.into_iter().map(|f| f.2).collect() }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(json: &str) -> Node {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn sequential_to_tweens_chain_start_values() {
        let root = tl(r#"{"kind":"timeline","id":1,"duration":2,"children":[
            {"kind":"tween","id":2,"start":0,"duration":1,"mode":"to","ease":"none","creation":0,
             "items":[{"target":{"el":1},"to":{"x":100},"base":{}}]},
            {"kind":"tween","id":3,"start":1,"duration":1,"mode":"to","ease":"none","creation":1,
             "items":[{"target":{"el":1},"to":{"x":"+=50"},"base":{}}]}
        ]}"#);
        let mut b = Builder::new();
        b.add_top(&root, Drive::Root, 2.0);
        let mut s = b.finish(2.0);
        let v = |s: &mut Scene, t: f64| s.frame(t, t).values[0].1.clone().unwrap().to_css();
        assert_eq!(v(&mut s, 0.5), "50px");
        assert_eq!(v(&mut s, 1.5), "125px");
        assert_eq!(v(&mut s, 3.0), "150px");
    }

    #[test]
    fn from_tween_owns_pre_state() {
        let root = tl(r#"{"kind":"timeline","id":1,"duration":2,"children":[
            {"kind":"tween","id":2,"start":1,"duration":1,"mode":"from","ease":"none","creation":0,"immediateRender":true,
             "items":[{"target":{"el":1},"to":{"opacity":0},"base":{"opacity":"1"}}]}
        ]}"#);
        let mut b = Builder::new();
        b.add_top(&root, Drive::Root, 2.0);
        let mut s = b.finish(2.0);
        let v = |s: &mut Scene, t: f64| s.frame(t, t).values[0].1.clone().unwrap().to_css();
        assert_eq!(v(&mut s, 0.0), "0");
        assert_eq!(v(&mut s, 1.5), "0.5");
        assert_eq!(v(&mut s, 2.5), "1");
    }

    #[test]
    fn stagger_and_yoyo_repeat() {
        let root = tl(r#"{"kind":"timeline","id":1,"duration":4,"children":[
            {"kind":"tween","id":2,"start":0,"duration":1,"mode":"to","ease":"none","creation":0,"repeat":1,"yoyo":true,
             "items":[{"target":{"el":1},"offset":0,"to":{"y":10},"base":{}},
                      {"target":{"el":2},"offset":0.5,"to":{"y":10},"base":{}}]}
        ]}"#);
        let mut b = Builder::new();
        b.add_top(&root, Drive::Root, 4.0);
        let mut s = b.finish(4.0);
        let f = s.frame(0.75, 0.75);
        assert_eq!(f.values[0].1.clone().unwrap().to_css(), "7.5px");
        // el 2 has not started yet at 0.25
        let f = s.frame(0.25, 0.25);
        assert!(f.values[1].1.is_none());
        // span 1.5, yoyo: at 2.25 the group is at 0.75 again (reversed)
        let f = s.frame(2.25, 2.25);
        assert_eq!(f.values[0].1.clone().unwrap().to_css(), "7.5px");
    }

    #[test]
    fn overwrite_auto_cuts_earlier_tween() {
        let root = tl(r#"{"kind":"timeline","id":1,"duration":2,"children":[
            {"kind":"tween","id":2,"start":0,"duration":2,"mode":"to","ease":"none","creation":0,
             "items":[{"target":{"el":1},"to":{"x":100},"base":{}}]},
            {"kind":"tween","id":3,"start":1,"duration":0.5,"mode":"to","ease":"none","creation":1,"overwrite":"auto",
             "items":[{"target":{"el":1},"to":{"x":0},"base":{}}]}
        ]}"#);
        let mut b = Builder::new();
        b.add_top(&root, Drive::Root, 2.0);
        let mut s = b.finish(2.0);
        let v = |s: &mut Scene, t: f64| s.frame(t, t).values[0].1.clone().unwrap().to_css();
        assert_eq!(v(&mut s, 1.25), "25px");
        assert_eq!(v(&mut s, 3.0), "0px");
    }
}

/// kebab-case -> camelCase (clearProps lists CSS names)
fn camel(p: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in p.chars() {
        if c == '-' && !out.is_empty() {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}
