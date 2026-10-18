//! K on GPU-resident arrays. Values are integer vectors held by a backend (the GPU, or the CPU as the reference);
//! element-wise verbs build a lazy tree that is compiled into one kernel when something forces it, so `+/x*y+z`
//! is one fused pass then one reduction, and nothing comes back to the host until it's read.
//!
//! The language is K's core, right to left, no precedence:
//!   nouns       1  1 2 3  name  (expr)  {lambda with x y z}
//!   verbs       + - * % ! & | < > = ~ , # _ @ ^ (dyadic and monadic; see `dyad` and `monad`)
//!   adverbs     v/ over, v\ scan, v': each-prior, f' each (f a lambda over scalars)
//!   apply       f[a;b]  x@i  x[i]
//!   statements  a:expr  separated by ; or newlines; the last value is the result
//! Integers only (i32, wrapping): every result, reductions included, is exact in any order.

use std::collections::HashMap;
use std::rc::Rc;

// === values ===

/// A fused element-wise expression over leaves (forced arrays) and the element index.
#[derive(Debug, Clone)]
pub enum F {
    Leaf(usize),
    Idx,
    Lit(i32),
    Un(char, Box<F>),
    Bin(char, Box<F>, Box<F>),
    /// leaf[k] for an index expression; out of range reads 0 (K's null)
    Gather(usize, Box<F>),
}

/// A vector: forced (a handle the backend holds), or lazy (a fused expression over forced leaves).
#[derive(Clone)]
pub enum Arr {
    Forced(Rc<dyn Handle>),
    Lazy { f: F, leaves: Vec<Rc<dyn Handle>>, len: usize },
}

pub trait Handle {
    fn len(&self) -> usize;
    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Clone)]
pub enum Val {
    Int(i32),
    Vec(Arr),
    Lambda(Rc<Lambda>),
    Verb(char, Option<String>),
}

#[derive(Debug)]
pub struct Lambda {
    pub body: Vec<Stmt>,
}

impl Arr {
    pub fn len(&self) -> usize {
        match self {
            Arr::Forced(h) => h.len(),
            Arr::Lazy { len, .. } => *len,
        }
    }
    /// As a fused expression (a forced array is a leaf of itself).
    fn fused(&self) -> (F, Vec<Rc<dyn Handle>>, usize) {
        match self {
            Arr::Forced(h) => (F::Leaf(0), vec![h.clone()], h.len()),
            Arr::Lazy { f, leaves, len } => (f.clone(), leaves.clone(), *len),
        }
    }
}

fn shift(f: &F, by: usize) -> F {
    match f {
        F::Leaf(k) => F::Leaf(k + by),
        F::Gather(k, e) => F::Gather(k + by, Box::new(shift(e, by))),
        F::Un(o, a) => F::Un(*o, Box::new(shift(a, by))),
        F::Bin(o, a, b) => F::Bin(*o, Box::new(shift(a, by)), Box::new(shift(b, by))),
        other => other.clone(),
    }
}

// === backends ===

/// What a backend does: everything that can't be fused, plus running a fused expression.
pub trait Backend {
    fn name(&self) -> &'static str;
    fn upload(&self, v: &[i32]) -> Rc<dyn Handle>;
    fn download(&self, h: &Rc<dyn Handle>) -> Vec<i32>;
    /// out[i] = f(i) for i in 0..len
    fn eval(&self, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> Rc<dyn Handle>;
    /// v/ for v in + * & | (sum, product, min, max), wrapping, over a fused expression (nothing materialised)
    fn reduce(&self, op: char, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> i32;
    /// one element, read back alone
    fn download_one(&self, h: &Rc<dyn Handle>, i: usize) -> i32 {
        self.download(h)[i]
    }
    /// v\ inclusive scan
    fn scan(&self, op: char, h: &Rc<dyn Handle>) -> Rc<dyn Handle>;
    /// &x: the indices where x is non-zero, each repeated x[i] times for x[i] > 1 (K's where on counts)
    fn wher(&self, h: &Rc<dyn Handle>) -> Rc<dyn Handle>;
    /// <x (asc) or >x (desc): the stable grade
    fn grade(&self, h: &Rc<dyn Handle>, desc: bool) -> Rc<dyn Handle>;
    /// x,y
    fn join(&self, a: &Rc<dyn Handle>, b: &Rc<dyn Handle>) -> Rc<dyn Handle>;
}

/// The reference: the same operations, one element at a time, in order.
pub struct Cpu;

pub struct CpuVec(pub Vec<i32>);

impl Handle for CpuVec {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn cv(h: &Rc<dyn Handle>) -> &[i32] {
    &h.as_any().downcast_ref::<CpuVec>().expect("a CPU vector").0
}

pub fn bin(op: char, a: i32, b: i32) -> i32 {
    match op {
        '+' => a.wrapping_add(b),
        '-' => a.wrapping_sub(b),
        '*' => a.wrapping_mul(b),
        '%' => {
            if b == 0 {
                0
            } else {
                a.wrapping_div(b)
            }
        }
        '!' => {
            if b == 0 {
                0
            } else {
                a.wrapping_rem(b)
            }
        }
        '&' => a.min(b),
        '|' => a.max(b),
        '<' => (a < b) as i32,
        '>' => (a > b) as i32,
        '=' => (a == b) as i32,
        '^' => a ^ b,
        _ => panic!("no dyad {op}"),
    }
}

pub fn un(op: char, a: i32) -> i32 {
    match op {
        '-' => a.wrapping_neg(),
        '~' => (a == 0) as i32,
        'a' => a.wrapping_abs(),
        _ => panic!("no monad {op}"),
    }
}

fn ident(op: char) -> i32 {
    match op {
        '+' => 0,
        '*' => 1,
        '&' => i32::MAX,
        '|' => i32::MIN,
        _ => panic!("no identity for {op}"),
    }
}

pub fn eval_f(f: &F, leaves: &[&[i32]], i: usize) -> i32 {
    match f {
        F::Leaf(k) => leaves[*k][i],
        F::Idx => i as i32,
        F::Lit(v) => *v,
        F::Un(o, a) => un(*o, eval_f(a, leaves, i)),
        F::Bin(o, a, b) => bin(*o, eval_f(a, leaves, i), eval_f(b, leaves, i)),
        F::Gather(k, e) => {
            let j = eval_f(e, leaves, i);
            if j >= 0 && (j as usize) < leaves[*k].len() { leaves[*k][j as usize] } else { 0 }
        }
    }
}

impl Backend for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
    }
    fn upload(&self, v: &[i32]) -> Rc<dyn Handle> {
        Rc::new(CpuVec(v.to_vec()))
    }
    fn download(&self, h: &Rc<dyn Handle>) -> Vec<i32> {
        cv(h).to_vec()
    }
    fn eval(&self, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> Rc<dyn Handle> {
        let ls: Vec<&[i32]> = leaves.iter().map(cv).collect();
        Rc::new(CpuVec((0..len).map(|i| eval_f(f, &ls, i)).collect()))
    }
    fn reduce(&self, op: char, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> i32 {
        let ls: Vec<&[i32]> = leaves.iter().map(cv).collect();
        (0..len).fold(ident(op), |a, i| bin(op, a, eval_f(f, &ls, i)))
    }
    fn scan(&self, op: char, h: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let mut acc = ident(op);
        Rc::new(CpuVec(
            cv(h)
                .iter()
                .map(|x| {
                    acc = bin(op, acc, *x);
                    acc
                })
                .collect(),
        ))
    }
    fn wher(&self, h: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let mut out = Vec::new();
        for (i, c) in cv(h).iter().enumerate() {
            for _ in 0..(*c).max(0) {
                out.push(i as i32);
            }
        }
        Rc::new(CpuVec(out))
    }
    fn grade(&self, h: &Rc<dyn Handle>, desc: bool) -> Rc<dyn Handle> {
        let v = cv(h);
        let mut idx: Vec<i32> = (0..v.len() as i32).collect();
        if desc {
            idx.sort_by(|a, b| v[*b as usize].cmp(&v[*a as usize]).then(a.cmp(b)));
        } else {
            idx.sort_by(|a, b| v[*a as usize].cmp(&v[*b as usize]).then(a.cmp(b)));
        }
        Rc::new(CpuVec(idx))
    }
    fn join(&self, a: &Rc<dyn Handle>, b: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let mut v = cv(a).to_vec();
        v.extend_from_slice(cv(b));
        Rc::new(CpuVec(v))
    }
}

// === parsing ===

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i32),
    Ints(Vec<i32>),
    Name(String),
    Lambda(Rc<Lambda>),
    /// a verb with an optional adverb, used as a noun (e.g. passed as a value) or applied
    Verb(char, Option<String>),
    Monad(Box<Expr>, Box<Expr>),
    Dyad(Box<Expr>, Box<Expr>, Box<Expr>),
    Apply(Box<Expr>, Vec<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Assign(String, Expr),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
enum T {
    Int(i64),
    Name(String),
    Verb(char),
    Adverb(String),
    LP,
    RP,
    LB,
    RB,
    LC,
    RC,
    Semi,
    Colon,
}

fn lex(s: &str) -> Result<Vec<T>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == ' ' || ch == '\t' {
            i += 1;
            continue;
        }
        if ch == '/' && (i == 0 || c[i - 1] == ' ' || c[i - 1] == '\n') {
            // a comment: / at the start of a line or after a space
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '\n' {
            out.push(T::Semi);
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '-' && i + 1 < c.len() && c[i + 1].is_ascii_digit() && (i == 0 || matches!(c[i - 1], ' ' | '(' | '[' | ';' | '{'))) {
            let st = i;
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            out.push(T::Int(c[st..i].iter().collect::<String>().parse().map_err(|e| format!("{e}"))?));
            continue;
        }
        if ch.is_alphabetic() {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' && i + 1 < c.len() && c[i + 1].is_alphanumeric()) {
                i += 1;
            }
            out.push(T::Name(c[st..i].iter().collect()));
            continue;
        }
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        if two == "':" {
            out.push(T::Adverb("':".into()));
            i += 2;
            continue;
        }
        out.push(match ch {
            '(' => T::LP,
            ')' => T::RP,
            '[' => T::LB,
            ']' => T::RB,
            '{' => T::LC,
            '}' => T::RC,
            ';' => T::Semi,
            ':' => T::Colon,
            '/' | '\\' | '\'' => T::Adverb(ch.to_string()),
            '+' | '-' | '*' | '%' | '!' | '&' | '|' | '<' | '>' | '=' | '~' | ',' | '#' | '_' | '@' | '^' => T::Verb(ch),
            other => return Err(format!("unexpected '{other}'")),
        });
        i += 1;
    }
    Ok(out)
}

struct P {
    t: Vec<T>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&T> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<T> {
        self.i += 1;
        self.t.get(self.i - 1).cloned()
    }
    fn stmts(&mut self, until: Option<T>) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        loop {
            while self.peek() == Some(&T::Semi) {
                self.next();
            }
            if self.peek().is_none() || self.peek() == until.as_ref() {
                break;
            }
            if let (Some(T::Name(n)), Some(T::Colon)) = (self.t.get(self.i).cloned(), self.t.get(self.i + 1).cloned()) {
                self.i += 2;
                out.push(Stmt::Assign(n, self.expr()?));
            } else {
                out.push(Stmt::Expr(self.expr()?));
            }
        }
        Ok(out)
    }
    fn starts_noun(t: Option<&T>) -> bool {
        matches!(t, Some(T::Int(_)) | Some(T::Name(_)) | Some(T::LP) | Some(T::LC))
    }
    /// a verb, with any adverb after it
    fn verb(&mut self, v: char) -> Expr {
        if let Some(T::Adverb(a)) = self.peek().cloned() {
            self.next();
            return Expr::Verb(v, Some(a));
        }
        Expr::Verb(v, None)
    }
    /// expr := verb expr | noun (verb expr)?    (a verb with nothing on its left is monadic)
    fn expr(&mut self) -> Result<Expr, String> {
        if let Some(T::Verb(v)) = self.peek().cloned() {
            self.next();
            let vb = self.verb(v);
            if !Self::starts_noun(self.peek()) && !matches!(self.peek(), Some(T::Verb(_))) {
                return Ok(vb); // a bare verb, as a value
            }
            return Ok(Expr::Monad(Box::new(vb), Box::new(self.expr()?)));
        }
        let n = self.noun()?;
        // a noun followed by an adverb: f' (each) of a lambda or name
        if let Some(T::Adverb(a)) = self.peek().cloned() {
            self.next();
            let each = Expr::Apply(Box::new(Expr::Verb('\'', Some(a))), vec![n]);
            return Ok(Expr::Monad(Box::new(each), Box::new(self.expr()?)));
        }
        match self.peek().cloned() {
            Some(T::Verb(v)) => {
                self.next();
                let vb = self.verb(v);
                Ok(Expr::Dyad(Box::new(vb), Box::new(n), Box::new(self.expr()?)))
            }
            // juxtaposition: a function noun applied to what follows (f x)
            t if Self::starts_noun(t.as_ref()) => Ok(Expr::Monad(Box::new(n), Box::new(self.expr()?))),
            _ => Ok(n),
        }
    }
    fn noun(&mut self) -> Result<Expr, String> {
        let mut n = match self.next() {
            Some(T::Int(v)) => {
                let mut vs = vec![v as i32];
                while let Some(T::Int(w)) = self.peek().cloned() {
                    self.next();
                    vs.push(w as i32);
                }
                if vs.len() == 1 { Expr::Int(vs[0]) } else { Expr::Ints(vs) }
            }
            Some(T::Name(s)) => Expr::Name(s),
            Some(T::LP) => {
                let e = self.expr()?;
                if self.next() != Some(T::RP) {
                    return Err("expected )".into());
                }
                e
            }
            Some(T::LC) => {
                let body = self.stmts(Some(T::RC))?;
                if self.next() != Some(T::RC) {
                    return Err("expected }".into());
                }
                Expr::Lambda(Rc::new(Lambda { body }))
            }
            other => return Err(format!("expected a noun, got {other:?}")),
        };
        // brackets apply: f[a;b], x[i]
        while self.peek() == Some(&T::LB) {
            self.next();
            let mut args = Vec::new();
            loop {
                if self.peek() == Some(&T::RB) {
                    self.next();
                    break;
                }
                args.push(self.expr()?);
                match self.next() {
                    Some(T::Semi) => continue,
                    Some(T::RB) => break,
                    other => return Err(format!("expected ; or ], got {other:?}")),
                }
            }
            n = Expr::Apply(Box::new(n), args);
        }
        Ok(n)
    }
}

pub fn parse(src: &str) -> Result<Vec<Stmt>, String> {
    let mut p = P { t: lex(src)?, i: 0 };
    let s = p.stmts(None)?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens at {:?}", p.t.get(p.i)));
    }
    Ok(s)
}

// === the interpreter ===

pub struct K<'b> {
    pub b: &'b dyn Backend,
    pub vars: HashMap<String, Val>,
    /// kernels forced (for the bench: how much fusion saved)
    pub forced: usize,
}

impl<'b> K<'b> {
    pub fn new(b: &'b dyn Backend) -> K<'b> {
        K { b, vars: HashMap::new(), forced: 0 }
    }

    pub fn set(&mut self, name: &str, v: &[i32]) {
        let h = self.b.upload(v);
        self.vars.insert(name.into(), Val::Vec(Arr::Forced(h)));
    }

    pub fn force(&mut self, a: &Arr) -> Rc<dyn Handle> {
        match a {
            Arr::Forced(h) => h.clone(),
            Arr::Lazy { f, leaves, len } => {
                self.forced += 1;
                self.b.eval(f, leaves, *len)
            }
        }
    }

    /// A value read back to the host: an int or a vector.
    pub fn read(&mut self, v: &Val) -> Result<Vec<i32>, String> {
        match v {
            Val::Int(x) => Ok(vec![*x]),
            Val::Vec(a) => {
                let h = self.force(a);
                Ok(self.b.download(&h))
            }
            _ => Err("a function has no value to read".into()),
        }
    }

    pub fn run(&mut self, src: &str) -> Result<Val, String> {
        let prog = parse(src)?;
        let mut last = Val::Int(0);
        for s in &prog {
            last = self.stmt(s)?;
        }
        Ok(last)
    }

    fn stmt(&mut self, s: &Stmt) -> Result<Val, String> {
        match s {
            Stmt::Assign(n, e) => {
                let v = self.eval(e)?;
                // keep assigned vectors lazy: a later use fuses into its consumer
                self.vars.insert(n.clone(), v.clone());
                Ok(v)
            }
            Stmt::Expr(e) => self.eval(e),
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, String> {
        Ok(match e {
            Expr::Int(v) => Val::Int(*v),
            Expr::Ints(vs) => Val::Vec(Arr::Forced(self.b.upload(vs))),
            Expr::Name(n) => self.vars.get(n).cloned().ok_or(format!("undefined: {n}"))?,
            Expr::Lambda(l) => Val::Lambda(l.clone()),
            Expr::Verb(v, a) => Val::Verb(*v, a.clone()),
            Expr::Monad(f, x) => {
                let x = self.eval(x)?;
                let f = self.eval(f)?;
                self.apply(&f, vec![x])?
            }
            Expr::Dyad(f, x, y) => {
                let y = self.eval(y)?;
                let x = self.eval(x)?;
                let f = self.eval(f)?;
                self.apply(&f, vec![x, y])?
            }
            Expr::Apply(f, args) => {
                let mut vals = Vec::new();
                for a in args.iter().rev() {
                    vals.push(self.eval(a)?);
                }
                vals.reverse();
                let f = self.eval(f)?;
                self.apply(&f, vals)?
            }
        })
    }

    fn apply(&mut self, f: &Val, args: Vec<Val>) -> Result<Val, String> {
        match f {
            Val::Verb(v, None) => match args.as_slice() {
                [x] => self.monad(*v, x.clone()),
                [x, y] => self.dyad(*v, x.clone(), y.clone()),
                _ => Err(format!("{v}: one or two arguments")),
            },
            Val::Verb(v, Some(adv)) => self.adverb(*v, adv, args),
            Val::Lambda(l) => self.call(l, args),
            // indexing: x[i] or x@i
            Val::Vec(_) | Val::Int(_) => match args.as_slice() {
                [i] => self.dyad('@', f.clone(), i.clone()),
                _ => Err("index with one argument".into()),
            },
        }
    }

    fn call(&mut self, l: &Rc<Lambda>, args: Vec<Val>) -> Result<Val, String> {
        // a lambda over vectors is just its body evaluated on them (K's verbs are already vectorised)
        let saved: Vec<(String, Option<Val>)> = ["x", "y", "z"].iter().map(|n| (n.to_string(), self.vars.get(*n).cloned())).collect();
        for (n, v) in ["x", "y", "z"].iter().zip(args) {
            self.vars.insert(n.to_string(), v);
        }
        let mut last = Val::Int(0);
        for s in &l.body {
            last = self.stmt(s)?;
        }
        for (n, v) in saved {
            match v {
                Some(v) => self.vars.insert(n, v),
                None => self.vars.remove(&n),
            };
        }
        Ok(last)
    }

    /// Two values as one fused element-wise expression (an int broadcasts).
    fn zip(&self, op: impl Fn(F, F) -> F, x: &Val, y: &Val) -> Result<Val, String> {
        match (x, y) {
            (Val::Int(a), Val::Int(b)) => {
                let f = op(F::Lit(*a), F::Lit(*b));
                Ok(Val::Int(eval_f(&f, &[], 0)))
            }
            (Val::Vec(a), Val::Int(b)) => {
                let (fa, la, n) = a.fused();
                Ok(Val::Vec(Arr::Lazy { f: op(fa, F::Lit(*b)), leaves: la, len: n }))
            }
            (Val::Int(a), Val::Vec(b)) => {
                let (fb, lb, n) = b.fused();
                Ok(Val::Vec(Arr::Lazy { f: op(F::Lit(*a), fb), leaves: lb, len: n }))
            }
            (Val::Vec(a), Val::Vec(b)) => {
                if a.len() != b.len() {
                    return Err(format!("length: {} vs {}", a.len(), b.len()));
                }
                let (fa, mut la, n) = a.fused();
                let (fb, lb, _) = b.fused();
                let fb = shift(&fb, la.len());
                la.extend(lb);
                Ok(Val::Vec(Arr::Lazy { f: op(fa, fb), leaves: la, len: n }))
            }
            _ => Err("functions can't be added".into()),
        }
    }

    fn map1(&self, op: impl Fn(F) -> F, x: &Val) -> Result<Val, String> {
        match x {
            Val::Int(a) => Ok(Val::Int(eval_f(&op(F::Lit(*a)), &[], 0))),
            Val::Vec(a) => {
                let (f, l, n) = a.fused();
                Ok(Val::Vec(Arr::Lazy { f: op(f), leaves: l, len: n }))
            }
            _ => Err("not a number".into()),
        }
    }

    fn vecof(&mut self, x: &Val) -> Result<Rc<dyn Handle>, String> {
        match x {
            Val::Vec(a) => Ok(self.force(a)),
            Val::Int(v) => Ok(self.b.upload(&[*v])),
            _ => Err("not a vector".into()),
        }
    }

    fn int(x: &Val) -> Result<i32, String> {
        match x {
            Val::Int(v) => Ok(*v),
            _ => Err("expected an integer".into()),
        }
    }

    /// A gather of `src` (forced) at index expression `idx` (over the index vector's own leaves), length n.
    fn gather(&mut self, src: &Val, idx: F, mut leaves: Vec<Rc<dyn Handle>>, n: usize) -> Result<Val, String> {
        let s = self.vecof(src)?;
        let k = leaves.len();
        leaves.push(s);
        Ok(Val::Vec(Arr::Lazy { f: F::Gather(k, Box::new(idx)), leaves, len: n }))
    }

    fn monad(&mut self, v: char, x: Val) -> Result<Val, String> {
        match v {
            '-' => self.map1(|a| F::Un('-', Box::new(a)), &x),
            '~' => self.map1(|a| F::Un('~', Box::new(a)), &x),
            // !n: til
            '!' => {
                let n = Self::int(&x)?;
                Ok(Val::Vec(Arr::Lazy { f: F::Idx, leaves: vec![], len: n.max(0) as usize }))
            }
            // #x: count
            '#' => Ok(Val::Int(match &x {
                Val::Vec(a) => a.len() as i32,
                _ => 1,
            })),
            // |x: reverse (a gather, fused)
            '|' => match &x {
                Val::Vec(a) => {
                    let n = a.len() as i32;
                    self.gather(&x, F::Bin('-', Box::new(F::Lit(n - 1)), Box::new(F::Idx)), vec![], n as usize)
                }
                other => Ok(other.clone()),
            },
            // *x: first
            '*' => match &x {
                Val::Vec(_) => {
                    let g = self.gather(&x, F::Lit(0), vec![], 1)?;
                    Ok(Val::Int(self.read(&g)?[0]))
                }
                other => Ok(other.clone()),
            },
            // &x: where
            '&' => {
                let h = self.vecof(&x)?;
                Ok(Val::Vec(Arr::Forced(self.b.wher(&h))))
            }
            // <x >x: grade
            '<' | '>' => {
                let h = self.vecof(&x)?;
                Ok(Val::Vec(Arr::Forced(self.b.grade(&h, v == '>'))))
            }
            // _x: floor (integers are already whole); ,x: enlist
            '_' => Ok(x),
            ',' => {
                let h = self.vecof(&x)?;
                Ok(Val::Vec(Arr::Forced(h)))
            }
            other => Err(format!("no monad {other}")),
        }
    }

    fn dyad(&mut self, v: char, x: Val, y: Val) -> Result<Val, String> {
        match v {
            '+' | '-' | '*' | '%' | '!' | '&' | '|' | '<' | '>' | '=' | '^' => {
                // K's x!y is mod with the divisor on the left
                if v == '!' {
                    return self.zip(|a, b| F::Bin('!', Box::new(b), Box::new(a)), &x, &y);
                }
                self.zip(move |a, b| F::Bin(v, Box::new(a), Box::new(b)), &x, &y)
            }
            // x~y: match (whole values)
            '~' => {
                let (a, b) = (self.read(&x)?, self.read(&y)?);
                Ok(Val::Int((a == b) as i32))
            }
            // x,y: join
            ',' => {
                let (a, b) = (self.vecof(&x)?, self.vecof(&y)?);
                Ok(Val::Vec(Arr::Forced(self.b.join(&a, &b))))
            }
            // n#y: take (n past the end wraps, K's overtake); negative n takes from the end
            '#' => {
                let n = Self::int(&x)?;
                let len = match &y {
                    Val::Vec(a) => a.len() as i32,
                    _ => 1,
                };
                let idx = if n >= 0 {
                    F::Bin('!', Box::new(F::Idx), Box::new(F::Lit(len.max(1))))
                } else {
                    F::Bin('+', Box::new(F::Idx), Box::new(F::Lit(len + n)))
                };
                self.gather(&y, idx, vec![], n.unsigned_abs() as usize)
            }
            // n_y: drop
            '_' => {
                let n = Self::int(&x)?;
                let len = match &y {
                    Val::Vec(a) => a.len() as i32,
                    _ => 1,
                };
                let keep = (len - n.abs()).max(0);
                let idx = if n >= 0 { F::Bin('+', Box::new(F::Idx), Box::new(F::Lit(n))) } else { F::Idx };
                self.gather(&y, idx, vec![], keep as usize)
            }
            // x@i: index (i a vector gathers, fused with i's own expression)
            '@' => match &y {
                Val::Int(i) => {
                    let g = self.gather(&x, F::Lit(*i), vec![], 1)?;
                    Ok(Val::Int(self.read(&g)?[0]))
                }
                Val::Vec(a) => {
                    let (fi, li, n) = a.fused();
                    self.gather(&x, fi, li, n)
                }
                _ => Err("index with integers".into()),
            },
            other => Err(format!("no dyad {other}")),
        }
    }

    fn adverb(&mut self, v: char, adv: &str, args: Vec<Val>) -> Result<Val, String> {
        match (adv, args.as_slice()) {
            ("/", [x]) => match x {
                // the reduction reads the fused expression directly: x*y+3 is never written out
                Val::Vec(a) => {
                    let (f, l, n) = a.fused();
                    Ok(Val::Int(self.b.reduce(v, &f, &l, n)))
                }
                other => Ok(other.clone()),
            },
            ("\\", [x]) => {
                let h = self.vecof(x)?;
                Ok(Val::Vec(Arr::Forced(self.b.scan(v, &h))))
            }
            // v': each-prior: x[i] v x[i-1] (the first against v's identity, as K's -': gives x[0])
            ("':", [x]) => match x {
                Val::Vec(a) => {
                    let (f, mut l, n) = a.fused();
                    let h = self.force(a);
                    let k = l.len();
                    l.push(h);
                    let prior = F::Gather(k, Box::new(F::Bin('-', Box::new(F::Idx), Box::new(F::Lit(1)))));
                    Ok(Val::Vec(Arr::Lazy { f: F::Bin(v, Box::new(f), Box::new(prior)), leaves: l, len: n }))
                }
                other => Ok(other.clone()),
            },
            // f' x: each; a lambda's body is already element-wise, so each is the call itself
            ("'", [f]) if v == '\'' => Ok(f.clone()),
            _ => Err(format!("adverb {adv} on {v}")),
        }
    }
}
