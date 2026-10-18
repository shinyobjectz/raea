//! The agent's read query on our own columns (docs/substrate.feature, phase 4): one SELECT over the edit's tables,
//! answered from the store without a database process. The tables are the ones the edit used to be saved as: each
//! kind (seq, id, its columns), edit, keys, rule, transition, asset, move, undone. Expressions are the rules' own
//! (ir::expr, eval_set, which take LIKE, IN, BETWEEN and IS [NOT] NULL too); this adds what reading needs: GROUP BY
//! and aggregates, HAVING, ORDER BY, LIMIT/OFFSET and DISTINCT. One table per SELECT; absent values read as NULL.

use std::collections::HashMap;

use sqlparser::ast::{self, Expr as SqlExpr, GroupByExpr, LimitClause, SelectItem, SetExpr, Statement, TableFactor};
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;

use crate::db::Db;
use crate::ir::{self, Expr, V};

type R<T> = Result<T, String>;

/// A table: named columns over n rows; None where a row has no value.
pub struct Table {
    pub cols: Vec<String>,
    pub data: HashMap<String, Vec<Option<V>>>,
    pub n: usize,
}

impl Table {
    fn new(cols: &[&str]) -> Table {
        Table { cols: cols.iter().map(|c| c.to_string()).collect(), data: cols.iter().map(|c| (c.to_string(), vec![])).collect(), n: 0 }
    }
    fn push(&mut self, row: Vec<Option<V>>) {
        for (c, v) in self.cols.iter().zip(row) {
            self.data.get_mut(c).expect("a column").push(v);
        }
        self.n += 1;
    }
}

const AGGS: &[&str] = &["count", "sum", "avg", "min", "max"];

/// One of the edit's tables.
pub fn table(db: &Db, name: &str) -> R<Table> {
    let s = &db.state;
    let (n, t) = (|x: f64| Some(V::N(x)), |x: &str| Some(V::S(x.to_string())));
    Ok(match name {
        "edit" => {
            let mut tb = Table::new(&["fps", "width", "height", "frames"]);
            tb.push(vec![n(s.fps), n(s.width as f64), n(s.height as f64), n(s.frames as f64)]);
            tb
        }
        "keys" => {
            let mut tb = Table::new(&["seq", "id", "prop", "frame", "value", "ease"]);
            for (i, k) in s.keys.iter().enumerate() {
                tb.push(vec![n(i as f64), t(&k.id), t(&k.prop), n(k.frame as f64), n(k.value), t(&k.ease)]);
            }
            tb
        }
        "rule" => {
            let mut tb = Table::new(&["seq", "name", "sql"]);
            for (i, (nm, q)) in s.rules.iter().enumerate() {
                tb.push(vec![n(i as f64), t(nm), t(q)]);
            }
            tb
        }
        "transition" => {
            let mut tb = Table::new(&["seq", "kind", "col", "frames", "ease", "align"]);
            for (i, x) in s.transitions.iter().enumerate() {
                tb.push(vec![n(i as f64), t(&x.kind), t(&x.col), n(x.frames as f64), t(&x.ease), t(&x.align)]);
            }
            tb
        }
        "asset" => {
            let mut tb = Table::new(&["seq", "hash", "ext", "origin"]);
            for (i, a) in s.assets.iter().enumerate() {
                tb.push(vec![n(i as f64), t(&a.hash), t(&a.ext), t(&a.origin.to_string())]);
            }
            tb
        }
        "move" | "undone" => {
            let mut tb = Table::new(if name == "move" { &["n", "text", "before", "after", "snapshot"] } else { &["n", "text", "before", "after"] });
            for m in if name == "move" { &db.moves } else { &db.undone } {
                let mut row = vec![n(m.n as f64), t(&m.text), t(&m.before), t(&m.after)];
                if name == "move" {
                    row.push(t(&m.snapshot));
                }
                tb.push(row);
            }
            tb
        }
        kind => {
            // a kind: seq (the row's place in the edit), id, then its columns as first seen
            let all = s.rows.tups();
            let rows: Vec<(usize, &crate::eval::Tup)> = all.iter().enumerate().filter(|(_, r)| r.kind == kind).collect();
            // a kind the edit knows of but has no rows of yet answers with no rows (an agent probed camera, media, clip,
            // caption, title and audio one by one on a new edit and got six errors)
            const KNOWN: &[&str] = &["camera", "media", "clip", "audio", "caption", "title", "text", "box", "image", "master", "speech",
                "light", "cube", "sphere", "plane", "mesh", "face", "cut"];
            if rows.is_empty() && KNOWN.contains(&kind) {
                return Ok(Table::new(&["seq", "id"]));
            }
            if rows.is_empty() {
                let mut kinds: Vec<&str> = all.iter().map(|r| r.kind.as_str()).collect();
                kinds.dedup();
                return Err(format!("no table {kind}: the tables are edit, keys, rule, transition, asset, move, undone, and the kinds with rows: {} (a kind's table appears with its first INSERT; there's no sqlite_master or information_schema)", if kinds.is_empty() { "none yet".to_string() } else { kinds.join(", ") }));
            }
            let mut cols: Vec<&str> = vec!["seq", "id"];
            for (_, r) in &rows {
                for c in r.vals.keys() {
                    if !cols.contains(&c.as_str()) {
                        cols.push(c);
                    }
                }
            }
            let mut tb = Table::new(&cols);
            for (i, r) in rows {
                let mut row = vec![n(i as f64), t(&r.id)];
                row.extend(cols[2..].iter().map(|c| r.vals.get(*c).cloned()));
                tb.push(row);
            }
            tb
        }
    })
}

/// Some rows of a table as a set for eval_set; a row index past the end is a row of NULLs (an empty group's).
struct Rows<'a> {
    db: &'a Db,
    t: &'a Table,
    idx: Vec<usize>,
    /// the output's aliases, readable by ORDER BY and HAVING
    extra: &'a HashMap<String, Vec<V>>,
}

impl ir::Set for Rows<'_> {
    fn n(&self) -> usize {
        self.idx.len()
    }
    fn col(&self, _: &Option<String>, c: &str) -> Option<Vec<V>> {
        if let Some(v) = self.extra.get(c) {
            return Some(v.clone());
        }
        let Some(col) = self.t.data.get(c) else {
            return (self.t.n == 0).then(|| vec![V::S(String::new()); self.idx.len()]);
        };
        Some(self.idx.iter().map(|&i| col.get(i).cloned().flatten().unwrap_or(V::S(String::new()))).collect())
    }
    fn sub(&self, agg: &str, arg: &Expr, kind: &str, filter: Option<&Expr>) -> R<V> {
        let t = table(self.db, kind)?;
        let none = HashMap::new();
        let all = Rows { db: self.db, t: &t, idx: (0..t.n).collect(), extra: &none };
        let idx = match filter {
            Some(f) => truthy(&ir::eval_set(f, &all)?, &all.idx),
            None => all.idx.clone(),
        };
        let set = Rows { db: self.db, t: &t, idx, extra: &none };
        aggregate(agg, arg, &set)
    }
}

fn truthy(m: &[V], idx: &[usize]) -> Vec<usize> {
    idx.iter().zip(m).filter(|(_, v)| v.truthy()).map(|(i, _)| *i).collect()
}

fn is_agg(e: &Expr) -> bool {
    match e {
        Expr::Call(f, args) => AGGS.contains(&f.as_str()) || args.iter().any(is_agg),
        Expr::Bin(_, a, b) => is_agg(a) || is_agg(b),
        Expr::Neg(a) | Expr::Not(a) => is_agg(a),
        _ => false,
    }
}

/// An aggregate over a set, skipping NULLs (absent values); min and max order text as text.
fn aggregate(f: &str, arg: &Expr, set: &Rows) -> R<V> {
    let vals: Vec<V> = match arg {
        // count(*)
        Expr::Lit(_) if f == "count" => return Ok(V::N(set.idx.len() as f64)),
        Expr::Col(_, c) if !set.extra.contains_key(c) && set.t.data.contains_key(c) => {
            let col = &set.t.data[c];
            set.idx.iter().filter_map(|&i| col.get(i).cloned().flatten()).collect()
        }
        e => ir::eval_set(e, set)?,
    };
    if f == "count" {
        return Ok(V::N(vals.len() as f64));
    }
    if vals.is_empty() {
        return Ok(V::S(String::new()));
    }
    if matches!(f, "min" | "max") && vals.iter().any(|v| matches!(v, V::S(_))) {
        let mut v = vals;
        v.sort_by(order);
        return Ok(if f == "min" { v.swap_remove(0) } else { v.pop().unwrap() });
    }
    ir::aggregate(f, &vals)
}

/// e with each aggregate replaced by its value over the group.
fn fold(e: &Expr, group: &Rows) -> R<Expr> {
    Ok(match e {
        Expr::Call(f, args) if AGGS.contains(&f.as_str()) => Expr::Lit(aggregate(f, args.first().unwrap_or(&Expr::Lit(V::N(1.0))), group)?),
        Expr::Call(f, args) => Expr::Call(f.clone(), args.iter().map(|a| fold(a, group)).collect::<R<_>>()?),
        Expr::Bin(op, a, b) => Expr::Bin(op.clone(), Box::new(fold(a, group)?), Box::new(fold(b, group)?)),
        Expr::Neg(a) => Expr::Neg(Box::new(fold(a, group)?)),
        Expr::Not(a) => Expr::Not(Box::new(fold(a, group)?)),
        other => other.clone(),
    })
}

/// SQL's order: numbers before text, NULLs last.
fn order(a: &V, b: &V) -> std::cmp::Ordering {
    match (a, b) {
        (V::N(x), V::N(y)) => x.total_cmp(y),
        (V::N(_), V::S(_)) => std::cmp::Ordering::Less,
        (V::S(_), V::N(_)) => std::cmp::Ordering::Greater,
        (V::S(x), V::S(y)) => x.cmp(y),
    }
}

fn order_opt(a: &Option<V>, b: &Option<V>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => order(x, y),
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
    }
}

/// The query's expressions are the rules' (ir::expr), with count(*) counting rows.
fn qexpr(st: &Statement, e: &SqlExpr) -> R<Expr> {
    Ok(match e {
        // count(*): ir drops the wildcard; give it the row marker aggregate() counts
        SqlExpr::Function(f) if f.name.to_string().eq_ignore_ascii_case("count") && f.over.is_none() && f.args.to_string().contains('*') => Expr::Call("count".into(), vec![Expr::Lit(V::N(1.0))]),
        SqlExpr::Nested(x) => qexpr(st, x)?,
        other => ir::expr(st, other)?,
    })
}

/// A SELECT's answer: column names and rows (None: NULL).
pub fn run(db: &Db, sql: &str) -> R<(Vec<String>, Vec<Vec<Option<V>>>)> {
    // [from] (SQL Server's quoting, which agents write) is "from" here
    let sql = &regex::Regex::new(r"\[([A-Za-z_][A-Za-z0-9_]*)\]").unwrap().replace_all(sql, "\"$1\"").into_owned();
    run_one(db, sql).map_err(|e| {
        // an unknown column: say which the table has (its rows may simply not have that one yet)
        match (e.starts_with("unknown column "), from_table(sql).and_then(|n| table(db, &n).ok())) {
            (true, Some(t)) => format!("{e} (no row has it; the table's columns: {})", t.cols.join(", ")),
            _ => e,
        }
    })
}

/// The table a SELECT reads, for messages.
fn from_table(sql: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?i)\bFROM\s+([A-Za-z_]+)").unwrap();
    re.captures(sql).map(|c| c[1].to_lowercase())
}

fn run_one(db: &Db, sql: &str) -> R<(Vec<String>, Vec<Vec<Option<V>>>)> {
    let stmts = Parser::parse_sql(&DuckDbDialect {}, sql).map_err(|e| format!("query: {e}"))?;
    let [st @ Statement::Query(q)] = stmts.as_slice() else { return Err("query: one SELECT per call (run each separately)".into()) };
    let SetExpr::Select(sel) = q.body.as_ref() else { return Err("query: one SELECT (no UNION, VALUES or WITH)".into()) };
    if q.with.is_some() {
        return Err("query: no WITH in a read; select from one table".into());
    }
    let [from] = sel.from.as_slice() else { return Err("query: one table in FROM".into()) };
    if !from.joins.is_empty() {
        return Err("query: no JOIN; kinds share ids, so select from each kind in turn".into());
    }
    let TableFactor::Table { name, .. } = &from.relation else { return Err("query: FROM a table".into()) };
    let t = table(db, &name.to_string().trim_matches('"').to_lowercase())?;
    let none = HashMap::new();
    let all = Rows { db, t: &t, idx: (0..t.n).collect(), extra: &none };
    let idx = match &sel.selection {
        Some(w) => truthy(&ir::eval_set(&qexpr(st, w)?, &all)?, &all.idx),
        None => all.idx.clone(),
    };
    // the projection: (name, expression, a bare column whose NULLs show as NULL)
    let mut items: Vec<(String, Expr, Option<String>)> = vec![];
    for it in &sel.projection {
        match it {
            SelectItem::Wildcard(_) => items.extend(t.cols.iter().map(|c| (c.clone(), Expr::Col(None, c.clone()), Some(c.clone())))),
            SelectItem::UnnamedExpr(e) => {
                let x = qexpr(st, e)?;
                let bare = if let Expr::Col(_, c) = &x { Some(c.clone()) } else { None };
                items.push((bare.clone().unwrap_or_else(|| e.to_string()), x, bare));
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                let x = qexpr(st, expr)?;
                let bare = if let Expr::Col(_, c) = &x { Some(c.clone()) } else { None };
                items.push((alias.value.clone(), x, bare));
            }
            other => return Err(format!("query: unsupported select item {other}")),
        }
    }
    let group_by: Vec<Expr> = match &sel.group_by {
        GroupByExpr::Expressions(es, _) => es.iter().map(|e| qexpr(st, e)).collect::<R<_>>()?,
        GroupByExpr::All(_) => return Err("query: GROUP BY the columns themselves, not ALL".into()),
    };
    let having = sel.having.as_ref().map(|h| qexpr(st, h)).transpose()?;
    let order_by: Vec<(Expr, bool)> = match &q.order_by {
        Some(ast::OrderBy { kind: ast::OrderByKind::Expressions(es), .. }) => es.iter().map(|o| Ok((qexpr(st, &o.expr)?, matches!(o.options.sort, Some(ast::OrderBySort::Desc))))).collect::<R<_>>()?,
        Some(_) => return Err("query: ORDER BY expressions".into()),
        None => vec![],
    };
    let present = |c: &Option<String>, i: usize| match c {
        Some(c) if t.data.contains_key(c) => t.data[c].get(i).cloned().flatten().is_some(),
        _ => true,
    };
    // output rows with their sort keys
    let mut out: Vec<(Vec<Option<V>>, Vec<Option<V>>)> = vec![];
    if !group_by.is_empty() || items.iter().any(|(_, e, _)| is_agg(e)) || having.is_some() {
        // groups in order of first appearance; no GROUP BY: one group, even of no rows
        let mut groups: Vec<(String, Vec<usize>)> = vec![];
        if group_by.is_empty() {
            groups.push((String::new(), idx.clone()));
        } else {
            let set = Rows { db, t: &t, idx: idx.clone(), extra: &none };
            let keys: Vec<Vec<V>> = group_by.iter().map(|g| ir::eval_set(g, &set)).collect::<R<_>>()?;
            let mut at: HashMap<String, usize> = HashMap::new();
            for (k, &i) in idx.iter().enumerate() {
                let key = keys.iter().map(|c| format!("{:?}", c[k])).collect::<Vec<_>>().join("\u{1}");
                let g = *at.entry(key.clone()).or_insert_with(|| {
                    groups.push((key, vec![]));
                    groups.len() - 1
                });
                groups[g].1.push(i);
            }
        }
        for (_, rows) in groups {
            let group = Rows { db, t: &t, idx: rows.clone(), extra: &none };
            // non-aggregate parts read the group's first row (a row of NULLs for an empty group)
            let first = vec![rows.first().copied().unwrap_or(usize::MAX)];
            let one = |e: &Expr, extra: &HashMap<String, Vec<V>>| -> R<V> {
                let set = Rows { db, t: &t, idx: first.clone(), extra };
                Ok(ir::eval_set(&fold(e, &group)?, &set)?.pop().unwrap_or(V::S(String::new())))
            };
            let mut vals = vec![];
            let mut extra: HashMap<String, Vec<V>> = HashMap::new();
            for (name, e, bare) in &items {
                let v = if !present(bare, first[0]) { None } else { Some(one(e, &none)?) };
                extra.insert(name.clone(), vec![v.clone().unwrap_or(V::S(String::new()))]);
                vals.push(v);
            }
            if let Some(h) = &having {
                if !one(h, &extra)?.truthy() {
                    continue;
                }
            }
            let keys = order_by.iter().map(|(e, _)| one(e, &extra).map(Some)).collect::<R<_>>()?;
            out.push((vals, keys));
        }
    } else {
        let set = Rows { db, t: &t, idx: idx.clone(), extra: &none };
        let cols: Vec<Vec<V>> = items.iter().map(|(_, e, _)| ir::eval_set(e, &set)).collect::<R<_>>()?;
        let extra: HashMap<String, Vec<V>> = items.iter().zip(&cols).map(|((n, _, _), c)| (n.clone(), c.clone())).collect();
        let oset = Rows { db, t: &t, idx: idx.clone(), extra: &extra };
        let okeys: Vec<Vec<V>> = order_by.iter().map(|(e, _)| ir::eval_set(e, &oset)).collect::<R<_>>()?;
        for (k, &i) in idx.iter().enumerate() {
            let vals = items.iter().zip(&cols).map(|((_, _, bare), c)| present(bare, i).then(|| c[k].clone())).collect();
            out.push((vals, okeys.iter().map(|c| Some(c[k].clone())).collect()));
        }
    }
    if !order_by.is_empty() {
        out.sort_by(|a, b| {
            for (k, (_, desc)) in order_by.iter().enumerate() {
                let o = order_opt(&a.1[k], &b.1[k]);
                let o = if *desc { o.reverse() } else { o };
                if o.is_ne() {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    let mut rows: Vec<Vec<Option<V>>> = out.into_iter().map(|(v, _)| v).collect();
    if sel.distinct.is_some() {
        let mut seen = std::collections::HashSet::new();
        rows.retain(|r| seen.insert(format!("{r:?}")));
    }
    let num = |e: &SqlExpr| -> R<usize> { e.to_string().parse().map_err(|_| format!("query: LIMIT/OFFSET take a number, not {e}")) };
    let (limit, offset) = match &q.limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => (limit.as_ref().map(num).transpose()?, offset.as_ref().map(|o| num(&o.value)).transpose()?),
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => (Some(num(limit)?), Some(num(offset)?)),
        None => (None, None),
    };
    let rows: Vec<_> = rows.into_iter().skip(offset.unwrap_or(0)).take(limit.unwrap_or(usize::MAX)).collect();
    Ok((items.into_iter().map(|(n, _, _)| n).collect(), rows))
}

/// The answer as JSON text: an array of objects, columns in select order.
pub fn to_json(cols: &[String], rows: &[Vec<Option<V>>]) -> String {
    let cell = |v: &Option<V>| match v {
        None => "null".to_string(),
        Some(V::N(x)) if x.is_finite() => serde_json::json!(x).to_string(),
        Some(V::N(x)) => serde_json::json!(x.to_string()).to_string(),
        Some(V::S(s)) => serde_json::json!(s).to_string(),
    };
    let lines: Vec<String> = rows.iter().map(|r| format!("  {{{}}}", cols.iter().zip(r).map(|(c, v)| format!("{}: {}", serde_json::json!(c), cell(v))).collect::<Vec<_>>().join(", "))).collect();
    if lines.is_empty() { "[]".into() } else { format!("[\n{}\n]", lines.join(",\n")) }
}
