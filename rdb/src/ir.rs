//! Plain SQL (DuckDB dialect) to raea's IR (docs/relational-plan.feature, "the language: plain SQL").
//!
//! The runtime subset:
//!   INSERT INTO edit VALUES (fps, width, height, frames)
//!   INSERT INTO <kind> (cols) VALUES (...), (...)            spawn rows (a kind is a table; its columns are components)
//!   INSERT INTO <kind> (cols) SELECT e, ... FROM range(n) t(i)   spawn n rows; `i` is 0..n
//!   INSERT INTO keys VALUES (id, prop, frame, value, ease)   time: keyframes, read as of the frame
//!   [WITH r AS (SELECT id, e AS a, ... FROM <kind> [WHERE c])] UPDATE <kind> SET col = e, ... [FROM r] [WHERE c]
//!       a system: runs every frame, in file order, never cascades; `frame` is the clock
//!   CREATE VIEW expect_... AS SELECT ...                     a check (kept for DuckDB; not run per frame)
//! Expressions: numbers, 'strings', columns (bare or qualified), frame, + - * / %, ||, comparisons, AND OR NOT,
//! sin cos tan abs sqrt floor ceil round exp ln pow least greatest pi, scalar subqueries over one kind with an
//! aggregate (max min sum avg count), and windows row_number() / count(*) / sum(e) / avg(e) OVER ([ORDER BY col]).
//! Anything else is refused with the statement and the reason.

use std::collections::BTreeMap;

use sqlparser::ast::{
    self, BinaryOperator, Expr as SqlExpr, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr, Query, SelectItem, SetExpr,
    Statement, TableFactor, UnaryOperator, Value as SqlValue,
};
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;

/// A value in a column: numbers (booleans are 1 and 0) or strings.
#[derive(Debug, Clone, PartialEq)]
pub enum V {
    N(f64),
    S(String),
}

impl V {
    pub fn num(&self) -> f64 {
        match self {
            V::N(x) => *x,
            V::S(s) => s.parse().unwrap_or(0.0),
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            V::N(x) => *x != 0.0,
            V::S(s) => !s.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(V),
    /// (qualifier, column); `frame` and `i` are columns too
    Col(Option<String>, String),
    Bin(String, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Call(String, Vec<Expr>),
    /// (SELECT agg(e) FROM kind [WHERE c]): one value from another set
    Sub { agg: String, arg: Box<Expr>, kind: String, filter: Option<Box<Expr>> },
    /// f(e) OVER ([ORDER BY col]): one value per row of the set
    Window { f: String, arg: Option<Box<Expr>>, order: Option<String> },
}

#[derive(Debug, Clone)]
pub struct Cte {
    pub name: String,
    pub kind: String,
    pub filter: Option<Expr>,
    pub cols: Vec<(String, Expr)>,
}

#[derive(Debug, Clone)]
pub struct System {
    pub kind: String,
    pub ctes: Vec<Cte>,
    pub sets: Vec<(String, Expr)>,
    pub filter: Option<Expr>,
    /// the statement as written, for errors and the lineage table
    pub text: String,
}

/// `INSERT INTO transition (kind, col, frames, ease, align)`: a change in kind.col is spread over `frames` frames along
/// `ease`, starting at the change (align 'after'), ending at it ('before': the move lands as the state changes) or
/// straddling it ('center'). A frame stays a function of the edit: the eased value is built from the column's value
/// at the neighbouring frames, never from what the last frame drew.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub kind: String,
    pub col: String,
    pub frames: i64,
    pub ease: String,
    pub align: String,
}

/// A Then clause in check mode: over `kind` rows where `filter` holds, `holds` must be true after the frame's step.
#[derive(Debug, Clone)]
pub struct Assert {
    pub label: String,
    pub kind: String,
    pub filter: Option<Expr>,
    pub holds: Expr,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub frames: i64,
    /// (kind, rows of column -> value), in file order
    pub spawns: Vec<(String, Vec<BTreeMap<String, V>>)>,
    /// (id, prop) -> [(frame, value, ease)] sorted by frame
    pub keys: BTreeMap<(String, String), Vec<(i64, f64, String)>>,
    /// smooth changes: when a column's value changes, move to the new value over `frames` with an ease
    pub transitions: Vec<Transition>,
    pub systems: Vec<System>,
    /// `next:` rules (a game's state): each reads frame N-1 and writes frame N, all at once, so their order can't
    /// change the result; frame 0 is the rows as written. The columns they write carry from frame to frame and no
    /// ordinary rule or key writes them (rrl refuses that), so only edits that have them fold frame by frame.
    pub nexts: Vec<System>,
    pub checks: Vec<String>,
    pub asserts: Vec<Assert>,
}

type R<T> = Result<T, String>;

fn refuse<T>(stmt: &impl std::fmt::Display, why: &str) -> R<T> {
    let s = stmt.to_string();
    Err(format!("{why}: {}", s.chars().take(160).collect::<String>()))
}

/// A program as rdb's SQL, the text `compile` reads back to the same program: the edit row, every row in order, its
/// keys and its transitions. Rules (systems, `next:` steps, checks) are expressions this doesn't write: a program with
/// any is refused. React's frames lowered to rows have none (raea-motion `render -o OUT.sql`, for the iPhone's engine).
pub fn to_sql(p: &Program) -> R<String> {
    if !p.systems.is_empty() || !p.nexts.is_empty() || !p.checks.is_empty() || !p.asserts.is_empty() {
        return Err("to_sql writes rows, keys and transitions; this program has rules".into());
    }
    let val = |v: &V| match v {
        V::N(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", *x as i64),
        V::N(x) => format!("{x}"),
        V::S(s) => format!("'{}'", s.replace('\'', "''")),
    };
    let mut out = vec![format!("INSERT INTO edit (fps, width, height, frames) VALUES ({}, {}, {}, {});", p.fps, p.width, p.height, p.frames)];
    for (kind, rows) in &p.spawns {
        for r in rows {
            let cols: Vec<&String> = r.keys().collect();
            out.push(format!("INSERT INTO {kind} ({}) VALUES ({});", cols.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "), r.values().map(val).collect::<Vec<_>>().join(", ")));
        }
    }
    for ((id, prop), ks) in &p.keys {
        for (f, v, ease) in ks {
            out.push(format!("INSERT INTO keys VALUES ({}, {}, {f}, {}, {});", val(&V::S(id.clone())), val(&V::S(prop.clone())), val(&V::N(*v)), val(&V::S(ease.clone()))));
        }
    }
    for t in &p.transitions {
        out.push(format!("INSERT INTO transition VALUES ({}, {}, {}, {}, {});", val(&V::S(t.kind.clone())), val(&V::S(t.col.clone())), t.frames, val(&V::S(t.ease.clone())), val(&V::S(t.align.clone()))));
    }
    Ok(out.join("\n") + "\n")
}

pub fn compile(src: &str) -> R<Program> {
    let stmts = Parser::parse_sql(&DuckDbDialect {}, src).map_err(|e| e.to_string())?;
    let mut p = Program { fps: 30.0, width: 1080, height: 1920, frames: 90, ..Default::default() };
    for st in &stmts {
        match st {
            Statement::Insert(ins) => insert(&mut p, st, ins)?,
            Statement::Query(q) => p.systems.push(with_update(st, q)?),
            Statement::Update(_) => p.systems.push(update(st, &[], st)?),
            Statement::CreateView(_) => p.checks.push(st.to_string()),
            _ => return refuse(st, "not in raea's SQL (INSERT, UPDATE, WITH ... UPDATE, CREATE VIEW)"),
        }
    }
    determinism(&p)?;
    Ok(p)
}

/// The columns an expression reads, as (kind, column); unqualified and self-qualified columns belong to `own`.
fn reads(e: &Expr, own: &str, ctes: &[Cte], out: &mut Vec<(String, String)>) {
    match e {
        Expr::Lit(_) => {}
        Expr::Col(q, c) => {
            if c == "frame" || c == "id" {
                return;
            }
            match q {
                Some(q) if ctes.iter().any(|t| &t.name == q) => {}
                Some(q) => out.push((q.clone(), c.clone())),
                None => out.push((own.to_string(), c.clone())),
            }
        }
        Expr::Bin(_, a, b) => {
            reads(a, own, ctes, out);
            reads(b, own, ctes, out);
        }
        Expr::Neg(a) | Expr::Not(a) => reads(a, own, ctes, out),
        Expr::Call(_, args) => args.iter().for_each(|a| reads(a, own, ctes, out)),
        Expr::Sub { arg, kind, filter, .. } => {
            reads(arg, kind, &[], out);
            if let Some(f) = filter {
                reads(f, kind, &[], out);
            }
        }
        Expr::Window { arg, order, .. } => {
            if let Some(a) = arg {
                reads(a, own, ctes, out);
            }
            if let Some(o) = order {
                if o != "id" {
                    out.push((own.to_string(), o.clone()));
                }
            }
        }
    }
}

/// Every frame starts from the inserted rows and the keys, then the statements apply in order, so a frame is a function
/// of the edit and `frame` by construction. One thing still reads wrong: a statement reading a column that a *later*
/// statement writes gets the inserted value, not the written one, so that order is refused with the fix.
fn determinism(p: &Program) -> R<()> {
    let writes: Vec<Vec<(String, String)>> = p.systems.iter().map(|s| s.sets.iter().map(|(c, _)| (s.kind.clone(), c.clone())).collect()).collect();
    for (i, s) in p.systems.iter().enumerate() {
        let mut r = Vec::new();
        for c in &s.ctes {
            for (_, e) in &c.cols {
                reads(e, &c.kind, &[], &mut r);
            }
            if let Some(f) = &c.filter {
                reads(f, &c.kind, &[], &mut r);
            }
        }
        for (_, e) in &s.sets {
            reads(e, &s.kind, &s.ctes, &mut r);
        }
        if let Some(f) = &s.filter {
            reads(f, &s.kind, &s.ctes, &mut r);
        }
        for w in writes.iter().skip(i + 1) {
            if let Some((k, c)) = r.iter().find(|x| w.contains(x)) {
                let why = format!("reads {k}.{c} before a later statement writes it this frame: move this statement after that one");
                return Err(format!("{why}: {}", s.text.chars().take(160).collect::<String>()));
            }
        }
    }
    Ok(())
}

fn table_name(t: &ast::TableObject) -> String {
    t.to_string().to_lowercase()
}

fn insert(p: &mut Program, st: &Statement, ins: &ast::Insert) -> R<()> {
    let kind = table_name(&ins.table);
    let cols: Vec<String> = ins.columns.iter().map(|c| c.to_string().trim_matches('"').to_lowercase()).collect();
    let Some(src) = &ins.source else { return refuse(st, "INSERT needs VALUES or SELECT") };
    let rows: Vec<Vec<V>> = match src.body.as_ref() {
        SetExpr::Values(v) => v.rows.iter().map(|r| r.iter().map(|e| const_expr(st, e)).collect::<R<Vec<V>>>()).collect::<R<_>>()?,
        SetExpr::Select(sel) => {
            // SELECT e, ... FROM range(n) t(i)
            let [twj] = sel.from.as_slice() else { return refuse(st, "INSERT ... SELECT reads FROM range(n) t(i)") };
            // range(n) t(i): a table with arguments (or a table function, in older dialect builds)
            let (name, args, alias) = match &twj.relation {
                TableFactor::Table { name, args: Some(a), alias, .. } => (name, a.args.clone(), alias),
                TableFactor::Function { name, args, alias, .. } => (name, args.clone(), alias),
                _ => return refuse(st, "INSERT ... SELECT reads FROM range(n) t(i)"),
            };
            if name.to_string().to_lowercase() != "range" {
                return refuse(st, "INSERT ... SELECT reads FROM range(n) t(i)");
            }
            let n = match args.first() {
                Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(e))) => const_expr(st, e)?.num() as usize,
                _ => return refuse(st, "range(n) needs a number"),
            };
            let var = alias.as_ref().and_then(|a| a.columns.first().map(|c| c.name.value.to_lowercase())).unwrap_or("range".into());
            let exprs: Vec<Expr> = sel
                .projection
                .iter()
                .map(|it| match it {
                    SelectItem::UnnamedExpr(e) | SelectItem::ExprWithAlias { expr: e, .. } => expr(st, e),
                    _ => refuse(st, "select each value explicitly"),
                })
                .collect::<R<_>>()?;
            (0..n)
                .map(|i| {
                    let env = |q: &Option<String>, c: &str| (q.is_none() && c == var).then(|| V::N(i as f64));
                    exprs.iter().map(|e| eval_row(e, &env)).collect::<R<Vec<V>>>()
                })
                .collect::<R<_>>()?
        }
        _ => return refuse(st, "INSERT needs VALUES or SELECT"),
    };
    match kind.as_str() {
        "edit" => {
            let r = rows.first().ok_or("edit needs a row")?;
            let names = if cols.is_empty() { vec!["fps".into(), "width".into(), "height".into(), "frames".into()] } else { cols };
            for (c, v) in names.iter().zip(r) {
                match c.as_str() {
                    "fps" => p.fps = v.num(),
                    "width" => p.width = v.num() as u32,
                    "height" => p.height = v.num() as u32,
                    "frames" => p.frames = v.num() as i64,
                    other => return refuse(st, &format!("edit has fps, width, height, frames (not {other})")),
                }
            }
        }
        "transition" => {
            let names = if cols.is_empty() { vec!["kind".into(), "col".into(), "frames".into(), "ease".into(), "align".into()] } else { cols };
            for r in rows {
                let g = |k: &str| names.iter().position(|c| c == k).and_then(|i| r.get(i)).cloned();
                let (Some(V::S(kind)), Some(V::S(col))) = (g("kind"), g("col")) else { return refuse(st, "a transition is (kind, col, frames[, ease[, align]])") };
                let frames = g("frames").map(|v| v.num() as i64).unwrap_or(12).max(1);
                let ease = match g("ease") { Some(V::S(e)) if !e.is_empty() => e, _ => "inout".into() };
                let align = match g("align") { Some(V::S(a)) if !a.is_empty() => a, _ => "after".into() };
                if !matches!(align.as_str(), "after" | "before" | "center") {
                    return refuse(st, "align is 'after', 'before' or 'center'");
                }
                p.transitions.retain(|t| !(t.kind == kind && t.col == col));
                p.transitions.push(Transition { kind, col, frames, ease, align });
            }
        }
        "keys" => {
            for r in rows {
                let [id, prop, f, v, rest @ ..] = r.as_slice() else { return refuse(st, "keys rows are (id, prop, frame, value[, ease])") };
                let (V::S(id), V::S(prop)) = (id, prop) else { return refuse(st, "keys: id and prop are strings") };
                let ease = match rest.first() {
                    Some(V::S(e)) => e.clone(),
                    _ => "linear".into(),
                };
                let e = p.keys.entry((id.clone(), prop.clone())).or_default();
                e.push((f.num() as i64, v.num(), ease));
                e.sort_by_key(|k| k.0);
            }
        }
        _ => {
            if cols.is_empty() {
                return refuse(st, "name the columns: INSERT INTO kind (id, ...) VALUES (...)");
            }
            let rows = rows.into_iter().map(|r| cols.iter().cloned().zip(r).collect()).collect();
            p.spawns.push((kind, rows));
        }
    }
    Ok(())
}

fn with_update(st: &Statement, q: &Query) -> R<System> {
    let SetExpr::Update(inner) = q.body.as_ref() else { return refuse(st, "a WITH must lead into an UPDATE") };
    let mut ctes = Vec::new();
    if let Some(w) = &q.with {
        for c in &w.cte_tables {
            let name = c.alias.name.value.to_lowercase();
            let SetExpr::Select(sel) = c.query.body.as_ref() else { return refuse(st, "a CTE is one SELECT") };
            let kind = one_kind(st, sel)?;
            let cols = sel
                .projection
                .iter()
                .filter_map(|it| match it {
                    SelectItem::ExprWithAlias { expr: e, alias } => Some(expr(st, e).map(|x| (alias.value.to_lowercase(), x))),
                    SelectItem::UnnamedExpr(SqlExpr::Identifier(id)) if id.value.to_lowercase() == "id" => None,
                    _ => Some(refuse(st, "a CTE selects id and aliased expressions (e AS name)")),
                })
                .collect::<R<_>>()?;
            ctes.push(Cte { name, kind, filter: sel.selection.as_ref().map(|e| expr(st, e)).transpose()?, cols });
        }
    }
    update(inner, &ctes, st)
}

fn one_kind(st: &Statement, sel: &ast::Select) -> R<String> {
    if !matches!(sel.group_by, GroupByExpr::Expressions(ref v, _) if v.is_empty()) {
        return refuse(st, "no GROUP BY in the runtime subset");
    }
    match sel.from.as_slice() {
        [twj] if twj.joins.is_empty() => match &twj.relation {
            TableFactor::Table { name, .. } => Ok(name.to_string().to_lowercase()),
            _ => refuse(st, "FROM one kind"),
        },
        _ => refuse(st, "FROM one kind"),
    }
}

fn update(u: &Statement, ctes: &[Cte], whole: &Statement) -> R<System> {
    let Statement::Update(up) = u else { return refuse(whole, "expected UPDATE") };
    let kind = match &up.table.relation {
        TableFactor::Table { name, .. } => name.to_string().to_lowercase(),
        _ => return refuse(whole, "UPDATE one kind"),
    };
    let sets = up
        .assignments
        .iter()
        .map(|a| {
            let col = a.target.to_string().trim_matches('"').to_lowercase();
            let col = col.rsplit('.').next().unwrap_or(&col).to_string();
            expr(whole, &a.value).map(|e| (col, e))
        })
        .collect::<R<_>>()?;
    Ok(System { kind, ctes: ctes.to_vec(), sets, filter: up.selection.as_ref().map(|e| expr(whole, e)).transpose()?, text: whole.to_string() })
}

fn const_expr(st: &Statement, e: &SqlExpr) -> R<V> {
    eval_row(&expr(st, e)?, &|_, _| None)
}

pub(crate) fn expr(st: &Statement, e: &SqlExpr) -> R<Expr> {
    Ok(match e {
        SqlExpr::Value(v) => match &v.value {
            SqlValue::Number(n, _) => Expr::Lit(V::N(n.parse().map_err(|_| format!("bad number {n}"))?)),
            SqlValue::SingleQuotedString(s) | SqlValue::DoubleQuotedString(s) => Expr::Lit(V::S(s.clone())),
            // $$ ... $$: code and markup travel as data (a rule's SQL, a component's TSX, an HTML fragment)
            SqlValue::DollarQuotedString(d) => Expr::Lit(V::S(d.value.clone())),
            SqlValue::Boolean(b) => Expr::Lit(V::N(*b as u8 as f64)),
            SqlValue::Null => Expr::Lit(V::S(String::new())),
            _ => return refuse(st, &format!("unsupported value {v}")),
        },
        SqlExpr::Identifier(id) => Expr::Col(None, id.value.to_lowercase()),
        SqlExpr::CompoundIdentifier(ids) => {
            let n = ids.len();
            Expr::Col(Some(ids[n - 2].value.to_lowercase()), ids[n - 1].value.to_lowercase())
        }
        SqlExpr::Nested(inner) => expr(st, inner)?,
        // CASE [x] WHEN a THEN b ... [ELSE e] END: nested ifs (an absent ELSE is NULL, '')
        SqlExpr::Case { operand, conditions, else_result, .. } => {
            let mut out = match else_result {
                Some(e) => expr(st, e)?,
                None => Expr::Lit(V::S(String::new())),
            };
            for w in conditions.iter().rev() {
                let cond = match operand {
                    Some(x) => Expr::Bin("=".into(), Box::new(expr(st, x)?), Box::new(expr(st, &w.condition)?)),
                    None => expr(st, &w.condition)?,
                };
                out = Expr::Call("if".into(), vec![cond, expr(st, &w.result)?, out]);
            }
            out
        }
        // CAST: to an integer type rounds (as DuckDB does, half away from zero); to a float or decimal, the value; to
        // text, the value's text
        SqlExpr::Cast { expr: x, data_type, .. } => {
            let x = expr(st, x)?;
            let ty = data_type.to_string().to_uppercase();
            if ["INT", "BIGINT", "SMALLINT", "TINYINT", "HUGEINT", "INTEGER", "UBIGINT", "UINTEGER"].iter().any(|t| ty == *t || ty.starts_with(&format!("{t}("))) {
                Expr::Call("round".into(), vec![x])
            } else if ty.starts_with("VARCHAR") || ty == "TEXT" || ty == "STRING" {
                Expr::Call("concat".into(), vec![x])
            } else {
                x
            }
        }
        // the forms rules and queries share, lowered onto the operators: IN, BETWEEN, IS [NOT] NULL (NULL is ''), LIKE
        SqlExpr::InList { expr: x, list, negated } => {
            let x = expr(st, x)?;
            let any = list.iter().map(|v| Ok(Expr::Bin("=".into(), Box::new(x.clone()), Box::new(expr(st, v)?)))).collect::<R<Vec<_>>>()?
                .into_iter().reduce(|a, b| Expr::Bin("or".into(), Box::new(a), Box::new(b))).unwrap_or(Expr::Lit(V::N(0.0)));
            if *negated { Expr::Not(Box::new(any)) } else { any }
        }
        SqlExpr::Between { expr: x, negated, low, high } => {
            let x = expr(st, x)?;
            let b = Expr::Bin("and".into(), Box::new(Expr::Bin(">=".into(), Box::new(x.clone()), Box::new(expr(st, low)?))), Box::new(Expr::Bin("<=".into(), Box::new(x), Box::new(expr(st, high)?))));
            if *negated { Expr::Not(Box::new(b)) } else { b }
        }
        SqlExpr::IsNull(x) => Expr::Bin("=".into(), Box::new(expr(st, x)?), Box::new(Expr::Lit(V::S(String::new())))),
        SqlExpr::IsNotNull(x) => Expr::Not(Box::new(Expr::Bin("=".into(), Box::new(expr(st, x)?), Box::new(Expr::Lit(V::S(String::new())))))),
        SqlExpr::Like { negated, expr: x, pattern, .. } => {
            let l = Expr::Call("like".into(), vec![expr(st, x)?, expr(st, pattern)?]);
            if *negated { Expr::Not(Box::new(l)) } else { l }
        }
        SqlExpr::ILike { negated, expr: x, pattern, .. } => {
            let lower = |e| Expr::Call("lower".into(), vec![e]);
            let l = Expr::Call("like".into(), vec![lower(expr(st, x)?), lower(expr(st, pattern)?)]);
            if *negated { Expr::Not(Box::new(l)) } else { l }
        }
        SqlExpr::UnaryOp { op: UnaryOperator::Minus, expr: x } => Expr::Neg(Box::new(expr(st, x)?)),
        SqlExpr::UnaryOp { op: UnaryOperator::Plus, expr: x } => expr(st, x)?,
        SqlExpr::UnaryOp { op: UnaryOperator::Not, expr: x } => Expr::Not(Box::new(expr(st, x)?)),
        SqlExpr::BinaryOp { left, op, right } => {
            let op = match op {
                BinaryOperator::Plus => "+",
                BinaryOperator::Minus => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Modulo => "%",
                BinaryOperator::StringConcat => "||",
                BinaryOperator::Eq => "=",
                BinaryOperator::NotEq => "<>",
                BinaryOperator::Lt => "<",
                BinaryOperator::LtEq => "<=",
                BinaryOperator::Gt => ">",
                BinaryOperator::GtEq => ">=",
                BinaryOperator::And => "and",
                BinaryOperator::Or => "or",
                other => return refuse(st, &format!("unsupported operator {other}")),
            };
            Expr::Bin(op.into(), Box::new(expr(st, left)?), Box::new(expr(st, right)?))
        }
        SqlExpr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            let args: Vec<Expr> = match &f.args {
                FunctionArguments::List(l) => l
                    .args
                    .iter()
                    .filter_map(|a| match a {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Some(expr(st, e)),
                        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => None,
                        _ => Some(refuse(st, "positional function arguments only")),
                    })
                    .collect::<R<_>>()?,
                FunctionArguments::None => vec![],
                _ => return refuse(st, "unsupported function arguments"),
            };
            if let Some(w) = &f.over {
                let ast::WindowType::WindowSpec(spec) = w else { return refuse(st, "inline OVER (...) only") };
                if !spec.partition_by.is_empty() {
                    return refuse(st, "no PARTITION BY in the runtime subset");
                }
                let order = match spec.order_by.as_slice() {
                    [] => None,
                    [o] => Some(o.expr.to_string().to_lowercase()),
                    _ => return refuse(st, "ORDER BY one column in a window"),
                };
                return Ok(Expr::Window { f: name, arg: args.into_iter().next().map(Box::new), order });
            }
            Expr::Call(name, args)
        }
        SqlExpr::Subquery(q) => {
            let SetExpr::Select(sel) = q.body.as_ref() else { return refuse(st, "a subquery is one SELECT") };
            let kind = one_kind(st, sel)?;
            let [SelectItem::UnnamedExpr(SqlExpr::Function(f)) | SelectItem::ExprWithAlias { expr: SqlExpr::Function(f), .. }] = sel.projection.as_slice() else {
                return refuse(st, "a subquery selects one aggregate: (SELECT max(y) FROM kind WHERE ...)");
            };
            let agg = f.name.to_string().to_lowercase();
            let arg = match &f.args {
                FunctionArguments::List(l) => match l.args.first() {
                    Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(e))) => expr(st, e)?,
                    _ => Expr::Lit(V::N(1.0)),
                },
                _ => Expr::Lit(V::N(1.0)),
            };
            Expr::Sub { agg, arg: Box::new(arg), kind, filter: sel.selection.as_ref().map(|e| expr(st, e).map(Box::new)).transpose()? }
        }
        other => return refuse(st, &format!("unsupported expression {other}")),
    })
}

// === evaluation ===

fn bin(op: &str, a: &V, b: &V) -> V {
    let n = |x: bool| V::N(x as u8 as f64);
    match op {
        "||" => V::S(format!("{}{}", s(a), s(b))),
        "=" => n(a == b || (matches!((a, b), (V::N(_), V::N(_))) && a.num() == b.num())),
        "<>" => n(!(a == b)),
        "<" => n(a.num() < b.num()),
        "<=" => n(a.num() <= b.num()),
        ">" => n(a.num() > b.num()),
        ">=" => n(a.num() >= b.num()),
        "and" => n(a.truthy() && b.truthy()),
        "or" => n(a.truthy() || b.truthy()),
        // text joins text (rrl writes `P.title + " · $" + P.price`); numbers add, and so does text that holds a number
        // (a row's "0.24" read back from the database: joining it broke clip timing on the step 0 path)
        "+" if is_text(a) || is_text(b) => V::S(format!("{}{}", s(a), s(b))),
        "+" => V::N(a.num() + b.num()),
        "-" => V::N(a.num() - b.num()),
        "*" => V::N(a.num() * b.num()),
        "/" => V::N(a.num() / b.num()),
        _ => V::N(a.num() % b.num()),
    }
}

/// Text that isn't a number (a number written as text still adds).
fn is_text(v: &V) -> bool {
    matches!(v, V::S(x) if x.trim().parse::<f64>().is_err())
}

pub(crate) fn s(v: &V) -> String {
    match v {
        V::S(s) => s.clone(),
        V::N(x) if x.fract() == 0.0 => format!("{}", *x as i64),
        V::N(x) => x.to_string(),
    }
}

fn call(f: &str, a: &[V]) -> R<V> {
    let x = |i: usize| a.get(i).map(V::num).unwrap_or(0.0);
    let t = |i: usize| a.get(i).map(s).unwrap_or_default();
    // text functions (the interpreter's only: kernels are numeric)
    match f {
        "upper" => return Ok(V::S(t(0).to_uppercase())),
        "lower" => return Ok(V::S(t(0).to_lowercase())),
        "length" => return Ok(V::N(t(0).chars().count() as f64)),
        "concat" => return Ok(V::S(a.iter().map(s).collect())),
        "like" => return Ok(V::N(like(&t(0), &t(1)) as u8 as f64)),
        "coalesce" => return Ok(a.iter().find(|v| !matches!(v, V::S(x) if x.is_empty())).cloned().unwrap_or(V::S(String::new()))),
        "if" => return Ok(if a.first().map(V::truthy).unwrap_or(false) { a.get(1).cloned() } else { a.get(2).cloned() }.unwrap_or(V::S(String::new()))),
        _ => {}
    }
    Ok(V::N(match f {
        "sin" => x(0).sin(),
        "cos" => x(0).cos(),
        "tan" => x(0).tan(),
        "abs" => x(0).abs(),
        "sqrt" => x(0).sqrt(),
        "floor" => x(0).floor(),
        "ceil" => x(0).ceil(),
        "round" => x(0).round(),
        "exp" => x(0).exp(),
        "ln" => x(0).ln(),
        "pow" | "power" => x(0).powf(x(1)),
        "pi" => std::f64::consts::PI,
        "least" => a.iter().map(V::num).fold(f64::INFINITY, f64::min),
        "greatest" => a.iter().map(V::num).fold(f64::NEG_INFINITY, f64::max),
        // rand(frame, k): a number in [0, 1) drawn from the frame and k alone (no running stream), so a frame's
        // state stays a function of the edit and its inputs
        "rand" => {
            let mut h = (x(0) as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ x(1).to_bits().wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            h ^= h >> 31;
            h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h ^= h >> 29;
            (h >> 11) as f64 / (1u64 << 53) as f64
        }
        other => return Err(format!("unknown function {other}()")),
    }))
}

/// SQL LIKE: % any run, _ any one character.
fn like(text: &str, pat: &str) -> bool {
    let (t, p): (Vec<char>, Vec<char>) = (text.chars().collect(), pat.chars().collect());
    // matched[j]: the text so far matches p[..j]
    let mut m = vec![false; p.len() + 1];
    m[0] = true;
    for j in 0..p.len() {
        m[j + 1] = m[j] && p[j] == '%';
    }
    for &c in &t {
        let mut next = vec![false; p.len() + 1];
        for j in 0..p.len() {
            next[j + 1] = match p[j] {
                '%' => next[j] || m[j + 1],
                '_' => m[j],
                pc => m[j] && pc == c,
            };
        }
        m = next;
    }
    m[p.len()]
}

/// One row's value (spawn-time values: no windows, no subqueries).
pub fn eval_row(e: &Expr, env: &dyn Fn(&Option<String>, &str) -> Option<V>) -> R<V> {
    Ok(match e {
        Expr::Lit(v) => v.clone(),
        Expr::Col(q, c) => env(q, c).ok_or(format!("unknown column {c}"))?,
        Expr::Bin(op, a, b) => bin(op, &eval_row(a, env)?, &eval_row(b, env)?),
        Expr::Neg(a) => V::N(-eval_row(a, env)?.num()),
        Expr::Not(a) => V::N((!eval_row(a, env)?.truthy()) as u8 as f64),
        Expr::Call(f, args) => call(f, &args.iter().map(|a| eval_row(a, env)).collect::<R<Vec<_>>>()?)?,
        Expr::Sub { .. } | Expr::Window { .. } => return Err("subqueries and windows only run in UPDATE".into()),
    })
}

/// What a whole-set evaluation can see.
pub trait Set {
    /// rows in the set
    fn n(&self) -> usize;
    /// a column for every row (qualifier = a CTE name or the kind), or None
    fn col(&self, q: &Option<String>, c: &str) -> Option<Vec<V>>;
    /// one aggregate over another kind's rows
    fn sub(&self, agg: &str, arg: &Expr, kind: &str, filter: Option<&Expr>) -> R<V>;
}

/// The K evaluator: an expression over the whole set at once, one value per row (scalars broadcast).
pub fn eval_set(e: &Expr, set: &dyn Set) -> R<Vec<V>> {
    let n = set.n();
    let bc = |v: Vec<V>| if v.len() == 1 && n != 1 { vec![v[0].clone(); n] } else { v };
    Ok(match e {
        Expr::Lit(v) => vec![v.clone(); n],
        Expr::Col(q, c) => set.col(q, c).ok_or(format!("unknown column {}{c}", q.as_ref().map(|q| format!("{q}.")).unwrap_or_default()))?,
        Expr::Bin(op, a, b) => {
            let (a, b) = (bc(eval_set(a, set)?), bc(eval_set(b, set)?));
            a.iter().zip(&b).map(|(x, y)| bin(op, x, y)).collect()
        }
        Expr::Neg(a) => eval_set(a, set)?.into_iter().map(|v| V::N(-v.num())).collect(),
        Expr::Not(a) => eval_set(a, set)?.into_iter().map(|v| V::N((!v.truthy()) as u8 as f64)).collect(),
        Expr::Call(f, args) => {
            let cols: Vec<Vec<V>> = args.iter().map(|a| eval_set(a, set).map(bc)).collect::<R<_>>()?;
            (0..n).map(|i| call(f, &cols.iter().map(|c| c[i].clone()).collect::<Vec<_>>())).collect::<R<_>>()?
        }
        Expr::Sub { agg, arg, kind, filter } => vec![set.sub(agg, arg, kind, filter.as_deref())?; n],
        Expr::Window { f, arg, order } => {
            // the order: by a column, else the set's own order
            let mut idx: Vec<usize> = (0..n).collect();
            if let Some(o) = order {
                let key = set.col(&None, o).ok_or(format!("unknown column {o}"))?;
                idx.sort_by(|&a, &b| match (&key[a], &key[b]) {
                    (V::N(x), V::N(y)) => x.total_cmp(y),
                    (x, y) => s(x).cmp(&s(y)),
                }.then(a.cmp(&b)));
            }
            let x: Vec<f64> = match arg {
                Some(a) => bc(eval_set(a, set)?).iter().map(V::num).collect(),
                None => vec![1.0; n],
            };
            let mut out = vec![V::N(0.0); n];
            match (f.as_str(), order.is_some()) {
                ("row_number", _) => idx.iter().enumerate().for_each(|(r, &i)| out[i] = V::N(r as f64 + 1.0)),
                ("count", false) => out = vec![V::N(n as f64); n],
                ("sum", false) => out = vec![V::N(x.iter().sum()); n],
                ("avg", false) => out = vec![V::N(x.iter().sum::<f64>() / n.max(1) as f64); n],
                ("max", false) => out = vec![V::N(x.iter().cloned().fold(f64::NEG_INFINITY, f64::max)); n],
                ("min", false) => out = vec![V::N(x.iter().cloned().fold(f64::INFINITY, f64::min)); n],
                // ORDER BY: running aggregates (K's scan)
                ("count", true) | ("sum", true) | ("avg", true) => {
                    let mut acc = 0.0;
                    for (r, &i) in idx.iter().enumerate() {
                        acc += x[i];
                        out[i] = V::N(match f.as_str() {
                            "count" => r as f64 + 1.0,
                            "avg" => acc / (r as f64 + 1.0),
                            _ => acc,
                        });
                    }
                }
                (other, _) => return Err(format!("unsupported window {other}()")),
            }
            out
        }
    })
}

/// An aggregate over values (for subqueries).
pub fn aggregate(agg: &str, x: &[V]) -> R<V> {
    let v: Vec<f64> = x.iter().map(V::num).collect();
    Ok(V::N(match agg {
        "max" => v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        "min" => v.iter().cloned().fold(f64::INFINITY, f64::min),
        "sum" => v.iter().sum(),
        "avg" => v.iter().sum::<f64>() / v.len().max(1) as f64,
        "count" => v.len() as f64,
        other => return Err(format!("unsupported aggregate {other}()")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOK: &str = r#"
INSERT INTO edit VALUES (30, 1080, 1920, 150);
INSERT INTO cube (id, size, color, y) VALUES ('hero', 1.6, '#ff7a59', 1.4);
INSERT INTO sphere (id, class, size, color) SELECT 'ring' || i, 'ring', 0.38, '#ffd8c2' FROM range(14) t(i);
INSERT INTO keys VALUES ('hook', 'top', 0, 340, 'out'), ('hook', 'top', 18, 260, 'out');
UPDATE cube SET rot_y = frame * 0.035;
WITH r AS (SELECT id, row_number() OVER (ORDER BY id) * 2 * pi() / count(*) OVER () + frame * 0.025 AS a FROM sphere WHERE class = 'ring')
UPDATE sphere SET x = 2.4 * cos(r.a) FROM r WHERE sphere.id = r.id;
UPDATE sphere SET scale = 0.55 + 0.6 * y / (SELECT max(y) FROM sphere WHERE class = 'ring') WHERE class = 'ring';
CREATE VIEW expect_nothing AS SELECT 1 WHERE false;
"#;

    #[test]
    fn the_hook_compiles() {
        let p = compile(HOOK).unwrap();
        assert_eq!((p.fps, p.width, p.height, p.frames), (30.0, 1080, 1920, 150));
        assert_eq!(p.spawns[1].0, "sphere");
        assert_eq!(p.spawns[1].1.len(), 14);
        assert_eq!(p.spawns[1].1[13]["id"], V::S("ring13".into()));
        assert_eq!(p.keys[&("hook".into(), "top".into())].len(), 2);
        assert_eq!(p.systems.len(), 3);
        assert_eq!(p.systems[1].ctes[0].kind, "sphere");
        assert!(matches!(p.systems[2].sets[0].1, Expr::Bin(..)));
        assert_eq!(p.checks.len(), 1);
    }

    #[test]
    fn a_frame_never_depends_on_the_last_one() {
        compile("UPDATE cube SET y = y + 1;").unwrap(); // the inserted y + 1, every frame
        let e = compile("UPDATE cube SET x = y; UPDATE cube SET y = frame;").unwrap_err();
        assert!(e.contains("before a later statement writes it"), "{e}");
        compile("UPDATE cube SET y = frame; UPDATE cube SET x = y;").unwrap();
    }

    #[test]
    fn outside_the_subset_is_refused_with_the_statement() {
        let e = compile("DELETE FROM cube WHERE id = 'hero';").unwrap_err();
        assert!(e.contains("not in raea's SQL") && e.contains("DELETE"), "{e}");
        let e = compile("UPDATE cube SET y = (SELECT y FROM sphere);").unwrap_err();
        assert!(e.contains("one aggregate"), "{e}");
    }

    struct T(Vec<(String, f64)>);
    impl Set for T {
        fn n(&self) -> usize {
            self.0.len()
        }
        fn col(&self, _: &Option<String>, c: &str) -> Option<Vec<V>> {
            match c {
                "id" => Some(self.0.iter().map(|r| V::S(r.0.clone())).collect()),
                "y" => Some(self.0.iter().map(|r| V::N(r.1)).collect()),
                _ => None,
            }
        }
        fn sub(&self, agg: &str, arg: &Expr, _: &str, _: Option<&Expr>) -> R<V> {
            aggregate(agg, &eval_set(arg, self)?)
        }
    }

    #[test]
    fn windows_and_subqueries_work_on_the_whole_set() {
        let set = T(vec![("b".into(), 2.0), ("a".into(), 1.0), ("c".into(), 4.0)]);
        let p = compile("UPDATE t SET r = row_number() OVER (ORDER BY id), run = sum(y) OVER (ORDER BY id), m = y / (SELECT max(y) FROM t);").unwrap();
        let get = |k: usize| eval_set(&p.systems[0].sets[k].1, &set).unwrap().iter().map(V::num).collect::<Vec<_>>();
        assert_eq!(get(0), [2.0, 1.0, 3.0]);
        assert_eq!(get(1), [3.0, 1.0, 7.0]);
        assert_eq!(get(2), [0.5, 0.25, 1.0]);
    }
}
