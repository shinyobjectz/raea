//! The shell's sqlite3, natively: SQLite (bundled) on a project file or in memory. Statements run one after another;
//! each that returns rows gives a result set (columns and rows, blobs as {blob: base64}); the first error stops the
//! script and says where, as the sqlite3 command line does.
//!
//!   sqlite.exec {path, sql, readonly?} -> {sets: [{columns, rows}], error: string | null, changes}

use std::sync::Arc;

use rusqlite::fallible_iterator::FallibleIterator as _;
use rusqlite::types::ValueRef;
use serde_json::{Value, json};

use crate::{Call, Handler, R, enc64, s};

pub(crate) fn calls() -> Vec<Call> {
    let mk = |name: &str, args: &str, answer: &str, doc: &str, f: Handler| Call { name: name.into(), args: args.into(), answer: answer.into(), doc: doc.into(), f };
    vec![mk(
        "sqlite.exec",
        "{ path: string; sql: string; readonly?: boolean }",
        "{ sets: { columns: string[]; rows: (number | string | null | { blob: string })[][] }[]; error: string | null; changes: number }",
        "SQL run by SQLite on a database file (or \":memory:\")",
        Arc::new(exec),
    )]
}

fn cell(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(b) => json!({"blob": enc64(b)}),
    }
}

fn exec(v: &Value) -> R<Value> {
    let path = s(v, "path")?;
    let sql = s(v, "sql")?;
    let ro = v.get("readonly").and_then(Value::as_bool).unwrap_or(false);
    let flags = if ro { rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY } else { rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE } | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let conn = if path == ":memory:" { rusqlite::Connection::open_in_memory() } else { rusqlite::Connection::open_with_flags(path, flags) }.map_err(|e| format!("Error: unable to open database \"{path}\": {e}"))?;
    let mut sets = Vec::new();
    let mut error = None;
    let mut batch = rusqlite::Batch::new(&conn, sql);
    loop {
        let next = match batch.next() {
            Ok(Some(st)) => st,
            Ok(None) => break,
            Err(e) => {
                error = Some(format!("Error: {e}"));
                break;
            }
        };
        let mut st = next;
        let cols: Vec<String> = st.column_names().iter().map(|c| c.to_string()).collect();
        if cols.is_empty() {
            if let Err(e) = st.raw_execute() {
                error = Some(format!("Error: {e}"));
                break;
            }
            continue;
        }
        let mut rows = Vec::new();
        let mut q = st.raw_query();
        loop {
            match q.next() {
                Ok(Some(r)) => rows.push((0..cols.len()).map(|i| r.get_ref(i).map(cell).unwrap_or(Value::Null)).collect::<Vec<_>>()),
                Ok(None) => break,
                Err(e) => {
                    error = Some(format!("Error: {e}"));
                    break;
                }
            }
        }
        sets.push(json!({"columns": cols, "rows": rows}));
        if error.is_some() {
            break;
        }
    }
    Ok(json!({"sets": sets, "error": error, "changes": conn.changes()}))
}
