//! Running statements over rows: one evaluator for the live tables (one-off moves) and the per-frame runtime (rules).
//! Rows are held as columns (Cols: per column one optional value per row), the shape the World's store gathers into
//! and the timeline is made of (docs/substrate.feature); Tup is the row form for interchange (JSON, files, tests).

use std::collections::{BTreeMap, HashMap};

use crate::ir::{self, Expr, V};

/// One row of a kind's table.
#[derive(Debug, Clone, PartialEq)]
pub struct Tup {
    pub kind: String,
    pub id: String,
    pub vals: BTreeMap<String, V>,
}

pub fn default_of(col: &str) -> V {
    V::N(if matches!(col, "scale" | "opacity") { 1.0 } else { 0.0 })
}

/// One column: numbers as f64 with a presence mask (the typed case K kernels read as slices), or any values (a
/// column that holds text in some row).
#[derive(Debug, Clone, PartialEq)]
pub enum Col {
    Num { v: Vec<f64>, has: Vec<bool> },
    /// shared until written (copying a frame copies no text)
    Any(std::sync::Arc<Vec<Option<V>>>),
}

impl Col {
    fn empty_for(v: &V, n: usize) -> Col {
        match v {
            V::N(_) => Col::Num { v: vec![0.0; n], has: vec![false; n] },
            V::S(_) => Col::Any(std::sync::Arc::new(vec![None; n])),
        }
    }
    pub fn get(&self, r: usize) -> Option<V> {
        match self {
            Col::Num { v, has } => has[r].then(|| V::N(v[r])),
            Col::Any(x) => x[r].clone(),
        }
    }
    pub fn has(&self, r: usize) -> bool {
        match self {
            Col::Num { has, .. } => has[r],
            Col::Any(x) => x[r].is_some(),
        }
    }
    pub fn num(&self, r: usize) -> Option<f64> {
        match self {
            Col::Num { v, has } => has[r].then_some(v[r]),
            Col::Any(x) => x[r].as_ref().map(V::num),
        }
    }
    pub fn set(&mut self, r: usize, x: V) {
        match (&mut *self, x) {
            (Col::Num { v, has }, V::N(n)) => {
                v[r] = n;
                has[r] = true;
            }
            (Col::Num { v, has }, x) => {
                // text arrives: the column holds any values from now on
                let mut any: Vec<Option<V>> = v.iter().zip(has.iter()).map(|(n, h)| h.then_some(V::N(*n))).collect();
                any[r] = Some(x);
                *self = Col::Any(std::sync::Arc::new(any));
            }
            (Col::Any(a), x) => std::sync::Arc::make_mut(a)[r] = Some(x),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Col::Num { v, .. } => v.len(),
            Col::Any(x) => x.len(),
        }
    }
}

/// Rows as columns: each row's kind and id, and per column one value per row (absent: the row doesn't have it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cols {
    /// shared between a frame and the base it came from (cloning a frame copies no strings)
    pub kind: std::sync::Arc<Vec<String>>,
    pub id: std::sync::Arc<Vec<String>>,
    pub cols: HashMap<String, Col>,
}

impl Cols {
    pub fn from_tups(rows: &[Tup]) -> Cols {
        let mut c = Cols { kind: std::sync::Arc::new(rows.iter().map(|t| t.kind.clone()).collect()), id: std::sync::Arc::new(rows.iter().map(|t| t.id.clone()).collect()), cols: HashMap::new() };
        for (r, t) in rows.iter().enumerate() {
            for (k, v) in &t.vals {
                c.set(r, k, v.clone());
            }
        }
        c
    }
    pub fn len(&self) -> usize {
        self.id.len()
    }
    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }
    pub fn get(&self, r: usize, c: &str) -> Option<V> {
        self.cols.get(c).and_then(|v| v.get(r))
    }
    pub fn num(&self, r: usize, c: &str) -> Option<f64> {
        self.cols.get(c).and_then(|v| v.num(r))
    }
    pub fn set(&mut self, r: usize, c: &str, v: V) {
        let n = self.id.len();
        match self.cols.get_mut(c) {
            Some(col) => col.set(r, v),
            None => {
                let mut col = Col::empty_for(&v, n);
                col.set(r, v);
                self.cols.insert(c.to_string(), col);
            }
        }
    }
    pub fn rows_of(&self, kind: &str) -> Vec<usize> {
        (0..self.kind.len()).filter(|&r| self.kind[r] == kind).collect()
    }
    pub fn tup(&self, r: usize) -> Tup {
        Tup { kind: self.kind[r].clone(), id: self.id[r].clone(), vals: self.cols.iter().filter_map(|(c, v)| v.get(r).map(|x| (c.clone(), x))).collect() }
    }
    pub fn to_tups(&self) -> Vec<Tup> {
        (0..self.len()).map(|r| self.tup(r)).collect()
    }
}

type Ctes = HashMap<String, HashMap<String, BTreeMap<String, V>>>;

/// A set of rows for the K evaluator: its own columns, CTE columns joined by id, other kinds through subqueries.
pub struct SetView<'a> {
    pub rows: Vec<usize>,
    pub all: &'a Cols,
    pub ctes: &'a Ctes,
    pub frame: i64,
}

impl ir::Set for SetView<'_> {
    fn n(&self) -> usize {
        self.rows.len()
    }
    fn col(&self, q: &Option<String>, c: &str) -> Option<Vec<V>> {
        if q.is_none() && c == "frame" {
            return Some(vec![V::N(self.frame as f64); self.rows.len()]);
        }
        if let Some(cte) = q.as_ref().and_then(|q| self.ctes.get(q)) {
            return Some(
                self.rows
                    .iter()
                    .map(|&k| {
                        let id = &self.all.id[k];
                        match cte.get(id) {
                            Some(_) if c == "id" => V::S(id.clone()),
                            Some(r) => r.get(c).cloned().unwrap_or(V::N(f64::NAN)),
                            None => V::S("\u{0}absent".into()),
                        }
                    })
                    .collect(),
            );
        }
        if c == "id" {
            return Some(self.rows.iter().map(|&k| V::S(self.all.id[k].clone())).collect());
        }
        let col = self.all.cols.get(c);
        if !self.rows.is_empty() && !self.rows.iter().any(|&k| col.map(|v| v.has(k)).unwrap_or(false)) {
            // the renderer's defaults: no row has opacity or scale yet, and they're 1 (an expect on them reads that)
            if matches!(c, "opacity" | "scale") {
                return Some(vec![default_of(c); self.rows.len()]);
            }
            return None;
        }
        // a text column's missing cell is null (''), so `parent IS NULL` finds the rows that never had one
        let missing = |c: &str| if matches!(col, Some(Col::Any(_))) { V::S(String::new()) } else { default_of(c) };
        Some(self.rows.iter().map(|&k| col.and_then(|v| v.get(k)).unwrap_or_else(|| missing(c))).collect())
    }
    fn sub(&self, agg: &str, arg: &Expr, kind: &str, filter: Option<&Expr>) -> Result<V, String> {
        let mut view = SetView { rows: self.all.rows_of(kind), all: self.all, ctes: self.ctes, frame: self.frame };
        mask(&mut view, filter)?;
        if view.rows.is_empty() {
            return Ok(V::N(if agg == "count" { 0.0 } else { f64::NAN }));
        }
        ir::aggregate(agg, &ir::eval_set(arg, &view)?)
    }
}

pub fn mask(view: &mut SetView, f: Option<&Expr>) -> Result<(), String> {
    if let Some(f) = f {
        let m = ir::eval_set(f, view)?;
        view.rows = view.rows.iter().zip(m).filter(|(_, v)| v.truthy()).map(|(r, _)| *r).collect();
    }
    Ok(())
}

/// The rows of `kind` that `filter` selects (at `frame`).
pub fn select_cols(all: &Cols, kind: &str, filter: Option<&Expr>, frame: i64) -> Result<Vec<usize>, String> {
    let ctes = Ctes::new();
    let mut view = SetView { rows: all.rows_of(kind), all, ctes: &ctes, frame };
    mask(&mut view, filter)?;
    Ok(view.rows)
}

/// As select_cols, over rows (the tables db keeps as rows: keys, rules, transitions).
pub fn select(all: &[Tup], kind: &str, filter: Option<&Expr>, frame: i64) -> Result<Vec<usize>, String> {
    select_cols(&Cols::from_tups(all), kind, filter, frame)
}

/// One statement: CTEs over their sets, the WHERE, then every SET from the old values (SQL's rule). Returns the rows
/// it wrote.
pub fn run_system_cols(sys: &ir::System, all: &mut Cols, frame: i64) -> Result<Vec<usize>, String> {
    let (rows, writes) = system_writes(sys, all, frame)?;
    for (c, v) in writes {
        for (&k, x) in rows.iter().zip(v) {
            all.set(k, &c, x);
        }
    }
    Ok(rows)
}

/// What a statement would write, read from `all` and not applied: the rows, and per column one value per row.
#[allow(clippy::type_complexity)]
fn system_writes(sys: &ir::System, all: &Cols, frame: i64) -> Result<(Vec<usize>, Vec<(String, Vec<V>)>), String> {
    let mut ctes = Ctes::new();
    for cte in &sys.ctes {
        let mut view = SetView { rows: all.rows_of(&cte.kind), all, ctes: &ctes, frame };
        mask(&mut view, cte.filter.as_ref())?;
        let mut table: HashMap<String, BTreeMap<String, V>> = view.rows.iter().map(|&k| (all.id[k].clone(), BTreeMap::new())).collect();
        for (name, e) in &cte.cols {
            let col = ir::eval_set(e, &view)?;
            for (&k, v) in view.rows.iter().zip(col) {
                table.get_mut(&all.id[k]).unwrap().insert(name.clone(), v);
            }
        }
        ctes.insert(cte.name.clone(), table);
    }
    let mut view = SetView { rows: all.rows_of(&sys.kind), all, ctes: &ctes, frame };
    mask(&mut view, sys.filter.as_ref())?;
    let rows = view.rows.clone();
    let writes: Vec<(String, Vec<V>)> = sys.sets.iter().map(|(c, e)| ir::eval_set(e, &view).map(|v| (c.clone(), v))).collect::<Result<_, _>>()?;
    Ok((rows, writes))
}

/// The (kind, column) pairs `next:` rules write: their values carry from one frame to the next.
pub fn carried(prog: &ir::Program) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for sys in &prog.nexts {
        for (c, _) in &sys.sets {
            let kc = (sys.kind.clone(), c.clone());
            if !out.contains(&kc) {
                out.push(kc);
            }
        }
    }
    out
}

/// One step of a game: every `next:` rule reads frame `frame`'s state `prev`, all from the same state (none sees
/// another's new value), and the result is what carries into frame + 1.
pub fn step_next(prog: &ir::Program, prev: &Cols, frame: i64) -> Result<Cols, String> {
    let mut out = prev.clone();
    for sys in &prog.nexts {
        let (rows, writes) = system_writes(sys, prev, frame).map_err(|e| format!("{e}\n  in: next: {}", sys.text))?;
        for (c, v) in writes {
            for (&k, x) in rows.iter().zip(v) {
                out.set(k, &c, x);
            }
        }
    }
    Ok(out)
}

/// The carried columns' values from `from` into `all` (after the frame's keys, before its rules).
fn carry_into(all: &mut Cols, from: &Cols, carried: &[(String, String)]) {
    for (kind, c) in carried {
        for r in 0..all.len() {
            if all.kind[r] == *kind
                && let Some(v) = from.get(r, c)
            {
                all.set(r, c, v);
            }
        }
    }
}

/// The raw frames 0..n of an edit with `next:` rules, in order: frame 0 is the rows as written (keys and rules over
/// them); frame N carries what the next rules made of frame N-1, then runs N's keys and rules. Through the K kernels
/// when given, the interpreter otherwise.
pub fn folded(prog: &ir::Program, k: Option<&crate::kernel::Compiled>, base: &Cols, n: usize) -> Result<Vec<Cols>, String> {
    let carried = carried(prog);
    let mut out: Vec<Cols> = Vec::with_capacity(n);
    for t in 0..n {
        let carry = match out.last() {
            Some(prev) => Some(step_next(prog, prev, t as i64 - 1)?),
            None => None,
        };
        out.push(raw_from(prog, k, base, t as i64, carry.as_ref().map(|c| (c, carried.as_slice())))?);
    }
    Ok(out)
}

/// The keys (id, prop) -> [(frame, value, ease)] as of `frame`, eased.
pub fn key_value(pts: &[(i64, f64, String)], frame: i64) -> f64 {
    let ease = |e: &str, p: f64| match e {
        "out" => 1.0 - (1.0 - p).powi(3),
        "in" => p * p * p,
        "inout" | "in-out" => if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 },
        // held until the next key's own frame, which has the new value (`{30: 1, 31: 0} step` is down at frame 30 only)
        "step" => if p >= 1.0 { 1.0 } else { 0.0 },
        _ => p,
    };
    let i = pts.partition_point(|k| k.0 < frame);
    if i == 0 {
        pts[0].1
    } else if i == pts.len() {
        pts[pts.len() - 1].1
    } else {
        let ((f0, v0, _), (f1, v1, e)) = (&pts[i - 1], &pts[i]);
        v0 + (v1 - v0) * ease(e, (frame - f0) as f64 / (f1 - f0) as f64)
    }
}

/// A frame's state: the inserted rows, then the keys, then every rule in order, then transitions. Pure: depends only on
/// its inputs (a transition reads the untransitioned state at neighbouring frames, never the last frame drawn).
pub fn frame_cols(prog: &ir::Program, base: &Cols, frame: i64) -> Result<Cols, String> {
    // a game's frame folds from frame 0 (the timeline keeps every frame, so this is for one-off reads)
    let reach = prog.transitions.iter().map(|t| transition_window(t, frame).1).max().unwrap_or(frame).max(frame);
    let fold = if prog.nexts.is_empty() { None } else { Some(folded(prog, None, base, reach.max(0) as usize + 1)?) };
    let raw = |t: i64| -> Result<Cols, String> {
        match &fold {
            Some(f) => Ok(f[t.max(0) as usize].clone()),
            None => raw_cols(prog, base, t.max(0)),
        }
    };
    let mut state = raw(frame)?;
    if prog.transitions.is_empty() {
        return Ok(state);
    }
    let mut cache: HashMap<i64, Cols> = HashMap::new();
    for tr in &prog.transitions {
        let (lo, hi) = transition_window(tr, frame);
        for t in lo - 1..=hi {
            if !cache.contains_key(&t) {
                cache.insert(t, raw(t)?);
            }
        }
        for i in 0..state.len() {
            if state.kind[i] == tr.kind {
                if let Some(v) = transitioned(tr, frame, |t| cache[&t].num(i, &tr.col)) {
                    state.set(i, &tr.col, V::N(v));
                }
            }
        }
    }
    Ok(state)
}

/// The frames whose changes can reach `frame` through a transition, by its alignment: (lo, hi); the value before them
/// is read at lo - 1.
pub fn transition_window(tr: &ir::Transition, frame: i64) -> (i64, i64) {
    let n = tr.frames.max(1);
    match tr.align.as_str() {
        "before" => (frame + 1, frame + n),
        "center" => (frame - (n - n / 2) + 1, frame + n / 2),
        _ => (frame - n + 1, frame),
    }
}

/// One row's transitioned value at `frame`, from its untransitioned values `at(t)` around it: every change between
/// lo-1 and hi arrives along the ease, by how far through its move this frame is. None: the row has no value there.
pub fn transitioned(tr: &ir::Transition, frame: i64, at: impl Fn(i64) -> Option<f64>) -> Option<f64> {
    let n = tr.frames.max(1);
    let (lo, hi) = transition_window(tr, frame);
    let mut v = at(lo - 1)?;
    for t in lo..=hi {
        let (Some(a), Some(b)) = (at(t - 1), at(t)) else { continue };
        if a == b {
            continue;
        }
        let start = match tr.align.as_str() {
            "before" => t - n,
            "center" => t - n / 2,
            _ => t,
        };
        let p = ((frame - start) as f64 / n as f64).clamp(0.0, 1.0);
        v += (b - a) * ease_fn(&tr.ease, p);
    }
    Some(v)
}

/// As frame_cols, over rows as inserted.
pub fn frame_state(prog: &ir::Program, inserted: &[Tup], frame: i64) -> Result<Vec<Tup>, String> {
    Ok(frame_cols(prog, &Cols::from_tups(inserted), frame)?.to_tups())
}

pub fn ease_fn(e: &str, p: f64) -> f64 {
    match e {
        "out" => 1.0 - (1.0 - p).powi(3),
        "in" => p * p * p,
        "linear" => p,
        "step" => if p >= 1.0 { 1.0 } else { 0.0 },
        _ => if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 },
    }
}

/// As raw_cols, each rule through its compiled K kernel where it has one (kernel.rs), the interpreter otherwise.
pub fn raw_cols_k(prog: &ir::Program, k: &crate::kernel::Compiled, base: &Cols, frame: i64) -> Result<Cols, String> {
    raw_from(prog, Some(k), base, frame, None)
}

/// A raw frame: the base rows, the keys at `frame`, then (a game) the carried columns, then the rules.
fn raw_from(prog: &ir::Program, k: Option<&crate::kernel::Compiled>, base: &Cols, frame: i64, carry: Option<(&Cols, &[(String, String)])>) -> Result<Cols, String> {
    let Some(k) = k else {
        let mut all = base.clone();
        keys_into(prog, &mut all, frame);
        if let Some((from, cols)) = carry {
            carry_into(&mut all, from, cols);
        }
        for sys in &prog.systems {
            run_system_cols(sys, &mut all, frame).map_err(|e| format!("{e}\n  in: {}", sys.text))?;
        }
        return Ok(all);
    };
    let mut all = base.clone();
    // the keyed rows, found once per row order
    let ix = k.key_rows(prog, base);
    for ((id, prop), pts) in &prog.keys {
        if let Some(rows) = ix.get(id) {
            let v = key_value(pts, frame);
            for &r in rows {
                all.set(r, prop, V::N(v));
            }
        }
    }
    if let Some((from, cols)) = carry {
        carry_into(&mut all, from, cols);
    }
    for (sys, kern) in prog.systems.iter().zip(&k.kernels) {
        let ran = kern.as_ref().and_then(|kn| kn.run(&mut all, frame));
        match ran {
            Some(r) => r.map(|_| ()),
            None => run_system_cols(sys, &mut all, frame).map(|_| ()),
        }
        .map_err(|e| format!("{e}\n  in: {}", sys.text))?;
    }
    Ok(all)
}

/// Every key's value at `frame` into its row's column.
fn keys_into(prog: &ir::Program, all: &mut Cols, frame: i64) {
    for r in 0..all.len() {
        let id = all.id[r].clone();
        for ((kid, prop), pts) in prog.keys.range((id.clone(), String::new())..) {
            if *kid != id {
                break;
            }
            all.set(r, prop, V::N(key_value(pts, frame)));
        }
    }
}

/// The state before transitions: the base rows, keys, rules (the interpreter: the reference the kernels match).
pub fn raw_cols(prog: &ir::Program, base: &Cols, frame: i64) -> Result<Cols, String> {
    raw_from(prog, None, base, frame, None)
}
