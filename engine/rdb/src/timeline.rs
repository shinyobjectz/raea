//! The timeline (docs/substrate.feature, phase 2): every frame of an edit evaluated once, shared by everything that
//! reads frames (the render, scrubbing, the sketch, previz, brief checks, the sound's lanes, frame digests). The raw
//! frames (keys and rules) are computed once each, in parallel; transitions are read off that series instead of
//! recomputing neighbouring frames; each frame keeps only the columns that differ from the base. A frame is a pure
//! function of the edit, so any frame can be read in any order. A game's frames (`next:` rules) are computed in order,
//! each from the one before, and then read like any others.

use std::collections::HashMap;

use crate::eval::{self, Col, Cols, Tup};
use crate::ir::{Program, V};

pub struct Timeline {
    pub frames: i64,
    pub base: Cols,
    /// per frame: the columns whose values differ from the base (whole columns)
    written: Vec<HashMap<String, Col>>,
}

impl Timeline {
    pub fn new(prog: &Program, rows: &[Tup]) -> Result<Timeline, String> {
        Timeline::from_cols(prog, Cols::from_tups(rows))
    }

    pub fn from_cols(prog: &Program, base: Cols) -> Result<Timeline, String> {
        let frames = prog.frames.max(0);
        // transitions read raw frames up to their window past the last frame
        let ext = prog.transitions.iter().map(|t| t.frames.max(1)).max().unwrap_or(0);
        let total = (frames + ext) as usize;
        // the rules as K kernels (the interpreter where a rule doesn't compile; RAEA_INTERP=1 forces it everywhere)
        let k = crate::kernel::Compiled::new(prog);
        let interp = std::env::var_os("RAEA_INTERP").is_some();
        // a game (`next:` rules) folds frame by frame: frame N is what its next rules made of N-1
        let raws = if !prog.nexts.is_empty() {
            eval::folded(prog, (!interp).then_some(&k), &base, total)?
        } else {
            parallel(total, |t| if interp { eval::raw_cols(prog, &base, t as i64) } else { eval::raw_cols_k(prog, &k, &base, t as i64) })?
        };
        let written = parallel(frames as usize, |f| {
            let f = f as i64;
            let raw = &raws[f as usize];
            let mut cols: HashMap<String, Col> = HashMap::new();
            // the raw frame's columns that differ from the base
            for (c, v) in &raw.cols {
                if base.cols.get(c) != Some(v) {
                    cols.insert(c.clone(), v.clone());
                }
            }
            for tr in &prog.transitions {
                for i in 0..raw.len() {
                    if raw.kind[i] != tr.kind {
                        continue;
                    }
                    let at = |t: i64| raws[t.max(0) as usize].num(i, &tr.col);
                    if let Some(v) = eval::transitioned(tr, f, at) {
                        let col = cols.entry(tr.col.clone()).or_insert_with(|| raw.cols.get(&tr.col).cloned().unwrap_or_else(|| Col::Num { v: vec![0.0; raw.len()], has: vec![false; raw.len()] }));
                        col.set(i, V::N(v));
                    }
                }
            }
            Ok(cols)
        })?;
        Ok(Timeline { frames, base, written })
    }

    /// Whether row r's value of any of `cols` differs from its base on some frame (it moves over the edit).
    pub fn moves(&self, r: usize, cols: &[&str]) -> bool {
        self.written.iter().any(|w| cols.iter().any(|c| w.get(*c).is_some_and(|col| col.get(r) != self.base.get(r, c))))
    }

    /// One frame as columns.
    pub fn cols(&self, f: i64) -> Cols {
        let mut c = self.base.clone();
        if let Some(w) = self.written.get(f as usize) {
            for (k, v) in w {
                c.cols.insert(k.clone(), v.clone());
            }
        }
        c
    }

    /// One frame as rows.
    pub fn tups(&self, f: i64) -> Vec<Tup> {
        self.cols(f).to_tups()
    }

    /// One value at frame f (None: the row doesn't have it).
    pub fn get(&self, f: i64, r: usize, c: &str) -> Option<V> {
        match self.written.get(f as usize).and_then(|w| w.get(c)) {
            Some(col) => col.get(r),
            None => self.base.get(r, c),
        }
    }
}

/// f(0..n) on every core, in order.
fn parallel<T: Send>(n: usize, f: impl Fn(usize) -> Result<T, String> + Sync) -> Result<Vec<T>, String> {
    let cores = std::thread::available_parallelism().map(|c| c.get()).unwrap_or(4).min(n.max(1));
    let chunk = n.div_ceil(cores.max(1)).max(1);
    let parts: Vec<Result<Vec<T>, String>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..n).step_by(chunk).map(|a| {
            let f = &f;
            s.spawn(move || (a..(a + chunk).min(n)).map(f).collect::<Result<Vec<T>, String>>())
        }).collect();
        hs.into_iter().map(|h| h.join().expect("a timeline thread panicked")).collect()
    });
    let mut out = Vec::with_capacity(n);
    for p in parts {
        out.extend(p?);
    }
    Ok(out)
}
