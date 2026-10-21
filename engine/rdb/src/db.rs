//! The live edit: the tables (edit settings, one table per kind, keys, rules) and the moves that changed them.
//! A snippet is one or more SQL statements; each statement is checked, applied and recorded as a move with the rows
//! it touched before and after, and a snapshot of the state before, so undo is exact. Saved as an .rdb (render database: JSON lines, a header,
//! the state with its rows as columns, then the move log) and loaded back exactly; read with `query` (query.rs) on our
//! own columns. A .duckdb from before phase 4 is refused.
//!
//!   INSERT INTO <kind> (id, ...) VALUES ... | SELECT ... FROM range(n) t(i)    rows
//!   INSERT INTO keys VALUES (id, prop, frame, value, ease)                    time
//!   INSERT INTO rule (name, sql) VALUES ('name', 'UPDATE ... ')               a behaviour (runs every frame)
//!   [WITH ...] UPDATE <kind> SET ... WHERE ...                                a one-off change to rows
//!   DELETE FROM <kind> | keys | rule WHERE ...                                remove (a WHERE is required)
//!   INSERT INTO edit VALUES (fps, width, height, frames)                      settings

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value as J, json};
use sqlparser::ast::{SetExpr, Statement};
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;

use crate::assets::{self, Asset};
use crate::eval::{self, Tup};
use crate::store::{self, Store};
use crate::ir::{self, V};

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub id: String,
    pub prop: String,
    pub frame: i64,
    pub value: f64,
    pub ease: String,
}

#[derive(Debug, Clone)]
pub struct Move {
    pub n: usize,
    pub text: String,
    /// the rows, keys and rules this move touched, before and after (JSON)
    pub before: String,
    pub after: String,
    /// the whole state before the move (JSON), for undo
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub frames: i64,
    /// every row of every kind, in insertion order: entities in a Bevy World (store.rs)
    pub rows: Store,
    pub keys: Vec<Key>,
    /// (name, sql) in run order
    pub rules: Vec<(String, String)>,
    /// content-addressed assets the rows refer to (`asset:<hash>`), with how each came to be
    pub assets: Vec<Asset>,
    /// smooth changes between states (kind.col eased over N frames)
    pub transitions: Vec<ir::Transition>,
}

impl Default for State {
    fn default() -> Self {
        State { fps: 30.0, width: 1080, height: 1920, frames: 90, rows: Store::default(), keys: vec![], rules: vec![], assets: vec![], transitions: vec![] }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Db {
    pub state: State,
    pub moves: Vec<Move>,
    /// moves that were undone: the "reverted" labels preference memory learns from (kept, never replayed)
    pub undone: Vec<Move>,
    /// where find()/make()/remake() keep bytes (next to the edit); None: they're refused
    pub store: Option<assets::Store>,
}

/// The .rdb (render database) format's version: the header line's "rdb".
const FORMAT: u64 = 1;

const META: &[&str] = &["edit", "keys", "rule", "move", "asset", "undone", "transition"];

fn vjson(v: &V) -> J {
    match v {
        V::N(x) => json!(x),
        V::S(s) => json!(s),
    }
}

fn jv(j: &J) -> V {
    match j {
        J::Number(n) => V::N(n.as_f64().unwrap_or(0.0)),
        J::String(s) => V::S(s.clone()),
        J::Bool(b) => V::N(*b as u8 as f64),
        _ => V::S(String::new()),
    }
}

/// A number as JSON, exactly (non-finite ones as {"f": "NaN"}, since JSON has no NaN).
fn num_json(x: f64) -> J {
    if x.is_finite() { json!(x) } else { json!({"f": x.to_string()}) }
}

fn json_v(j: &J) -> Option<V> {
    match j {
        J::Null => None,
        J::Object(o) => o.get("f").and_then(J::as_str).and_then(|f| f.parse().ok()).map(V::N),
        other => Some(jv(other)),
    }
}

/// The rows as columns: kind and id per row, then each column with a value or null per row.
fn rows_json(rows: &[Tup]) -> J {
    let mut cols: Vec<&str> = vec![];
    for t in rows {
        for c in t.vals.keys() {
            if !cols.contains(&c.as_str()) {
                cols.push(c);
            }
        }
    }
    let col = |c: &str| -> J {
        J::Array(rows.iter().map(|t| match t.vals.get(c) {
            None => J::Null,
            Some(V::N(x)) => num_json(*x),
            Some(V::S(x)) => json!(x),
        }).collect())
    };
    json!({
        "kind": rows.iter().map(|t| t.kind.as_str()).collect::<Vec<_>>(),
        "id": rows.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        "cols": cols.iter().map(|c| (c.to_string(), col(c))).collect::<serde_json::Map<_, _>>(),
    })
}

/// Rows from rows_json, or from the row-per-object form older saves used.
fn json_rows(j: &J) -> Vec<Tup> {
    if let Some(a) = j.as_array() {
        return a
            .iter()
            .map(|r| Tup {
                kind: r["kind"].as_str().unwrap_or_default().to_string(),
                id: r["id"].as_str().unwrap_or_default().to_string(),
                vals: r.as_object().map(|o| o.iter().filter(|(k, _)| *k != "kind" && *k != "id").map(|(k, v)| (k.clone(), jv(v))).collect()).unwrap_or_default(),
            })
            .collect();
    }
    let strs = |k: &str| j[k].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap_or_default().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    let (kinds, ids) = (strs("kind"), strs("id"));
    let mut rows: Vec<Tup> = kinds.into_iter().zip(ids).map(|(kind, id)| Tup { kind, id, vals: BTreeMap::new() }).collect();
    if let Some(cols) = j["cols"].as_object() {
        for (c, vs) in cols {
            for (r, v) in vs.as_array().into_iter().flatten().enumerate() {
                if let (Some(row), Some(v)) = (rows.get_mut(r), json_v(v)) {
                    row.vals.insert(c.clone(), v);
                }
            }
        }
    }
    rows
}

fn row_json(t: &Tup) -> J {
    let mut m = serde_json::Map::new();
    m.insert("kind".into(), json!(t.kind));
    m.insert("id".into(), json!(t.id));
    for (k, v) in &t.vals {
        m.insert(k.clone(), vjson(v));
    }
    J::Object(m)
}

fn key_json(k: &Key) -> J {
    json!({"id": k.id, "prop": k.prop, "frame": k.frame, "value": k.value, "ease": k.ease})
}

impl State {
    pub fn to_json(&self) -> J {
        json!({
            "edit": {"fps": num_json(self.fps), "width": self.width, "height": self.height, "frames": self.frames},
            "rows": rows_json(&self.rows.tups()),
            "keys": self.keys.iter().map(|k| json!({"id": k.id, "prop": k.prop, "frame": k.frame, "value": num_json(k.value), "ease": k.ease})).collect::<Vec<_>>(),
            "rules": self.rules.iter().map(|(n, s)| json!({"name": n, "sql": s})).collect::<Vec<_>>(),
            "assets": self.assets.iter().map(|a| json!({"hash": a.hash, "ext": a.ext, "origin": a.origin})).collect::<Vec<_>>(),
            "transitions": self.transitions.iter().map(|t| json!({"kind": t.kind, "col": t.col, "frames": t.frames, "ease": t.ease, "align": t.align})).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(j: &J) -> State {
        let e = &j["edit"];
        State {
            fps: json_v(&e["fps"]).map(|v| v.num()).unwrap_or(30.0),
            width: e["width"].as_u64().unwrap_or(1080) as u32,
            height: e["height"].as_u64().unwrap_or(1920) as u32,
            frames: e["frames"].as_i64().unwrap_or(90),
            rows: Store::new(&json_rows(&j["rows"])),
            keys: j["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|k| Key {
                    id: k["id"].as_str().unwrap_or_default().into(),
                    prop: k["prop"].as_str().unwrap_or_default().into(),
                    frame: k["frame"].as_i64().unwrap_or(0),
                    value: json_v(&k["value"]).map(|v| v.num()).unwrap_or(0.0),
                    ease: k["ease"].as_str().unwrap_or("linear").into(),
                })
                .collect(),
            rules: j["rules"].as_array().into_iter().flatten().map(|r| (r["name"].as_str().unwrap_or_default().into(), r["sql"].as_str().unwrap_or_default().into())).collect(),
            assets: j["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| Asset { hash: a["hash"].as_str().unwrap_or_default().into(), ext: a["ext"].as_str().unwrap_or("png").into(), origin: a["origin"].clone() })
                .collect(),
            transitions: j["transitions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| ir::Transition {
                    kind: t["kind"].as_str().unwrap_or_default().into(),
                    col: t["col"].as_str().unwrap_or_default().into(),
                    frames: t["frames"].as_i64().unwrap_or(12),
                    ease: t["ease"].as_str().unwrap_or("inout").into(),
                    align: t["align"].as_str().unwrap_or("after").into(),
                })
                .collect(),
        }
    }

    /// The runtime's view: rows as inserted, keys, and the rules compiled in order.
    pub fn program(&self) -> Result<(ir::Program, Vec<Tup>), String> {
        let rules: String = self.rules.iter().map(|(_, s)| format!("{};\n", s.trim().trim_end_matches(';'))).collect();
        let mut p = ir::compile(&rules)?;
        p.fps = self.fps;
        p.width = self.width;
        p.height = self.height;
        p.frames = self.frames;
        p.transitions = self.transitions.clone();
        for k in &self.keys {
            let e = p.keys.entry((k.id.clone(), k.prop.clone())).or_default();
            e.push((k.frame, k.value, k.ease.clone()));
            e.sort_by_key(|x| x.0);
        }
        Ok((p, self.rows.tups()))
    }
}

/// What a move touched: rows by (kind, id), keys by (id, prop, frame), rules by name, settings.
fn touched(a: &State, b: &State) -> (J, J) {
    let rmap = |s: &State| s.rows.tups().into_iter().map(|t| ((t.kind.clone(), t.id.clone()), t)).collect::<BTreeMap<_, _>>();
    let kmap = |s: &State| s.keys.iter().map(|k| ((k.id.clone(), k.prop.clone(), k.frame), k.clone())).collect::<BTreeMap<_, _>>();
    let umap = |s: &State| s.rules.iter().cloned().collect::<BTreeMap<_, _>>();
    let (ra, rb, ka, kb, ua, ub) = (rmap(a), rmap(b), kmap(a), kmap(b), umap(a), umap(b));
    let (mut before, mut after) = (vec![], vec![]);
    for k in ra.keys().chain(rb.keys()).collect::<std::collections::BTreeSet<_>>() {
        if ra.get(k) != rb.get(k) {
            if let Some(t) = ra.get(k) {
                before.push(row_json(t));
            }
            if let Some(t) = rb.get(k) {
                after.push(row_json(t));
            }
        }
    }
    for k in ka.keys().chain(kb.keys()).collect::<std::collections::BTreeSet<_>>() {
        if ka.get(k) != kb.get(k) {
            if let Some(x) = ka.get(k) {
                before.push(json!({"keys": key_json(x)}));
            }
            if let Some(x) = kb.get(k) {
                after.push(json!({"keys": key_json(x)}));
            }
        }
    }
    for k in ua.keys().chain(ub.keys()).collect::<std::collections::BTreeSet<_>>() {
        if ua.get(k) != ub.get(k) {
            if let Some(x) = ua.get(k) {
                before.push(json!({"rule": k, "sql": x}));
            }
            if let Some(x) = ub.get(k) {
                after.push(json!({"rule": k, "sql": x}));
            }
        }
    }
    if (a.fps, a.width, a.height, a.frames) != (b.fps, b.width, b.height, b.frames) {
        before.push(json!({"edit": [a.fps, a.width, a.height, a.frames]}));
        after.push(json!({"edit": [b.fps, b.width, b.height, b.frames]}));
    }
    (J::Array(before), J::Array(after))
}

/// The edit's settings, in range (a swapped fps and frames rendered 30 frames at 917 fps in an agent's first try).
fn settings_ok(fps: f64, width: u32, height: u32, frames: i64) -> Result<(), String> {
    let order = "edit is (fps, width, height, frames), e.g. INSERT INTO edit VALUES (30, 1080, 1920, 900) for 30 s at 30 fps";
    if !(1.0..=240.0).contains(&fps) {
        return Err(format!("fps {fps} isn't a frame rate (1 to 240): {order}"));
    }
    if !(16..=8192).contains(&width) || !(16..=8192).contains(&height) {
        return Err(format!("{width}x{height} isn't a frame size (16 to 8192 pixels a side): {order}"));
    }
    if !(1..=fps as i64 * 3600).contains(&frames) {
        return Err(format!("{frames} frames isn't a length (1 frame to an hour): {order}"));
    }
    Ok(())
}

fn refuse<T>(st: &impl std::fmt::Display, why: &str) -> Result<T, String> {
    Err(format!("{why}: {}", st.to_string().chars().take(200).collect::<String>()))
}

fn table_of(st: &Statement) -> Option<String> {
    match st {
        Statement::Insert(i) => Some(i.table.to_string().to_lowercase()),
        Statement::Delete(d) => {
            let from = match &d.from {
                sqlparser::ast::FromTable::WithFromKeyword(t) | sqlparser::ast::FromTable::WithoutKeyword(t) => t,
            };
            from.first().map(|t| t.relation.to_string().to_lowercase())
        }
        _ => None,
    }
}

/// "unknown column x" where the snippet wrote "x": in SQL double quotes name a column, text takes single quotes.
fn quote_hint(e: String, snippet: &str) -> String {
    match e.find("unknown column ").map(|i| e[i + 15..].split([':', ' ', ')']).next().unwrap_or("").to_string()) {
        Some(c) if !c.is_empty() && !matches!(c.as_str(), "from" | "to") && snippet.to_lowercase().contains(&format!("\"{c}")) => {
            format!("{e} (\"{c}\" in double quotes names a column; text takes single quotes: '{c}')")
        }
        _ => e,
    }
}

impl Db {
    /// Apply a snippet: every statement or none (a refused statement leaves the edit as it was). Returns the moves.
    /// `CAPTIONS FROM 'takes/<take>/words.json' [PER n] [STYLE 'color=#fff,size=90'] [HIGHLIGHT '#22d3ee'];` in a snippet becomes the
    /// INSERT of the take's words over the clips made so far (captions.rs), so a whole edit stays one file.
    pub fn apply(&mut self, snippet: &str) -> Result<Vec<usize>, String> {
        // a rule's sql in double quotes ("UPDATE clip ... WHERE id = 'c1'", written to dodge the inner quotes) is a
        // string here, not a column: agents did it five times in one run and read "unknown column update clip ..."
        let dq = regex::Regex::new(r#"(?is)(INSERT\s+INTO\s+rule\b[^;]*?VALUES\s*\(\s*'[^']*'\s*,\s*)"(\s*UPDATE\b[^"]*)""#).unwrap();
        let snippet = &dq.replace_all(snippet, |c: &regex::Captures| format!("{}'{}'", &c[1], c[2].replace('\'', "''"))).into_owned();
        let re = regex::Regex::new(r"(?is)\bCAPTIONS\s+FROM\s+'([^']+)'(?:\s+PER\s+(\d+))?(?:\s+STYLE\s+'([^']*)')?(?:\s+HIGHLIGHT\s+'([^']*)')?\s*(?:;|$)").unwrap();
        if !re.is_match(snippet) {
            return self.apply_sql(snippet).map_err(|e| quote_hint(e, snippet));
        }
        let saved = (self.state.clone(), self.moves.len());
        let mut made = Vec::new();
        let mut at = 0;
        let run = |db: &mut Db, sql: &str| -> Result<Vec<usize>, String> {
            if !sql.trim().trim_matches(';').trim().is_empty() && !sql.lines().all(|l| l.trim().is_empty() || l.trim().starts_with("--")) {
                return db.apply_sql(sql);
            }
            Ok(vec![])
        };
        let res = (|| {
            for c in re.captures_iter(snippet) {
                let m = c.get(0).unwrap();
                made.extend(run(self, &snippet[at..m.start()])?);
                at = m.end();
                let words = Path::new(&c[1]).to_path_buf();
                let dir = self.store.as_ref().and_then(|s| s.dir.parent().map(Path::to_path_buf)).unwrap_or_default();
                // the project's path (the cwd), else one under the edit's folders
                let words = if words.exists() { words } else { dir.ancestors().map(|a| a.join(&words)).find(|p| p.exists()).ok_or(format!("CAPTIONS FROM '{}': no such file", &c[1]))? };
                let (prog, rows) = self.state.program()?;
                let per = c.get(2).and_then(|p| p.as_str().parse().ok()).unwrap_or(1usize).max(1);
                let (sql, _) = crate::captions::caption_sql(&prog, &rows, &dir, &words, c.get(3).map(|s| s.as_str()).unwrap_or(""), per, c.get(4).map(|s| s.as_str()))?;
                // one move, so one undo takes the captions and their word runs back together
                let ns = self.apply_sql(&sql)?;
                if let (Some(&first), Some(&last)) = (ns.first(), ns.last()) {
                    let parts: Vec<Move> = self.moves.drain(first - 1..last).collect();
                    let after: Vec<J> = parts.iter().flat_map(|m| serde_json::from_str::<Vec<J>>(&m.after).unwrap_or_default()).collect();
                    self.moves.push(Move { n: first, text: m.as_str().trim().trim_end_matches(';').to_string(), before: parts[0].before.clone(), after: J::Array(after).to_string(), snapshot: parts[0].snapshot.clone() });
                    made.push(first);
                }
            }
            made.extend(run(self, &snippet[at..])?);
            Ok::<(), String>(())
        })();
        if let Err(e) = res {
            self.state = saved.0;
            self.moves.truncate(saved.1);
            return Err(e);
        }
        Ok(made)
    }

    fn apply_sql(&mut self, snippet: &str) -> Result<Vec<usize>, String> {
        let stmts = Parser::parse_sql(&DuckDbDialect {}, snippet).map_err(|e| {
            let e = e.to_string();
            // the commonest slip: from and to are SQL keywords, so as column names they need quotes
            if e.contains("found: from") || e.contains("found: to") || e.contains("found: FROM") { format!("{e} (quote the columns \"from\" and \"to\")") } else { e }
        })?;
        if stmts.is_empty() {
            return Err("an empty snippet".into());
        }
        let saved = (self.state.clone(), self.moves.len());
        let mut made = Vec::new();
        for st in &stmts {
            let before = self.state.clone();
            let mut st = st.clone();
            if let Err(e) = self.resolve_assets(&mut st).and_then(|_| self.apply_one(&st)) {
                self.state = saved.0;
                self.moves.truncate(saved.1);
                return Err(e);
            }
            // a row on screen from `from` to `to` needs `to` after `from` (an agent wrote a caption 30..0, and the edit
            // then failed to render): refused here, with the row
            let bad = self.state.rows.tups().into_iter().find(|t| match (t.vals.get("from"), t.vals.get("to")) {
                (Some(V::N(a)), Some(V::N(b))) => b <= a,
                _ => false,
            });
            if let Some(t) = bad {
                self.state = saved.0;
                self.moves.truncate(saved.1);
                let (a, b) = (t.vals.get("from").map(|v| v.num()).unwrap_or(0.0), t.vals.get("to").map(|v| v.num()).unwrap_or(0.0));
                return Err(format!("{} {}: \"to\" ({b}) must be after \"from\" ({a}): frames [from, to) are when it's on screen", t.kind, t.id));
            }
            // every rule must still compile, in order (the determinism lint runs over the whole list)
            if let Err(e) = self.state.program() {
                self.state = saved.0;
                self.moves.truncate(saved.1);
                return Err(format!("the rules no longer compile: {e}"));
            }
            let (b, a) = touched(&before, &self.state);
            let n = self.moves.len() + 1;
            self.moves.push(Move { n, text: st.to_string(), before: b.to_string(), after: a.to_string(), snapshot: before.to_json().to_string() });
            made.push(n);
        }
        Ok(made)
    }

    fn keep(&mut self, a: Asset) -> String {
        if !self.state.assets.iter().any(|x| x.hash == a.hash) {
            self.state.assets.push(a.clone());
        }
        format!("asset:{}", a.hash)
    }

    /// find('phrase') and make('prompt'[, seed]) become the `asset:<hash>` they resolve to, before the statement runs.
    fn resolve_assets(&mut self, st: &mut Statement) -> Result<(), String> {
        use sqlparser::ast::{Expr, FunctionArg, FunctionArgExpr, FunctionArguments, Value};
        use std::ops::ControlFlow;
        let mut calls: Vec<(String, Vec<String>)> = Vec::new();
        let _ = sqlparser::ast::visit_expressions(st, |e: &Expr| {
            if let Expr::Function(f) = e {
                let name = f.name.to_string().to_lowercase();
                if name == "find" || name == "make" {
                    let args = match &f.args {
                        FunctionArguments::List(l) => l
                            .args
                            .iter()
                            .filter_map(|a| match a {
                                FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(v))) => Some(match &v.value {
                                    Value::SingleQuotedString(s) | Value::DollarQuotedString(sqlparser::ast::DollarQuotedString { value: s, .. }) => s.clone(),
                                    Value::Number(n, _) => n.clone(),
                                    other => other.to_string(),
                                }),
                                _ => None,
                            })
                            .collect(),
                        _ => vec![],
                    };
                    calls.push((name, args));
                }
            }
            ControlFlow::<()>::Continue(())
        });
        if calls.is_empty() {
            return Ok(());
        }
        let store = self.store.clone().ok_or("find()/make() need an asset store (an edit saved on disk)")?;
        let mut hashes = Vec::new();
        for (name, args) in &calls {
            let q = args.first().ok_or(format!("{name}() takes a phrase: {name}('hands holding the serum')"))?;
            let a = if name == "find" { store.find(q, 0)? } else { store.make("stand-in", q, args.get(1).and_then(|x| x.parse().ok()).unwrap_or(1), &json!({}))? };
            hashes.push(self.keep(a));
        }
        let mut it = hashes.into_iter();
        let _ = sqlparser::ast::visit_expressions_mut(st, |e: &mut Expr| {
            if let Expr::Function(f) = e {
                let name = f.name.to_string().to_lowercase();
                if name == "find" || name == "make" {
                    if let Some(h) = it.next() {
                        *e = Expr::Value(Value::SingleQuotedString(h).with_empty_span());
                    }
                }
            }
            ControlFlow::<()>::Continue(())
        });
        Ok(())
    }

    fn apply_one(&mut self, st: &Statement) -> Result<(), String> {
        // UPDATE ... SET col = remake(col): the explicit "do it again" for an asset, row by row
        if let Statement::Update(up) = st {
            let remakes: Vec<(String, String)> = up
                .assignments
                .iter()
                .filter_map(|a| match &a.value {
                    sqlparser::ast::Expr::Function(f) if f.name.to_string().to_lowercase() == "remake" => {
                        let arg = f.args.to_string();
                        Some((a.target.to_string().to_lowercase(), arg.trim_matches(|c| c == '(' || c == ')').trim().to_lowercase()))
                    }
                    _ => None,
                })
                .collect();
            if !remakes.is_empty() {
                let mut plain = up.clone();
                for a in &mut plain.assignments {
                    if let sqlparser::ast::Expr::Function(f) = &a.value {
                        if f.name.to_string().to_lowercase() == "remake" {
                            a.value = sqlparser::ast::Expr::Identifier(sqlparser::ast::Ident::new(a.target.to_string()));
                        }
                    }
                }
                let p = ir::compile(&Statement::Update(plain).to_string())?;
                let sys = &p.systems[0];
                let rows = eval::select_cols(&self.state.rows.gather(), &sys.kind, sys.filter.as_ref(), 0)?;
                let ents = store::rows(&self.state.rows.world).to_vec();
                if rows.is_empty() {
                    return refuse(st, "the WHERE matched no rows, so nothing changed");
                }
                let store = self.store.clone().ok_or("remake() needs an asset store (an edit saved on disk)")?;
                for k in rows {
                    for (target, from) in &remakes {
                        let Some(V::S(old)) = store::get(&self.state.rows.world, ents[k], from) else {
                            return refuse(st, &format!("{} has no asset in {from}", store::key(&self.state.rows.world, ents[k]).map(|k| k.id.clone()).unwrap_or_default()));
                        };
                        let hash = old.trim_start_matches("asset:");
                        let a = self.state.assets.iter().find(|a| a.hash == hash).cloned().ok_or(format!("unknown asset {old}"))?;
                        let new = store.remake(&a)?;
                        let v = self.keep(new);
                        store::set(&mut self.state.rows.world, ents[k], target, V::S(v));
                    }
                }
                return Ok(());
            }
        }
        let s = &mut self.state;
        match st {
            Statement::Insert(_) => match table_of(st).as_deref() {
                Some("rule") => {
                    // (name, sql): the sql must be one UPDATE (a behaviour)
                    let p = ir::compile(&st.to_string()).map_err(|e| format!("rule: {e}"))?;
                    for (_, rows) in &p.spawns {
                        for r in rows {
                            let (Some(V::S(name)), Some(V::S(sql))) = (r.get("name"), r.get("sql")) else {
                                return refuse(st, "a rule is (name, sql): INSERT INTO rule (name, sql) VALUES ('name', 'UPDATE ...')");
                            };
                            // several UPDATEs in one rule become rules name, name.2, ... in order (an agent packed a
                            // punch-in's three zoom steps into one)
                            let parts = Parser::parse_sql(&DuckDbDialect {}, sql).map_err(|e| format!("rule {name}: {e}"))?;
                            if parts.is_empty() {
                                return refuse(st, &format!("rule {name} is empty: its sql is one or more UPDATEs"));
                            }
                            for (k, part) in parts.iter().enumerate() {
                                let text = part.to_string();
                                let one = ir::compile(&text).map_err(|e| format!("rule {name}: {e}"))?;
                                if one.systems.len() != 1 || !one.spawns.is_empty() || !one.keys.is_empty() {
                                    return refuse(st, &format!("rule {name}: each statement of a rule is an UPDATE (this one isn't: {text})"));
                                }
                                let named = if k == 0 { name.clone() } else { format!("{name}.{}", k + 1) };
                                if s.rules.iter().any(|(n, _)| *n == named) {
                                    return refuse(st, &format!("there is already a rule named {named}; DELETE FROM rule WHERE name = '{named}' first"));
                                }
                                s.rules.push((named, if parts.len() == 1 { sql.trim().trim_end_matches(';').to_string() } else { text }));
                            }
                        }
                    }
                    Ok(())
                }
                Some("transition") => {
                    let p = ir::compile(&st.to_string())?;
                    for t in p.transitions {
                        s.transitions.retain(|x| !(x.kind == t.kind && x.col == t.col));
                        s.transitions.push(t);
                    }
                    Ok(())
                }
                Some("edit") | Some("keys") => {
                    let p = ir::compile(&st.to_string())?;
                    if table_of(st).as_deref() == Some("edit") {
                        settings_ok(p.fps, p.width, p.height, p.frames).or_else(|e| refuse(st, &e))?;
                        (s.fps, s.width, s.height, s.frames) = (p.fps, p.width, p.height, p.frames);
                    } else {
                        for ((id, prop), pts) in p.keys {
                            for (frame, value, ease) in pts {
                                s.keys.retain(|k| !(k.id == id && k.prop == prop && k.frame == frame));
                                s.keys.push(Key { id: id.clone(), prop: prop.clone(), frame, value, ease });
                            }
                        }
                    }
                    Ok(())
                }
                Some(kind) if META.contains(&kind) => refuse(st, &format!("{kind} is written by raea")),
                Some(_) => {
                    let p = ir::compile(&st.to_string())?;
                    // seq is raea's own column (the row order on disk)
                    if p.spawns.iter().any(|(_, rows)| rows.iter().any(|r| r.contains_key("seq"))) {
                        return refuse(st, "seq is raea's column (it keeps row order); leave it out");
                    }
                    for (kind, rows) in p.spawns {
                        for r in rows {
                            let Some(V::S(id)) = r.get("id").cloned() else { return refuse(st, "every row needs an id (a string)") };
                            if store::find(&s.rows.world, &kind, &id).is_some() {
                                return refuse(st, &format!("{kind} already has a row with id {id:?}"));
                            }
                            store::spawn(&mut s.rows.world, &Tup { kind: kind.clone(), id, vals: r.into_iter().filter(|(k, _)| k != "id").collect() });
                        }
                    }
                    Ok(())
                }
                None => refuse(st, "INSERT INTO what?"),
            },
            Statement::Update(_) | Statement::Query(_) => {
                if let Statement::Query(q) = st {
                    if !matches!(q.body.as_ref(), SetExpr::Update(_)) {
                        return refuse(st, "a SELECT reads the edit: use query, not apply");
                    }
                }
                // a one-off change to rows (at frame 0); a behaviour over time is a rule
                let p = ir::compile(&st.to_string())?;
                let sys = &p.systems[0];
                if sys.sets.iter().any(|(c, _)| c == "seq" || c == "id") {
                    return refuse(st, "id and seq can't be changed (delete the row and insert it again)");
                }
                // the edit's settings: one row, changed in place
                if sys.kind == "edit" {
                    let cur = |c: &str| -> Option<V> {
                        Some(V::N(match c {
                            "fps" => s.fps,
                            "width" => s.width as f64,
                            "height" => s.height as f64,
                            "frames" => s.frames as f64,
                            _ => return None,
                        }))
                    };
                    let mut next = (s.fps, s.width, s.height, s.frames);
                    for (c, e) in &sys.sets {
                        let v = ir::eval_row(e, &|_, k| cur(k))?.num();
                        match c.as_str() {
                            "fps" => next.0 = v,
                            "width" => next.1 = v as u32,
                            "height" => next.2 = v as u32,
                            "frames" => next.3 = v as i64,
                            other => return refuse(st, &format!("edit has fps, width, height and frames, not {other}")),
                        }
                    }
                    settings_ok(next.0, next.1, next.2, next.3).or_else(|e| refuse(st, &e))?;
                    (s.fps, s.width, s.height, s.frames) = next;
                    return Ok(());
                }
                if META.contains(&sys.kind.as_str()) {
                    return refuse(st, &format!("change {} rows with DELETE FROM {} WHERE ... and INSERT", sys.kind, sys.kind));
                }
                // the base as columns, the statement over it, the rows it wrote back into the World
                let mut cols = s.rows.gather();
                let wrote = eval::run_system_cols(sys, &mut cols, 0)?;
                if wrote.is_empty() {
                    return refuse(st, "the WHERE matched no rows, so nothing changed");
                }
                store::write_back(&mut s.rows.world, &cols, &wrote);
                Ok(())
            }
            Statement::Delete(d) => {
                let table = table_of(st).ok_or("DELETE FROM what?")?;
                // no WHERE empties the table (undo takes it back); emptying an empty table is fine, so a snippet that
                // rebuilds an edit from scratch runs on a fresh edit too
                let sel = d.selection.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "true".into());
                let everything = sel.eq_ignore_ascii_case("true");
                let probe = ir::compile(&format!("UPDATE {table} SET _x = 1 WHERE {sel};"))?;
                let filter = probe.systems[0].filter.clone();
                // the table as rows, so the WHERE runs the same way for kinds, keys and rules
                let rows: Vec<Tup> = match table.as_str() {
                    "rule" => s.rules.iter().map(|(n, q)| Tup { kind: "rule".into(), id: n.clone(), vals: [("name".into(), V::S(n.clone())), ("sql".into(), V::S(q.clone()))].into() }).collect(),
                    "keys" => s
                        .keys
                        .iter()
                        .map(|k| Tup {
                            kind: "keys".into(),
                            id: k.id.clone(),
                            vals: [("prop".into(), V::S(k.prop.clone())), ("frame".into(), V::N(k.frame as f64)), ("value".into(), V::N(k.value)), ("ease".into(), V::S(k.ease.clone()))].into(),
                        })
                        .collect(),
                    "transition" => s
                        .transitions
                        .iter()
                        .map(|t| Tup {
                            kind: "transition".into(),
                            id: format!("{}.{}", t.kind, t.col),
                            vals: [("kind".into(), V::S(t.kind.clone())), ("col".into(), V::S(t.col.clone())), ("frames".into(), V::N(t.frames as f64)), ("ease".into(), V::S(t.ease.clone())), ("align".into(), V::S(t.align.clone()))].into(),
                        })
                        .collect(),
                    "edit" | "move" | "asset" | "undone" => return refuse(st, &format!("{table} can't be deleted from")),
                    _ => s.rows.tups(),
                };
                let gone = eval::select(&rows, &table, filter.as_ref(), 0)?;
                if gone.is_empty() {
                    if everything {
                        return Ok(());
                    }
                    return refuse(st, "the WHERE matched nothing");
                }
                match table.as_str() {
                    "rule" => {
                        let names: Vec<String> = gone.iter().map(|&i| rows[i].id.clone()).collect();
                        s.rules.retain(|(n, _)| !names.contains(n));
                    }
                    "transition" => {
                        let ids: Vec<String> = gone.iter().map(|&i| rows[i].id.clone()).collect();
                        s.transitions.retain(|t| !ids.contains(&format!("{}.{}", t.kind, t.col)));
                    }
                    "keys" => {
                        let mut k = 0;
                        let gone: std::collections::BTreeSet<usize> = gone.into_iter().collect();
                        s.keys.retain(|_| {
                            k += 1;
                            !gone.contains(&(k - 1))
                        });
                    }
                    _ => {
                        let ents = store::rows(&s.rows.world).to_vec();
                        for k in gone {
                            store::despawn(&mut s.rows.world, ents[k]);
                        }
                    }
                }
                Ok(())
            }
            _ => refuse(st, "not a move (INSERT, UPDATE, WITH ... UPDATE, DELETE)"),
        }
    }

    /// Undo the last move: the state goes back to the snapshot taken before it.
    pub fn undo(&mut self) -> Result<Move, String> {
        let m = self.moves.pop().ok_or("nothing to undo")?;
        let j: J = serde_json::from_str(&m.snapshot).map_err(|e| e.to_string())?;
        self.state = State::from_json(&j);
        self.undone.push(m.clone());
        Ok(m)
    }

    // === saving and loading ===

    /// The edit as JSON lines: a header, the state (rows as columns), then one line per move and per undone move.
    /// Written beside the file and renamed over it, so a save is all or nothing.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mv = |m: &Move, snap: bool| {
            let mut o = json!({"n": m.n, "text": m.text, "before": m.before, "after": m.after});
            if snap {
                o["snapshot"] = json!(m.snapshot);
            }
            o
        };
        let mut out = String::new();
        let mut line = |j: J| {
            out.push_str(&j.to_string());
            out.push('\n');
        };
        line(json!({"rdb": FORMAT}));
        line(json!({"state": self.state.to_json()}));
        for m in &self.moves {
            line(json!({"move": mv(m, true)}));
        }
        for m in &self.undone {
            line(json!({"undone": mv(m, false)}));
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, out).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    /// A read-only query on the saved edit, answered from its columns (query.rs), as JSON text.
    pub fn query(path: &Path, sql: &str) -> Result<String, String> {
        let db = Self::load(path)?;
        let (cols, rows) = crate::query::run(&db, sql)?;
        Ok(crate::query::to_json(&cols, &rows))
    }

    pub fn load(path: &Path) -> Result<Db, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // a DuckDB file (its magic at byte 8): an edit saved before phase 4, no longer read
        if bytes.get(8..12) == Some(b"DUCK") {
            return Err(format!("{}: a DuckDB edit from before phase 4; rdb no longer reads these", path.display()));
        }
        let text = String::from_utf8(bytes).map_err(|_| format!("{}: not an .rdb edit", path.display()))?;
        let mut lines = text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str::<J>(l).map_err(|e| format!("{}: {e}", path.display())));
        let head = lines.next().ok_or(format!("{}: empty", path.display()))??;
        if head["rdb"].as_u64() != Some(FORMAT) {
            return Err(format!("{}: not an .rdb edit (format {FORMAT})", path.display()));
        }
        let mut db = Db { store: Some(assets::Store::beside(path)), ..Default::default() };
        let mv = |m: &J| Move {
            n: m["n"].as_u64().unwrap_or(0) as usize,
            text: m["text"].as_str().unwrap_or_default().into(),
            before: m["before"].as_str().unwrap_or_default().into(),
            after: m["after"].as_str().unwrap_or_default().into(),
            snapshot: m["snapshot"].as_str().unwrap_or_default().into(),
        };
        for l in lines {
            let l = l?;
            if let Some(st) = l.get("state") {
                db.state = State::from_json(st);
            } else if let Some(m) = l.get("move") {
                db.moves.push(mv(m));
            } else if let Some(m) = l.get("undone") {
                db.undone.push(mv(m));
            }
        }
        Ok(db)
    }
}
