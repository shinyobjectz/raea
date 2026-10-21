//! .row: edits written as plain Odin declarations (docs/row.feature), parsed into the rows that
//! src/timeline.rs evaluates, and printed back. The rows are the truth: `parse(print(rows)) == rows`.
//! Nothing here is compiled by Odin: the syntax is Odin's, the meaning is rows (entities and their
//! components).
//!
//!   import EndCard "./EndCard.tsx"                   a component implemented in React (a .tsx node)
//!   edit  :: Edit{fps = 30, size = {1080, 1920}, frames = 450}
//!   words :: bind("words")                           an alias for a data column
//!   brand :: Style{font = "Montserrat", color = "#fff"}   a named set of props (a design token)
//!   dodge :: proc(...) -> f32 { ... }                a procedure (kept as source until procedures run)
//!   hook  :: title{"Stop scrolling", in = bg, span = 0..<90, style = brand, y = 1200}   one element = one row
//!   bg    :: box{...} { hook :: title{...} ... }     a block after an element declares its children
//!   hook.y = ease(.out, {0 = 1400, 12 = 1200})       set one prop; `{frame = value}` is a curve
//!   expect captions inside .safe_zone during 0..<450     kept as written, checked later

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

#[derive(Debug)]
pub struct Error {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

/// An element declaration (`hook :: title{...}`): one row.
struct Decl {
    id: String,
    kind: String,
    parent: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    props: Map<String, Value>,
}

struct Parser {
    s: Vec<char>,
    i: usize,
    /// `words :: bind("words")`
    aliases: BTreeMap<String, String>,
    /// `brand :: Style{...}`
    styles: Map<String, Value>,
}

impl Parser {
    fn err<T>(&self, at: usize, msg: impl Into<String>) -> Result<T, Error> {
        let before: String = self.s[..at.min(self.s.len())].iter().collect();
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
        Err(Error { line, col, msg: msg.into() })
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn starts(&self, word: &str) -> bool {
        let w: Vec<char> = word.chars().collect();
        self.s.get(self.i..self.i + w.len()) == Some(&w[..])
    }

    /// `word` as a whole identifier (not a prefix of a longer one)
    fn starts_word(&self, word: &str) -> bool {
        self.starts(word) && !self.s.get(self.i + word.chars().count()).is_some_and(|c| c.is_alphanumeric() || *c == '_')
    }

    fn ws(&mut self) {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.i += 1;
            }
            if self.starts("//") {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.i += 1;
                }
            } else {
                return;
            }
        }
    }

    fn eat(&mut self, token: &str) -> Result<(), Error> {
        self.ws();
        if self.starts(token) {
            self.i += token.chars().count();
            Ok(())
        } else {
            let found: String = self.s[self.i..].iter().take(12).collect();
            self.err(self.i, format!("expected {token:?}, found {:?}", found.split('\n').next().unwrap_or("")))
        }
    }

    /// Skip one `,` if it's next.
    fn comma(&mut self) {
        self.ws();
        if self.peek() == Some(',') {
            self.i += 1;
        }
    }

    /// Is the next thing `name =` (a named field or argument)?
    fn named(&mut self) -> bool {
        let save = self.i;
        let named = self.ident().is_ok() && {
            self.ws();
            self.peek() == Some('=')
        };
        self.i = save;
        named
    }

    fn ident(&mut self) -> Result<String, Error> {
        self.ws();
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.i += 1;
        }
        if start == self.i || self.s[start].is_ascii_digit() {
            return self.err(start, "expected a name");
        }
        Ok(self.s[start..self.i].iter().collect())
    }

    fn string(&mut self) -> Result<String, Error> {
        self.ws();
        let start = self.i;
        if self.peek() != Some('"') {
            return self.err(start, "expected a string");
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return self.err(start, "unterminated string"),
                Some('"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some(c) => out.push(c),
                        None => return self.err(start, "unterminated string"),
                    }
                    self.i += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Value, Error> {
        self.ws();
        let start = self.i;
        if self.peek() == Some('-') {
            self.i += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '_') {
            // `0..<450` is a range, not a number
            if self.peek() == Some('.') && self.s.get(self.i + 1) == Some(&'.') {
                break;
            }
            self.i += 1;
        }
        let text: String = self.s[start..self.i].iter().filter(|c| **c != '_').collect();
        if let Ok(n) = text.parse::<i64>() {
            return Ok(json!(n));
        }
        match text.parse::<f64>() {
            Ok(f) => Ok(json!(f)),
            Err(_) => self.err(start, format!("bad number {text:?}")),
        }
    }

    fn frame(&mut self, what: &str) -> Result<i64, Error> {
        self.ws();
        let at = self.i;
        match self.number()?.as_i64() {
            Some(f) => Ok(f),
            None => self.err(at, format!("{what} must be a whole frame number")),
        }
    }

    /// The source text of a balanced group starting at the current char, brackets included.
    fn balanced(&mut self, open: char, close: char) -> Result<String, Error> {
        let start = self.i;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            if c == '"' {
                self.string()?;
                continue;
            }
            self.i += 1;
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(self.s[start..self.i].iter().collect());
                }
            }
        }
        self.err(start, format!("unclosed {open:?}"))
    }

    /// `name = value, ...}` (the opening brace already eaten)
    fn fields(&mut self) -> Result<Map<String, Value>, Error> {
        let mut out = Map::new();
        loop {
            self.ws();
            if self.peek() == Some('}') {
                self.i += 1;
                return Ok(out);
            }
            let name = self.ident()?;
            self.eat("=")?;
            out.insert(name, self.value()?);
            self.comma();
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        self.ws();
        let at = self.i;
        match self.peek() {
            Some('"') => Ok(Value::String(self.string()?)),
            Some(c) if c.is_ascii_digit() || c == '-' => self.number(),
            Some('.') => {
                // an enum value: `.out`, `.center`
                self.i += 1;
                Ok(Value::String(self.ident()?))
            }
            Some('{') => {
                self.i += 1;
                self.ws();
                let save = self.i;
                // designated indices `{0 = 760, 14 = 620}`: keyframes, frame = value
                let is_keys = self.peek().is_some_and(|c| c.is_ascii_digit()) && self.number().is_ok() && {
                    self.ws();
                    self.peek() == Some('=')
                };
                self.i = save;
                if is_keys {
                    self.i -= 1;
                    return self.keys(None);
                }
                if self.named() {
                    return Ok(Value::Object(self.fields()?));
                }
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.peek() == Some('}') {
                        self.i += 1;
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value()?);
                    self.comma();
                }
            }
            Some(_) if self.starts_word("true") => {
                self.i += 4;
                Ok(json!(true))
            }
            Some(_) if self.starts_word("false") => {
                self.i += 5;
                Ok(json!(false))
            }
            Some(_) if self.starts_word("ease") => {
                // ease(.out, {0 = 760, 14 = 620})
                self.i += 4;
                self.eat("(")?;
                self.eat(".")?;
                let ease = self.ident()?;
                self.eat(",")?;
                self.ws();
                let keys = self.keys(Some(json!(ease)))?;
                self.eat(")")?;
                Ok(keys)
            }
            Some(_) if self.starts_word("spring") => {
                // spring(from, to, at = 0, stiffness = 180, damping = 12)
                self.i += 6;
                self.eat("(")?;
                let mut m = Map::new();
                let mut positional = ["from", "to"].into_iter();
                loop {
                    self.ws();
                    if self.peek() == Some(')') {
                        self.i += 1;
                        break;
                    }
                    if self.named() {
                        let k = self.ident()?;
                        self.eat("=")?;
                        m.insert(k, self.value()?);
                    } else {
                        let Some(k) = positional.next() else { return self.err(self.i, "spring takes (from, to, name = value, ...)") };
                        m.insert(k.into(), self.value()?);
                    }
                    self.comma();
                }
                Ok(json!({ "spring": m }))
            }
            Some(_) if self.starts_word("bind") || self.starts_word("asset") => {
                let name = self.ident()?;
                self.eat("(")?;
                let s = self.string()?;
                self.eat(")")?;
                Ok(json!({ name: s }))
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let name = self.ident()?;
                if self.peek() == Some('(') {
                    // a procedure call, kept as source until procedures run
                    let args = self.balanced('(', ')')?;
                    return Ok(json!({ "call": name, "args": args[1..args.len() - 1].trim() }));
                }
                match self.aliases.get(&name) {
                    Some(col) => Ok(json!({ "bind": col })),
                    None => self.err(at, format!("unknown name {name:?} (bind it first: {name} :: bind(\"column\"))")),
                }
            }
            _ => self.err(at, "expected a value"),
        }
    }

    /// `{0 = 760, 14 = 620}` (the brace not yet eaten) as a curve, with an optional ease.
    fn keys(&mut self, ease: Option<Value>) -> Result<Value, Error> {
        self.eat("{")?;
        let mut keys = Vec::new();
        loop {
            self.ws();
            if self.peek() == Some('}') {
                self.i += 1;
                break;
            }
            let f = self.frame("a key's frame")?;
            self.eat("=")?;
            keys.push(json!([f, self.value()?]));
            self.comma();
        }
        let mut m = Map::new();
        m.insert("keys".into(), Value::Array(keys));
        if let Some(e) = ease {
            m.insert("ease".into(), e);
        }
        Ok(Value::Object(m))
    }

    /// `style = brand` or `style = {brand, big}`: names of declared styles.
    fn style_names(&mut self) -> Result<Value, Error> {
        fn one(p: &mut Parser) -> Result<Value, Error> {
            p.ws();
            let at = p.i;
            let name = p.ident()?;
            if !p.styles.contains_key(&name) {
                return p.err(at, format!("unknown style {name:?} (declare it first: {name} :: Style{{...}})"));
            }
            Ok(Value::String(name))
        }
        self.ws();
        if self.peek() != Some('{') {
            return one(self);
        }
        self.i += 1;
        let mut names = Vec::new();
        loop {
            self.ws();
            if self.peek() == Some('}') {
                self.i += 1;
                return Ok(Value::Array(names));
            }
            names.push(one(self)?);
            self.comma();
        }
    }

    /// `name :: kind{"text", in = parent, span = 0..<90, prop = value, ...}`, optionally followed by a
    /// `{ ... }` block of child declarations. Returns the rows, parent first, in declaration order.
    fn decl(&mut self, name: String, parent: Option<String>, declared: &mut BTreeSet<String>, at: usize) -> Result<Vec<Decl>, Error> {
        if !declared.insert(name.clone()) || self.styles.contains_key(&name) {
            return self.err(at, format!("{name:?} is declared twice"));
        }
        let kind = self.ident()?;
        self.eat("{")?;
        let mut row = Decl { id: name.clone(), kind, parent, from: None, to: None, props: Map::new() };
        let mut first = true;
        loop {
            self.ws();
            if self.peek() == Some('}') {
                self.i += 1;
                break;
            }
            let fpos = self.i;
            if first && self.peek() == Some('"') {
                row.props.insert("text".into(), Value::String(self.string()?));
            } else {
                let field = self.ident()?;
                self.eat("=")?;
                match field.as_str() {
                    "in" => {
                        let p = self.ident()?;
                        if !declared.contains(&p) || p == name {
                            return self.err(fpos, format!("`in = {p}`: declare {p} before {name}"));
                        }
                        row.parent = Some(p);
                    }
                    "span" => {
                        let a = self.frame("a span")?;
                        self.ws();
                        let inclusive = if self.starts("..<") {
                            false
                        } else if self.starts("..=") {
                            true
                        } else {
                            return self.err(self.i, "a span is from..<to (or from..=last)");
                        };
                        self.i += 3;
                        let b = self.frame("a span")?;
                        row.from = Some(a);
                        row.to = Some(if inclusive { b + 1 } else { b });
                    }
                    "from" => row.from = Some(self.frame("from")?),
                    "to" => row.to = Some(self.frame("to")?),
                    "style" => {
                        let v = self.style_names()?;
                        row.props.insert(field, v);
                    }
                    "id" => return self.err(fpos, "the declared name is the id"),
                    _ => {
                        let v = self.value()?;
                        row.props.insert(field, v);
                    }
                }
            }
            first = false;
            self.comma();
        }
        let mut out = vec![row];
        // children: a block right after the literal, on the same line
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.i += 1;
        }
        if self.peek() == Some('{') {
            self.i += 1;
            loop {
                self.ws();
                if self.peek() == Some('}') {
                    self.i += 1;
                    break;
                }
                let cat = self.i;
                let child = self.ident()?;
                self.eat("::")?;
                out.extend(self.decl(child, Some(name.clone()), declared, cat)?);
            }
        }
        Ok(out)
    }
}

/// Parse .row into the edit's rows (the JSON src/timeline.rs reads).
pub fn parse(src: &str) -> Result<Value, Error> {
    let mut p = Parser { s: src.chars().collect(), i: 0, aliases: BTreeMap::new(), styles: Map::new() };
    let mut settings = Map::new();
    let mut imports = Map::new();
    let mut procs = Map::new();
    let mut decls: Vec<Decl> = Vec::new();
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut sets: Vec<(String, String, Value, usize)> = Vec::new();
    let mut expects = Vec::new();
    loop {
        p.ws();
        if p.peek().is_none() {
            break;
        }
        let at = p.i;
        if p.starts_word("import") {
            // import EndCard "./EndCard.tsx"
            p.i += 6;
            let name = p.ident()?;
            imports.insert(name, Value::String(p.string()?));
            continue;
        }
        if p.starts_word("expect") {
            p.i += 6;
            let start = p.i;
            while p.peek().is_some_and(|c| c != '\n') {
                p.i += 1;
            }
            let text: String = p.s[start..p.i].iter().collect();
            expects.push(json!({ "row": text.trim() }));
            continue;
        }
        let name = p.ident()?;
        p.ws();
        if p.peek() == Some('.') {
            p.i += 1;
            let prop = p.ident()?;
            p.eat("=")?;
            sets.push((name, prop, p.value()?, at));
            continue;
        }
        p.eat("::")?;
        p.ws();
        if p.starts_word("Edit") {
            p.i += 4;
            p.eat("{")?;
            for (k, v) in p.fields()? {
                match k.as_str() {
                    "size" => {
                        let wh = v.as_array().filter(|a| a.len() == 2).ok_or(()).or_else(|_| p.err(at, "size is {width, height}"))?;
                        settings.insert("width".into(), wh[0].clone());
                        settings.insert("height".into(), wh[1].clone());
                    }
                    "fps" | "frames" | "duration" => {
                        settings.insert(k, v);
                    }
                    other => return p.err(at, format!("unknown Edit setting {other:?} (fps, size, frames, duration)")),
                }
            }
        } else if p.starts_word("Style") {
            p.i += 5;
            p.eat("{")?;
            if declared.contains(&name) || p.styles.contains_key(&name) {
                return p.err(at, format!("{name:?} is declared twice"));
            }
            let fields = p.fields()?;
            p.styles.insert(name, Value::Object(fields));
        } else if p.starts_word("bind") {
            if let Value::Object(m) = p.value()? {
                p.aliases.insert(name, m["bind"].as_str().unwrap_or_default().to_string());
            }
        } else if p.starts_word("proc") {
            let start = p.i;
            while p.peek().is_some_and(|c| c != '{') {
                p.i += 1;
            }
            p.balanced('{', '}')?;
            let text: String = p.s[start..p.i].iter().collect();
            procs.insert(name, Value::String(text));
        } else {
            let save = p.i;
            let is_decl = p.ident().is_ok() && p.peek() == Some('{');
            p.i = save;
            if !is_decl {
                return p.err(p.i, format!("after `{name} ::` expected kind{{..}}, Edit{{..}}, Style{{..}}, bind(..) or proc(..)"));
            }
            decls.extend(p.decl(name, None, &mut declared, at)?);
        }
    }

    // Declarations to rows; `order` counts siblings in declaration order
    let mut orders: BTreeMap<Option<String>, usize> = BTreeMap::new();
    let mut rows: Vec<Value> = Vec::new();
    for d in decls {
        let n = orders.entry(d.parent.clone()).or_insert(0);
        let mut row = Map::new();
        row.insert("id".into(), json!(d.id));
        row.insert("kind".into(), json!(d.kind));
        if let Some(parent) = &d.parent {
            row.insert("parent".into(), json!(parent));
        }
        row.insert("order".into(), json!(*n));
        *n += 1;
        if let Some(f) = d.from {
            row.insert("from".into(), json!(f));
        }
        if let Some(t) = d.to {
            row.insert("to".into(), json!(t));
        }
        if !d.props.is_empty() {
            row.insert("props".into(), Value::Object(d.props));
        }
        rows.push(Value::Object(row));
    }
    for (id, prop, v, at) in sets {
        let row = rows.iter_mut().find(|r| r["id"] == json!(id));
        let Some(row) = row else { return p.err(at, format!("no element named {id:?}")) };
        let props = row.as_object_mut().unwrap().entry("props").or_insert_with(|| json!({}));
        props.as_object_mut().unwrap().insert(prop, v);
    }

    let mut edit = settings;
    edit.insert("instances".into(), Value::Array(rows));
    if !imports.is_empty() {
        edit.insert("imports".into(), Value::Object(imports));
    }
    if !p.styles.is_empty() {
        edit.insert("styles".into(), Value::Object(p.styles));
    }
    if !procs.is_empty() {
        edit.insert("procs".into(), Value::Object(procs));
    }
    if !expects.is_empty() {
        edit.insert("expects".into(), Value::Array(expects));
    }
    Ok(Value::Object(edit))
}

// === Printing ===

fn print_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{:?}", s),
        Value::Array(a) => format!("{{{}}}", a.iter().map(print_value).collect::<Vec<_>>().join(", ")),
        Value::Object(m) => {
            if let Some(keys) = m.get("keys").and_then(Value::as_array) {
                let curve = format!("{{{}}}", keys.iter().map(|k| format!("{} = {}", k[0], print_value(&k[1]))).collect::<Vec<_>>().join(", "));
                return match m.get("ease").and_then(Value::as_str) {
                    Some(e) => format!("ease(.{e}, {curve})"),
                    None => curve,
                };
            }
            if let (1, Some(s)) = (m.len(), m.get("spring").and_then(Value::as_object)) {
                let mut args: Vec<String> = ["from", "to"].iter().filter_map(|k| s.get(*k)).map(print_value).collect();
                args.extend(s.iter().filter(|(k, _)| *k != "from" && *k != "to").map(|(k, v)| format!("{k} = {}", print_value(v))));
                return format!("spring({})", args.join(", "));
            }
            for f in ["bind", "asset"] {
                if let (1, Some(s)) = (m.len(), m.get(f).and_then(Value::as_str)) {
                    return format!("{f}({:?})", s);
                }
            }
            if let (2, Some(name), Some(args)) = (m.len(), m.get("call").and_then(Value::as_str), m.get("args").and_then(Value::as_str)) {
                return format!("{name}({args})");
            }
            print_struct(m)
        }
        other => other.to_string(),
    }
}

fn print_struct(m: &Map<String, Value>) -> String {
    format!("{{{}}}", m.iter().map(|(k, v)| format!("{k} = {}", print_value(v))).collect::<Vec<_>>().join(", "))
}

/// Curves and springs read better on their own line (`hook.y = ...`); everything else stays inline.
fn animated(v: &Value) -> bool {
    v.get("keys").is_some() || v.get("spring").is_some()
}

/// Print rows as canonical .row: one declaration per element, its animated props on the lines after it.
pub fn print(edit: &Value) -> String {
    let mut out = String::new();
    if let Some(imports) = edit["imports"].as_object() {
        for (name, path) in imports {
            out.push_str(&format!("import {name} {}\n", print_value(path)));
        }
        out.push('\n');
    }
    let mut settings = Vec::new();
    if let Some(f) = edit.get("fps") {
        settings.push(format!("fps = {}", print_value(f)));
    }
    if let (Some(w), Some(h)) = (edit.get("width"), edit.get("height")) {
        settings.push(format!("size = {{{w}, {h}}}"));
    }
    for k in ["frames", "duration"] {
        if let Some(v) = edit.get(k) {
            settings.push(format!("{k} = {v}"));
        }
    }
    out.push_str(&format!("edit :: Edit{{{}}}\n", settings.join(", ")));
    if let Some(styles) = edit["styles"].as_object() {
        out.push('\n');
        for (name, s) in styles {
            out.push_str(&format!("{name} :: Style{}\n", s.as_object().map(print_struct).unwrap_or_default()));
        }
    }
    if let Some(procs) = edit["procs"].as_object() {
        out.push('\n');
        for (name, src) in procs {
            out.push_str(&format!("{name} :: {}\n", src.as_str().unwrap_or_default()));
        }
    }
    for row in edit["instances"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let id = row["id"].as_str().unwrap_or_default();
        let empty = Map::new();
        let props = row["props"].as_object().unwrap_or(&empty);
        let mut fields = Vec::new();
        let text = props.get("text").and_then(Value::as_str);
        if let Some(t) = text {
            fields.push(format!("{t:?}"));
        }
        if let Some(p) = row["parent"].as_str() {
            fields.push(format!("in = {p}"));
        }
        match (row["from"].as_i64(), row["to"].as_i64()) {
            (Some(a), Some(b)) => fields.push(format!("span = {a}..<{b}")),
            (Some(a), None) => fields.push(format!("from = {a}")),
            (None, Some(b)) => fields.push(format!("to = {b}")),
            (None, None) => {}
        }
        let mut later = Vec::new();
        for (k, v) in props {
            match k.as_str() {
                "text" if text.is_some() => {}
                "style" => fields.push(format!(
                    "style = {}",
                    match v {
                        Value::Array(a) => format!("{{{}}}", a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")),
                        other => other.as_str().unwrap_or_default().to_string(),
                    }
                )),
                _ if animated(v) => later.push(format!("{id}.{k} = {}", print_value(v))),
                _ => fields.push(format!("{k} = {}", print_value(v))),
            }
        }
        out.push_str(&format!("\n{id} :: {}{{{}}}\n", row["kind"].as_str().unwrap_or("box"), fields.join(", ")));
        for l in later {
            out.push_str(&l);
            out.push('\n');
        }
    }
    if let Some(expects) = edit["expects"].as_array() {
        out.push('\n');
        for e in expects {
            if let Some(t) = e["row"].as_str() {
                out.push_str(&format!("expect {t}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r##"
import Card "./Card.tsx"

edit  :: Edit{fps = 30, size = {1080, 1920}, frames = 450}
face  :: bind("face.box")
words :: bind("words")
brand :: Style{font = "Montserrat", color = "#ffffff"}

dodge :: proc(c: Box, f: Box) -> f32 {
    if overlaps(c, f) { return f.y - c.h - 24 }
    return c.y
}

scene :: stack{span = 0..<450} {
    hook :: title{"Stop scrolling", span = 0..<60, role = .title, style = brand, y = ease(.out, {0 = 1400, 12 = 1200})}
    caps :: captions{words = words, y = dodge(self.box, face)}
    card :: Card{from = 390, title = "Shop now", product = asset("hero.png")}
}

hook.opacity = spring(0, 1, at = 4)

expect captions inside .safe_zone during 0..<450
"##;

    #[test]
    fn the_example_lowers_to_rows() {
        let e = parse(EXAMPLE).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((e["fps"].clone(), e["width"].clone(), e["height"].clone(), e["frames"].clone()), (json!(30), json!(1080), json!(1920), json!(450)));
        let rows = e["instances"].as_array().unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["scene", "hook", "caps", "card"]);
        let hook = &rows[1];
        assert_eq!((hook["parent"].clone(), hook["order"].clone()), (json!("scene"), json!(0)));
        assert_eq!((hook["from"].clone(), hook["to"].clone()), (json!(0), json!(60)));
        assert_eq!(hook["props"]["role"], "title");
        assert_eq!(hook["props"]["style"], "brand");
        assert_eq!(hook["props"]["text"], "Stop scrolling");
        assert_eq!(hook["props"]["y"], json!({"keys": [[0, 1400], [12, 1200]], "ease": "out"}));
        assert_eq!(hook["props"]["opacity"], json!({"spring": {"from": 0, "to": 1, "at": 4}}));
        assert_eq!(rows[2]["props"]["words"], json!({"bind": "words"}));
        assert_eq!(rows[2]["props"]["y"], json!({"call": "dodge", "args": "self.box, face"}));
        assert_eq!((rows[3]["order"].clone(), rows[3]["props"]["product"].clone()), (json!(2), json!({"asset": "hero.png"})));
        assert_eq!(e["imports"]["Card"], "./Card.tsx");
        assert_eq!(e["styles"]["brand"]["font"], "Montserrat");
        assert!(e["procs"]["dodge"].as_str().unwrap().contains("overlaps(c, f)"));
        assert_eq!(e["expects"][0]["row"], "captions inside .safe_zone during 0..<450");
    }

    #[test]
    fn rows_round_trip_through_row() {
        let rows = parse(EXAMPLE).unwrap_or_else(|e| panic!("{e}"));
        let text = print(&rows);
        let again = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(rows, again, "parse(print(rows)) == rows\n{text}");
        assert_eq!(print(&again), text, "printing is stable");
    }

    #[test]
    fn flat_and_nested_are_the_same_rows() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/row");
        let rows = |f: &str| parse(&std::fs::read_to_string(dir.join(f)).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert_eq!(rows("hook.row"), rows("hook-nested.row"));
    }

    #[test]
    fn lowered_rows_evaluate_in_the_timeline() {
        let mut e = parse(EXAMPLE).unwrap_or_else(|e| panic!("{e}"));
        e["data"] = json!({"words": [[0, "stop"]], "face.box": [[0, [0, 0, 10, 10]]]});
        let t = crate::timeline::Timeline::new(crate::timeline::Edit::from_value(&e).unwrap());
        let f = t.frame(6);
        let hook = f["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "hook").unwrap().clone();
        let y = hook["props"]["y"].as_f64().unwrap();
        assert!(y < 1400.0 && y > 1200.0, "the title is mid-move at frame 6: {y}");
        assert_eq!(t.frame(400)["nodes"].as_array().unwrap().iter().filter(|n| n["id"] == "card").count(), 1);
    }

    #[test]
    fn mistakes_name_the_line_and_column() {
        let err = |src: &str| parse(src).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err("a :: box{y = nope}").starts_with("1:"), "{}", err("a :: box{y = nope}"));
        assert!(err("a :: box{y = nope}").contains("unknown name \"nope\""));
        assert!(err("edit :: Edit{fps = 30, speed = 2}").contains("unknown Edit setting"));
        assert!(err("a :: 5").contains("expected kind{..}"));
        assert!(err("\n\na :: box{from = 1.5}").starts_with("3:"));
        assert!(err("x.y = 1").contains("no element named"));
        assert!(err("a :: box{in = b}").contains("declare b before a"));
        assert!(err("a :: box{}\na :: box{}").starts_with("2:"));
        assert!(err("a :: box{span = 0..90}").contains("from..<to"));
        assert!(err("a :: box{id = \"x\"}").contains("the declared name is the id"));
        assert!(err("a :: box{style = nope}").contains("unknown style"));
        assert_eq!(parse("a :: box{span = 0..=9}").unwrap()["instances"][0]["to"], 10);
    }
}
