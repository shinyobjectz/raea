//! Rules compiled to K kernels over typed columns (docs/substrate.feature, phase 3). A rule (an UPDATE run every
//! frame) becomes a fused per-row loop over f64 column slices: arithmetic, comparisons, functions and unordered
//! window aggregates run natively, in the interpreter's order of operations and rows, so every value equals the
//! interpreter's (eval.rs stays the reference). Text equality against a literal is read in place. What a kernel
//! doesn't compile (ordered windows, subqueries, text results) runs through the interpreter as a leaf; a rule with
//! CTEs or text concatenation runs through the interpreter whole. Leaves and filters that read neither `frame` nor a
//! column any key or rule writes are computed once per timeline, not once per frame. Each kernel prints as K.

use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

use crate::eval::{self, Col, Cols, SetView};
use crate::ir::{self, Expr, Program, System, V};

/// A numeric expression per row.
enum N {
    Lit(f64),
    Frame,
    /// a column, by its slot (resolved to the column once per run)
    Col(usize),
    Bin(Op, Box<N>, Box<N>),
    Neg(Box<N>),
    Not(Box<N>),
    Call(String, Vec<N>),
    /// an unordered aggregate over the set (count, sum, avg, max, min of the argument)
    Agg(String, Option<Box<N>>),
    /// col = 'text' (or <>): compared in place
    TextEq { col: usize, lit: String, neg: bool },
    /// computed by the interpreter over the set (one value per row), cached when invariant
    Leaf(usize),
}

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

struct LeafSpec {
    expr: Expr,
    invariant: bool,
    cache: OnceLock<Vec<f64>>,
}

enum SetE {
    Num(N),
    /// a value the kernel can't type (text): the interpreter's, written as it is
    Interp(Expr),
}

/// One rule as a kernel.
pub struct Kernel {
    kind: String,
    filter: Option<N>,
    filter_invariant: bool,
    rows_cache: OnceLock<Vec<usize>>,
    /// the kind's rows, for the row order (kinds) they were found in
    kind_cache: OnceLock<(usize, Vec<usize>)>,
    sets: Vec<(String, SetE)>,
    leaves: Vec<LeafSpec>,
    /// the columns it reads, by slot (and, for an ordered window, the column it orders by: checked like a read, so the
    /// kernel falls back where the interpreter would refuse a column no row of the set has)
    slots: Vec<String>,
    order_reads: BTreeSet<String>,
    /// the rule as K
    pub k: String,
}

/// The program's rules compiled: a kernel per rule, or None where the rule runs through the interpreter.
pub struct Compiled {
    pub kernels: Vec<Option<Kernel>>,
    /// rows by id for the keys (built once for a row order)
    key_rows: OnceLock<(usize, std::collections::HashMap<String, Vec<usize>>)>,
}

impl Compiled {
    pub fn new(prog: &Program) -> Compiled {
        // what varies from frame to frame: every column a key or a rule writes
        let mut varying: HashSet<String> = prog.keys.keys().map(|(_, p)| p.clone()).collect();
        for s in &prog.systems {
            for (c, _) in &s.sets {
                varying.insert(c.clone());
            }
        }
        Compiled { kernels: prog.systems.iter().map(|s| compile(s, &varying)).collect(), key_rows: OnceLock::new() }
    }

    /// The rows each keyed id names, for this row order.
    pub fn key_rows(&self, prog: &Program, base: &Cols) -> std::borrow::Cow<'_, std::collections::HashMap<String, Vec<usize>>> {
        let key = std::sync::Arc::as_ptr(&base.id) as usize;
        let build = || {
            let ids: HashSet<&str> = prog.keys.keys().map(|(id, _)| id.as_str()).collect();
            let mut m: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
            for (r, id) in base.id.iter().enumerate() {
                if ids.contains(id.as_str()) {
                    m.entry(id.clone()).or_default().push(r);
                }
            }
            m
        };
        match self.key_rows.get() {
            Some((k, m)) if *k == key => std::borrow::Cow::Borrowed(m),
            Some(_) => std::borrow::Cow::Owned(build()),
            None => {
                let _ = self.key_rows.set((key, build()));
                std::borrow::Cow::Borrowed(&self.key_rows.get().unwrap().1)
            }
        }
    }

    /// The rules as K, one per line ("-- interpreted" where a rule runs through the interpreter).
    pub fn k_text(&self, prog: &Program) -> String {
        prog.systems.iter().zip(&self.kernels).map(|(s, k)| match k {
            Some(k) => k.k.clone(),
            None => format!("/ interpreted: {}", s.text.split_whitespace().collect::<Vec<_>>().join(" ")),
        }).collect::<Vec<_>>().join("\n")
    }
}

fn compile(sys: &System, varying: &HashSet<String>) -> Option<Kernel> {
    if !sys.ctes.is_empty() {
        return None;
    }
    let mut c = Comp { leaves: vec![], reads: BTreeSet::new(), slots: vec![], varying };
    let filter = match &sys.filter {
        Some(f) => Some(c.num(f)?),
        None => None,
    };
    let filter_invariant = sys.filter.as_ref().map(|f| invariant(f, varying)).unwrap_or(true);
    let mut sets = vec![];
    for (col, e) in &sys.sets {
        sets.push((col.clone(), match c.num(e) {
            Some(n) if !produces_text(e) => SetE::Num(n),
            _ => SetE::Interp(e.clone()),
        }));
    }
    let k = format!(
        "{}{}",
        sys.sets.iter().map(|(col, e)| format!("{}.{col}:{}", sys.kind, kx(e))).collect::<Vec<_>>().join("; "),
        sys.filter.as_ref().map(|f| format!(" / where {}", kx(f))).unwrap_or_default()
    );
    Some(Kernel { kind: sys.kind.clone(), filter, filter_invariant, rows_cache: OnceLock::new(), kind_cache: OnceLock::new(), sets, leaves: c.leaves, slots: c.slots, order_reads: c.reads, k })
}

/// Does the expression produce text (so a numeric kernel can't hold it)?
fn produces_text(e: &Expr) -> bool {
    match e {
        Expr::Lit(V::S(_)) => true,
        Expr::Col(..) => false, // a column read in a SET: numeric unless the column holds text, checked at run time
        Expr::Bin(op, ..) => op == "||",
        _ => false,
    }
}

/// Reads neither `frame` nor a varying column (so its value is the same at every frame).
fn invariant(e: &Expr, varying: &HashSet<String>) -> bool {
    match e {
        Expr::Lit(_) => true,
        Expr::Col(q, c) => q.is_none() && c != "frame" && !varying.contains(c),
        Expr::Bin(_, a, b) => invariant(a, varying) && invariant(b, varying),
        Expr::Neg(a) | Expr::Not(a) => invariant(a, varying),
        Expr::Call(_, args) => args.iter().all(|a| invariant(a, varying)),
        Expr::Sub { arg, filter, .. } => invariant(arg, varying) && filter.as_deref().map(|f| invariant(f, varying)).unwrap_or(true),
        Expr::Window { arg, order, .. } => arg.as_deref().map(|a| invariant(a, varying)).unwrap_or(true) && order.as_ref().map(|o| !varying.contains(o)).unwrap_or(true),
    }
}

/// The functions a kernel evaluates (ev's Call arms).
const NUM_FNS: &[&str] = &["pi", "least", "greatest", "pow", "power", "sin", "cos", "tan", "abs", "sqrt", "floor", "ceil", "round", "exp", "ln"];

struct Comp<'a> {
    leaves: Vec<LeafSpec>,
    reads: BTreeSet<String>,
    slots: Vec<String>,
    varying: &'a HashSet<String>,
}

impl Comp<'_> {
    fn slot(&mut self, c: &str) -> usize {
        match self.slots.iter().position(|x| x == c) {
            Some(k) => k,
            None => {
                self.slots.push(c.to_string());
                self.slots.len() - 1
            }
        }
    }

    fn leaf(&mut self, e: &Expr, set_invariant: bool) -> N {
        let invariant = set_invariant && invariant(e, self.varying);
        self.leaves.push(LeafSpec { expr: e.clone(), invariant, cache: OnceLock::new() });
        N::Leaf(self.leaves.len() - 1)
    }

    fn num(&mut self, e: &Expr) -> Option<N> {
        Some(match e {
            Expr::Lit(v) => N::Lit(v.num()),
            Expr::Col(Some(_), _) => return None,
            Expr::Col(None, c) if c == "frame" => N::Frame,
            Expr::Col(None, c) if c == "id" => return None,
            Expr::Col(None, c) => N::Col(self.slot(c)),
            Expr::Bin(op, a, b) => {
                // text comparisons: a column against a literal in place; anything else textual isn't compiled
                if op == "=" || op == "<>" {
                    if let (Expr::Col(None, c), Expr::Lit(V::S(s))) | (Expr::Lit(V::S(s)), Expr::Col(None, c)) = (a.as_ref(), b.as_ref()) {
                        if c != "frame" {
                            return Some(N::TextEq { col: self.slot(c), lit: s.clone(), neg: op == "<>" });
                        }
                    }
                }
                if op == "||" || matches!(a.as_ref(), Expr::Lit(V::S(_))) || matches!(b.as_ref(), Expr::Lit(V::S(_))) || is_id(a) || is_id(b) {
                    return None;
                }
                let o = match op.as_str() {
                    "+" => Op::Add,
                    "-" => Op::Sub,
                    "*" => Op::Mul,
                    "/" => Op::Div,
                    "%" => Op::Mod,
                    "<" => Op::Lt,
                    "<=" => Op::Le,
                    ">" => Op::Gt,
                    ">=" => Op::Ge,
                    "=" => Op::Eq,
                    "<>" => Op::Ne,
                    "and" => Op::And,
                    "or" => Op::Or,
                    _ => return None,
                };
                N::Bin(o, Box::new(self.num(a)?), Box::new(self.num(b)?))
            }
            Expr::Neg(a) => N::Neg(Box::new(self.num(a)?)),
            Expr::Not(a) => N::Not(Box::new(self.num(a)?)),
            // numeric functions only; text functions (upper, like, ...) are the interpreter's
            Expr::Call(f, args) if NUM_FNS.contains(&f.as_str()) => N::Call(f.clone(), args.iter().map(|a| self.num(a)).collect::<Option<_>>()?),
            Expr::Call(..) => return None,
            Expr::Window { f, arg, order: None } if matches!(f.as_str(), "count" | "sum" | "avg" | "max" | "min") => {
                let a = match arg {
                    Some(a) => Some(Box::new(self.num(a)?)),
                    None => None,
                };
                N::Agg(f.clone(), a)
            }
            // ordered windows and subqueries: the interpreter's, over the set
            Expr::Window { .. } | Expr::Sub { .. } => {
                if let Expr::Window { order: Some(o), .. } = e {
                    if o != "id" && o != "frame" {
                        self.reads.insert(o.clone());
                    }
                }
                self.leaf(e, true)
            }
        })
    }
}

fn is_id(e: &Expr) -> bool {
    matches!(e, Expr::Col(None, c) if c == "id")
}

/// An expression as K (right to left, verbs as K spells them).
fn kx(e: &Expr) -> String {
    match e {
        Expr::Lit(V::N(x)) => format!("{x}"),
        Expr::Lit(V::S(s)) => format!("{s:?}"),
        Expr::Col(q, c) => q.as_ref().map(|q| format!("{q}.{c}")).unwrap_or_else(|| c.clone()),
        Expr::Bin(op, a, b) => {
            let v = match op.as_str() {
                "/" => "%",
                "%" => "!",
                "and" => "&",
                "or" => "|",
                "<>" => "~=",
                "||" => ",",
                o => o,
            };
            format!("({}{v}{})", kx(a), kx(b))
        }
        Expr::Neg(a) => format!("-{}", kx(a)),
        Expr::Not(a) => format!("~{}", kx(a)),
        Expr::Call(f, args) => format!("{f}[{}]", args.iter().map(kx).collect::<Vec<_>>().join(";")),
        Expr::Window { f, arg, order } => {
            let a = arg.as_deref().map(kx).unwrap_or("1".into());
            match (f.as_str(), order) {
                ("row_number", Some(o)) => format!("1+<<{o}"),
                ("count", None) => "#rows".into(),
                ("sum", None) => format!("+/{a}"),
                ("max", None) => format!("|/{a}"),
                ("min", None) => format!("&/{a}"),
                ("avg", None) => format!("avg {a}"),
                ("sum", Some(o)) | ("count", Some(o)) => format!("+\\{a}@<{o}"),
                (f, o) => format!("{f}[{a}]{}", o.as_ref().map(|o| format!(" by {o}")).unwrap_or_default()),
            }
        }
        Expr::Sub { agg, arg, kind, filter } => format!("{agg}[{}] from {kind}{}", kx(arg), filter.as_deref().map(|f| format!(" where {}", kx(f))).unwrap_or_default()),
    }
}

/// What a kernel reads while it runs over one frame.
struct Ctx<'a> {
    /// each slot's column (None: no row has it) and its value where a row lacks it
    refs: Vec<Option<&'a Col>>,
    defaults: Vec<f64>,
    rows: &'a [usize],
    frame: f64,
    leaves: Vec<Option<std::borrow::Cow<'a, [f64]>>>,
}

fn num_at(col: Option<&Col>, r: usize, default: f64) -> f64 {
    match col {
        Some(Col::Num { v, has }) => {
            if has[r] {
                v[r]
            } else {
                default
            }
        }
        Some(c) => c.num(r).unwrap_or(default),
        None => default,
    }
}

/// A node's value over the set: one scalar (literals, the frame, aggregates) or one value per row.
enum Vl {
    S(f64),
    V(Vec<f64>),
}

fn unary(v: Vl, f: impl Fn(f64) -> f64) -> Vl {
    match v {
        Vl::S(x) => Vl::S(f(x)),
        Vl::V(mut a) => {
            for x in a.iter_mut() {
                *x = f(*x);
            }
            Vl::V(a)
        }
    }
}

fn binary(a: Vl, b: Vl, f: impl Fn(f64, f64) -> f64) -> Vl {
    match (a, b) {
        (Vl::S(x), Vl::S(y)) => Vl::S(f(x, y)),
        (Vl::V(mut a), Vl::S(y)) => {
            for x in a.iter_mut() {
                *x = f(*x, y);
            }
            Vl::V(a)
        }
        (Vl::S(x), Vl::V(mut b)) => {
            for y in b.iter_mut() {
                *y = f(x, *y);
            }
            Vl::V(b)
        }
        (Vl::V(mut a), Vl::V(b)) => {
            for (x, y) in a.iter_mut().zip(b) {
                *x = f(*x, y);
            }
            Vl::V(a)
        }
    }
}

/// One verb over two values: the op is matched once, so each loop is its own inlined body (not a call per element).
fn bin_op(op: Op, a: Vl, b: Vl) -> Vl {
    match op {
        Op::Add => binary(a, b, |x, y| x + y),
        Op::Sub => binary(a, b, |x, y| x - y),
        Op::Mul => binary(a, b, |x, y| x * y),
        Op::Div => binary(a, b, |x, y| x / y),
        Op::Mod => binary(a, b, |x, y| x % y),
        Op::Lt => binary(a, b, |x, y| (x < y) as u8 as f64),
        Op::Le => binary(a, b, |x, y| (x <= y) as u8 as f64),
        Op::Gt => binary(a, b, |x, y| (x > y) as u8 as f64),
        Op::Ge => binary(a, b, |x, y| (x >= y) as u8 as f64),
        Op::Eq => binary(a, b, |x, y| (x == y) as u8 as f64),
        Op::Ne => binary(a, b, |x, y| (x != y) as u8 as f64),
        Op::And => binary(a, b, |x, y| (x != 0.0 && y != 0.0) as u8 as f64),
        Op::Or => binary(a, b, |x, y| (x != 0.0 || y != 0.0) as u8 as f64),
    }
}

/// A node over the whole set at once (K's way: one tight loop per verb, scalars stay scalars).
fn ev(n: &N, ctx: &Ctx) -> Vl {
    let rows = ctx.rows;
    match n {
        N::Lit(x) => Vl::S(*x),
        N::Frame => Vl::S(ctx.frame),
        N::Col(k) => {
            let (col, d) = (ctx.refs[*k], ctx.defaults[*k]);
            Vl::V(match col {
                Some(Col::Num { v, has }) => rows.iter().map(|&r| if has[r] { v[r] } else { d }).collect(),
                _ => rows.iter().map(|&r| num_at(col, r, d)).collect(),
            })
        }
        N::Bin(op, a, b) => bin_op(*op, ev(a, ctx), ev(b, ctx)),
        N::Neg(a) => unary(ev(a, ctx), |x| -x),
        N::Not(a) => unary(ev(a, ctx), |x| (x == 0.0) as u8 as f64),
        N::Call(f, args) => match (f.as_str(), args.len()) {
            ("pi", _) => Vl::S(std::f64::consts::PI),
            ("least", _) | ("greatest", _) => {
                // folded left to right from the identity, as the interpreter does
                let least = f == "least";
                let mut acc = Vl::S(if least { f64::INFINITY } else { f64::NEG_INFINITY });
                for a in args {
                    acc = if least { binary(acc, ev(a, ctx), f64::min) } else { binary(acc, ev(a, ctx), f64::max) };
                }
                acc
            }
            ("pow", _) | ("power", _) => {
                let a = args.first().map(|a| ev(a, ctx)).unwrap_or(Vl::S(0.0));
                let b = args.get(1).map(|a| ev(a, ctx)).unwrap_or(Vl::S(0.0));
                binary(a, b, f64::powf)
            }
            (f, _) => {
                let a = args.first().map(|a| ev(a, ctx)).unwrap_or(Vl::S(0.0));
                match f {
                    "sin" => unary(a, f64::sin),
                    "cos" => unary(a, f64::cos),
                    "tan" => unary(a, f64::tan),
                    "abs" => unary(a, f64::abs),
                    "sqrt" => unary(a, f64::sqrt),
                    "floor" => unary(a, f64::floor),
                    "ceil" => unary(a, f64::ceil),
                    "round" => unary(a, f64::round),
                    "exp" => unary(a, f64::exp),
                    "ln" => unary(a, f64::ln),
                    _ => unary(a, |_| f64::NAN),
                }
            }
        },
        N::Agg(f, arg) => {
            let nr = rows.len();
            if f == "count" {
                return Vl::S(nr as f64);
            }
            let x: Vec<f64> = match arg.as_ref().map(|a| ev(a, ctx)) {
                Some(Vl::V(v)) => v,
                Some(Vl::S(s)) => vec![s; nr],
                None => vec![1.0; nr],
            };
            Vl::S(match f.as_str() {
                "count" => nr as f64,
                "sum" => x.iter().sum(),
                "avg" => x.iter().sum::<f64>() / nr.max(1) as f64,
                "max" => x.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                _ => x.iter().cloned().fold(f64::INFINITY, f64::min),
            })
        }
        N::TextEq { col, lit, neg } => {
            let c = ctx.refs[*col];
            Vl::V(rows.iter().map(|&r| {
                let eq = match c {
                    // a missing cell is null (''): IS NULL compiles to = ''
                    Some(Col::Any(x)) => match &x[r] {
                        Some(V::S(s)) => s == lit,
                        None => lit.is_empty(),
                        _ => false,
                    },
                    Some(Col::Num { has, .. }) => !has[r] && lit.is_empty(),
                    None => lit.is_empty(),
                };
                (eq != *neg) as u8 as f64
            }).collect())
        }
        N::Leaf(k) => Vl::V(ctx.leaves[*k].as_ref().map(|v| v.to_vec()).unwrap_or_else(|| vec![f64::NAN; rows.len()])),
    }
}

fn values(v: Vl, n: usize) -> Vec<f64> {
    match v {
        Vl::V(v) => v,
        Vl::S(x) => vec![x; n],
    }
}

impl Kernel {
    /// Run over `all` at `frame`. None: this frame falls back to the interpreter (a read the interpreter would refuse,
    /// a column holding text where a number is computed); the caller runs eval::run_system_cols.
    pub fn run(&self, all: &mut Cols, frame: i64) -> Option<Result<Vec<usize>, String>> {
        // the kind's rows: found once for a given row order (frames share their base's kinds)
        let key = std::sync::Arc::as_ptr(&all.kind) as usize;
        let kind_rows: Vec<usize> = match self.kind_cache.get() {
            Some((k, r)) if *k == key => r.clone(),
            Some(_) => all.rows_of(&self.kind),
            None => {
                let r = all.rows_of(&self.kind);
                let _ = self.kind_cache.set((key, r.clone()));
                r
            }
        };
        // the interpreter refuses a column no row of the kind has: so does the kernel (it falls back to say so)
        for c in self.slots.iter().chain(self.order_reads.iter()).filter(|c| !matches!(c.as_str(), "opacity" | "scale")) {
            if !kind_rows.is_empty() && !kind_rows.iter().any(|&r| all.cols.get(c).map(|col| col.has(r)).unwrap_or(false)) {
                return None;
            }
        }
        let defaults: Vec<f64> = self.slots.iter().map(|c| eval::default_of(c).num()).collect();
        let frame_f = frame as f64;
        // the WHERE: once per timeline when it can't change
        let rows: Vec<usize> = match &self.filter {
            None => kind_rows,
            Some(f) => {
                let compute = |all: &Cols| -> Option<Vec<usize>> {
                    let refs = self.slots.iter().map(|c| all.cols.get(c)).collect();
                    let mut ctx = Ctx { refs, defaults: defaults.clone(), rows: &kind_rows, frame: frame_f, leaves: vec![None; self.leaves.len()] };
                    for (k, l) in self.leaves.iter().enumerate() {
                        // leaves inside the WHERE run over the kind's rows
                        if leaf_in(f, k) {
                            ctx.leaves[k] = Some(std::borrow::Cow::Owned(leaf_values(&l.expr, all, &kind_rows, frame)?));
                        }
                    }
                    let m = values(ev(f, &ctx), kind_rows.len());
                    Some((0..kind_rows.len()).filter(|&i| m[i] != 0.0).map(|i| kind_rows[i]).collect())
                };
                if self.filter_invariant {
                    if let Some(r) = self.rows_cache.get() {
                        r.clone()
                    } else {
                        let r = compute(all)?;
                        let _ = self.rows_cache.set(r.clone());
                        r
                    }
                } else {
                    compute(all)?
                }
            }
        };
        let refs = self.slots.iter().map(|c| all.cols.get(c)).collect();
        let mut ctx = Ctx { refs, defaults, rows: &rows, frame: frame_f, leaves: Vec::with_capacity(self.leaves.len()) };
        for l in &self.leaves {
            let v: std::borrow::Cow<[f64]> = if l.invariant {
                match l.cache.get() {
                    Some(v) => std::borrow::Cow::Borrowed(v.as_slice()),
                    None => {
                        let v = leaf_values(&l.expr, all, &rows, frame)?;
                        let _ = l.cache.set(v);
                        std::borrow::Cow::Borrowed(l.cache.get().unwrap().as_slice())
                    }
                }
            } else {
                std::borrow::Cow::Owned(leaf_values(&l.expr, all, &rows, frame)?)
            };
            ctx.leaves.push(Some(v));
        }
        // every SET from the old values, then the writes (SQL's rule)
        enum W {
            Num(Vec<f64>),
            Any(Vec<V>),
        }
        let mut writes: Vec<(&str, W)> = Vec::with_capacity(self.sets.len());
        for (col, e) in &self.sets {
            match e {
                SetE::Num(n) => {
                    writes.push((col, W::Num(values(ev(n, &ctx), rows.len()))));
                }
                SetE::Interp(e) => {
                    let ctes = Default::default();
                    let view = SetView { rows: rows.clone(), all, ctes: &ctes, frame };
                    match ir::eval_set(e, &view) {
                        Ok(v) => writes.push((col, W::Any(v))),
                        Err(err) => return Some(Err(err)),
                    }
                }
            }
        }
        drop(ctx);
        let n = all.len();
        for (col, w) in writes {
            match w {
                // numbers straight into a numeric column
                W::Num(v) => match all.cols.entry(col.to_string()).or_insert_with(|| Col::Num { v: vec![0.0; n], has: vec![false; n] }) {
                    Col::Num { v: cv, has } => {
                        for (&r, x) in rows.iter().zip(v) {
                            cv[r] = x;
                            has[r] = true;
                        }
                    }
                    c => {
                        for (&r, x) in rows.iter().zip(v) {
                            c.set(r, V::N(x));
                        }
                    }
                },
                W::Any(v) => {
                    for (&r, x) in rows.iter().zip(v) {
                        all.set(r, col, x);
                    }
                }
            }
        }
        Some(Ok(rows))
    }
}

fn leaf_in(n: &N, k: usize) -> bool {
    match n {
        N::Leaf(j) => *j == k,
        N::Bin(_, a, b) => leaf_in(a, k) || leaf_in(b, k),
        N::Neg(a) | N::Not(a) => leaf_in(a, k),
        N::Call(_, args) => args.iter().any(|a| leaf_in(a, k)),
        N::Agg(_, Some(a)) => leaf_in(a, k),
        _ => false,
    }
}

/// A leaf by the interpreter over a set; None if it isn't numeric-safe (the frame falls back).
fn leaf_values(e: &Expr, all: &Cols, rows: &[usize], frame: i64) -> Option<Vec<f64>> {
    let ctes = Default::default();
    let view = SetView { rows: rows.to_vec(), all, ctes: &ctes, frame };
    let v = ir::eval_set(e, &view).ok()?;
    Some(v.iter().map(V::num).collect())
}
