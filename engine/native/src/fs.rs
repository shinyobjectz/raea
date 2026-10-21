//! A project folder as the shell's filesystem (docs/media.feature, "The sandbox boundary and the action rows"): virtual
//! paths ("/a/b") are files under the project's root and nothing else; a symlink can't lead out of it, and `.raea/`
//! (where this keeps its records) isn't visible. Every command is one action row in `.raea/actions.jsonl`: the command,
//! its verb and arguments, the files it read and wrote (each written path's state before and after: content hash,
//! folder, link or absent), bytes, exit, time, error, and the project's state hash before and after (a chain over the
//! rows). Contents are kept by hash in `.raea/objects/`, so any command can be undone and redone.
//!
//!   fs.open {root} -> {id}; fs.begin {id, cmd}; fs.end {id, exit?, error?} -> row; fs.undo / fs.redo {id} -> row | null
//!   fs.stat / fs.lstat / fs.read / fs.write / fs.mkdir / fs.readdir / fs.rm / fs.cp / fs.mv / fs.chmod / fs.symlink /
//!   fs.link / fs.readlink / fs.realpath / fs.utimes / fs.exists / fs.paths / fs.log

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::{Value, json};
use sha2::Digest as _;

use crate::{R, dec64, enc64, s};

const META: &str = ".raea";

fn projects() -> &'static Mutex<(u64, HashMap<u64, Project>)> {
    static P: OnceLock<Mutex<(u64, HashMap<u64, Project>)>> = OnceLock::new();
    P.get_or_init(|| Mutex::new((0, HashMap::new())))
}

struct Group {
    cmd: String,
    t0: Instant,
    /// each written path's state before the command first touched it
    before: BTreeMap<String, Value>,
    read: BTreeSet<String>,
    bytes: u64,
}

struct Project {
    root: PathBuf,
    group: Option<Group>,
    seq: u64,
    state: String,
    done: Vec<u64>,
    redo: Vec<u64>,
}

/// A Node-style error message, as just-bash's filesystems throw them.
fn err(code: &str, what: &str, op: &str, path: &str) -> String {
    format!("{code}: {what}, {op} '{path}'")
}

fn hash_hex(b: &[u8]) -> String {
    sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// A virtual path made absolute and lexically clean ("/a/../b" is "/b"; nothing goes above "/").
fn clean(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in Path::new(p).components() {
        match c {
            Component::Normal(x) => parts.push(x.to_str().unwrap_or("")),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(unix)]
fn mode_of(m: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    m.permissions().mode() & 0o7777
}
#[cfg(not(unix))]
fn mode_of(m: &fs::Metadata) -> u32 {
    if m.is_dir() { 0o755 } else { 0o644 }
}
#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(p, fs::Permissions::from_mode(mode))
}
#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> std::io::Result<()> {
    Ok(())
}

impl Project {
    fn real(&self, v: &str) -> R<PathBuf> {
        if v.contains('\0') {
            return Err(err("EINVAL", "path contains a null byte", "open", &v.replace('\0', "\\0")));
        }
        let v = clean(v);
        if v == format!("/{META}") || v.starts_with(&format!("/{META}/")) {
            return Err(err("ENOENT", "no such file or directory", "open", &v));
        }
        Ok(self.root.join(v.trim_start_matches('/')))
    }
    fn virt(&self, real: &Path) -> Option<String> {
        real.strip_prefix(&self.root).ok().map(|r| format!("/{}", r.to_string_lossy()).replace("//", "/")).map(|s| if s.len() > 1 { s.trim_end_matches('/').to_string() } else { s })
    }
    /// The real path with every link followed, refused if it leaves the project.
    fn follow(&self, v: &str, op: &str) -> R<PathBuf> {
        let p = self.real(v)?;
        match p.canonicalize() {
            Ok(c) if c.starts_with(&self.root) => {
                if c.strip_prefix(&self.root).ok().and_then(|r| r.components().next()).is_some_and(|c| c.as_os_str() == META) {
                    return Err(err("ENOENT", "no such file or directory", op, v));
                }
                Ok(c)
            }
            Ok(_) => Err(err("EACCES", "permission denied (outside the project)", op, v)),
            Err(e) if e.raw_os_error() == Some(62) => Err(err("ELOOP", "too many levels of symbolic links", op, v)),
            Err(_) => Err(err("ENOENT", "no such file or directory", op, v)),
        }
    }
    /// `follow` for reading: a link the space was given (placed by the host: a space's media, a project's takes) may
    /// lead outside the folder, and what's behind it can be read, never written. The shell can't make such a link
    /// itself (fs.symlink refuses a target that leaves the folder), so reading through one reads only what the host
    /// put in reach.
    fn follow_read(&self, v: &str, op: &str) -> R<PathBuf> {
        match self.follow(v, op) {
            Err(e) if e.starts_with("EACCES") => self.real(v)?.canonicalize().map_err(|_| err("ENOENT", "no such file or directory", op, v)),
            r => r,
        }
    }
    /// `parent_followed` for reading (lstat, readlink): the parent may be behind a link the host placed.
    fn parent_followed_read(&self, v: &str, op: &str) -> R<PathBuf> {
        match self.parent_followed(v, op) {
            Err(e) if e.starts_with("EACCES") => {
                let c = clean(v);
                let (dir, name) = c.rsplit_once('/').unwrap();
                let d = self.follow_read(if dir.is_empty() { "/" } else { dir }, op)?;
                Ok(d.join(name))
            }
            r => r,
        }
    }
    /// The real path of `v`'s parent followed, with `v`'s own name kept (for writes, links and lstat).
    fn parent_followed(&self, v: &str, op: &str) -> R<PathBuf> {
        if v.contains('\0') {
            return Err(err("EINVAL", "path contains a null byte", op, &v.replace('\0', "\\0")));
        }
        let c = clean(v);
        if c == "/" {
            return Ok(self.root.clone());
        }
        let (dir, name) = c.rsplit_once('/').unwrap();
        let dir = if dir.is_empty() { "/" } else { dir };
        let d = self.follow(dir, op).map_err(|e| if e.starts_with("ENOENT") { err("ENOENT", "no such file or directory", op, &c) } else { e })?;
        if !d.is_dir() {
            return Err(err("ENOTDIR", "not a directory", op, &c));
        }
        if name == META && d == self.root {
            return Err(err("ENOENT", "no such file or directory", op, &c));
        }
        Ok(d.join(name))
    }

    fn objects(&self) -> PathBuf {
        self.root.join(META).join("objects")
    }
    fn keep(&self, bytes: &[u8]) -> R<String> {
        let h = hash_hex(bytes);
        let o = self.objects().join(&h);
        if !o.exists() {
            fs::create_dir_all(self.objects()).map_err(|e| e.to_string())?;
            fs::write(&o, bytes).map_err(|e| e.to_string())?;
        }
        Ok(h)
    }
    /// A path's state for the record: a file's content hash and mode, a folder, a link's target, or null.
    fn state_of(&self, real: &Path) -> R<Value> {
        match fs::symlink_metadata(real) {
            Err(_) => Ok(Value::Null),
            Ok(m) if m.file_type().is_symlink() => Ok(json!({"kind": "link", "target": fs::read_link(real).map(|t| t.to_string_lossy().to_string()).unwrap_or_default()})),
            Ok(m) if m.is_dir() => Ok(json!({"kind": "dir", "mode": mode_of(&m)})),
            Ok(m) => {
                let b = fs::read(real).map_err(|e| e.to_string())?;
                Ok(json!({"kind": "file", "hash": self.keep(&b)?, "mode": mode_of(&m), "size": b.len()}))
            }
        }
    }
    /// Remembers `real` (and, for a folder, everything under it) as it was before the command first changed it.
    fn touch(&mut self, real: &Path) -> R<()> {
        let mut paths = vec![real.to_path_buf()];
        if fs::symlink_metadata(real).is_ok_and(|m| m.is_dir()) {
            let mut stack = vec![real.to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
                        stack.push(p.clone());
                    }
                    paths.push(p);
                }
            }
        }
        for p in paths {
            let Some(v) = self.virt(&p) else { continue };
            if self.group.as_ref().is_some_and(|g| g.before.contains_key(&v)) {
                continue;
            }
            let st = self.state_of(&p)?;
            if let Some(g) = self.group.as_mut() {
                g.before.insert(v, st);
            }
        }
        Ok(())
    }

    fn begin(&mut self, cmd: &str) {
        self.group = Some(Group { cmd: cmd.into(), t0: Instant::now(), before: BTreeMap::new(), read: BTreeSet::new(), bytes: 0 });
    }

    /// Closes the open command into its row (written to actions.jsonl); null when nothing is open.
    fn end(&mut self, exit: i64, error: Option<&str>, kind: &str, target: Option<u64>) -> R<Value> {
        let Some(mut g) = self.group.take() else { return Ok(Value::Null) };
        // what appeared under a folder the command was given (a tool's outputs, an extraction): absent before
        let dirs: Vec<String> = g.before.keys().filter(|v| fs::symlink_metadata(self.root.join(v.trim_start_matches('/'))).is_ok_and(|m| m.is_dir())).cloned().collect();
        for d in dirs {
            let mut stack = vec![self.root.join(d.trim_start_matches('/'))];
            while let Some(x) = stack.pop() {
                for e in fs::read_dir(&x).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.starts_with(self.root.join(META)) {
                        continue;
                    }
                    if let Some(v) = self.virt(&p) {
                        g.before.entry(v).or_insert(Value::Null);
                    }
                    if e.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(p);
                    }
                }
            }
        }
        let mut written = Vec::new();
        for (v, before) in &g.before {
            let after = self.state_of(&self.root.join(v.trim_start_matches('/')))?;
            if &after != before {
                written.push(json!({"path": v, "before": before, "after": after}));
            }
        }
        self.seq += 1;
        let words: Vec<&str> = g.cmd.split_whitespace().collect();
        let state_before = self.state.clone();
        let mut h = sha2::Sha256::new();
        h.update(state_before.as_bytes());
        h.update(serde_json::to_vec(&written).unwrap_or_default());
        let state_after: String = h.finalize().iter().map(|x| format!("{x:02x}")).collect();
        if !written.is_empty() {
            self.state = state_after.clone();
        }
        let row = json!({
            "seq": self.seq,
            "kind": kind,
            "target": target,
            "at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            "cmd": g.cmd,
            "verb": words.first().copied().unwrap_or(""),
            "args": words.iter().skip(1).copied().collect::<Vec<_>>(),
            "read": g.read.iter().collect::<Vec<_>>(),
            "written": written,
            "bytes": g.bytes,
            "exit": exit,
            "ms": (g.t0.elapsed().as_secs_f64() * 1e4).round() / 10.0,
            "error": error,
            "state_before": state_before,
            "state_after": if self.state == state_before { state_before.clone() } else { state_after },
        });
        match kind {
            "undo" => {
                self.done.pop();
                if let Some(t) = target {
                    self.redo.push(t);
                }
            }
            "redo" => {
                self.redo.pop();
                if let Some(t) = target {
                    self.done.push(t);
                }
            }
            _ => {
                if !row["written"].as_array().is_some_and(|w| w.is_empty()) {
                    self.done.push(self.seq);
                    self.redo.clear();
                }
            }
        }
        fs::create_dir_all(self.root.join(META)).map_err(|e| e.to_string())?;
        let mut f = fs::OpenOptions::new().create(true).append(true).open(self.root.join(META).join("actions.jsonl")).map_err(|e| e.to_string())?;
        writeln!(f, "{row}").map_err(|e| e.to_string())?;
        Ok(row)
    }

    fn rows(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join(META).join("actions.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    /// Puts each path of a row back to its `before` (undo) or `after` (redo) state.
    fn apply(&mut self, row: &Value, side: &str) -> R<()> {
        let mut entries: Vec<(String, Value)> = row["written"].as_array().into_iter().flatten().map(|w| (w["path"].as_str().unwrap_or("").to_string(), w[side].clone())).collect();
        // removals deepest first, then creations shallowest first
        entries.sort_by_key(|(p, _)| std::cmp::Reverse(p.matches('/').count()));
        for (v, st) in &entries {
            if st.is_null() {
                let p = self.root.join(v.trim_start_matches('/'));
                if let Ok(m) = fs::symlink_metadata(&p) {
                    self.touch(&p)?;
                    if m.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) }.map_err(|e| e.to_string())?;
                }
            }
        }
        entries.reverse();
        for (v, st) in &entries {
            if st.is_null() {
                continue;
            }
            let p = self.root.join(v.trim_start_matches('/'));
            self.touch(&p)?;
            match st["kind"].as_str() {
                Some("dir") => {
                    if fs::symlink_metadata(&p).is_ok_and(|m| !m.is_dir()) {
                        fs::remove_file(&p).map_err(|e| e.to_string())?;
                    }
                    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                    let _ = set_mode(&p, st["mode"].as_u64().unwrap_or(0o755) as u32);
                }
                Some("file") => {
                    if fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
                        fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
                    } else if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                        fs::remove_file(&p).map_err(|e| e.to_string())?;
                    }
                    let b = fs::read(self.objects().join(st["hash"].as_str().unwrap_or(""))).map_err(|e| format!("the kept copy of {v} is gone: {e}"))?;
                    fs::write(&p, b).map_err(|e| e.to_string())?;
                    let _ = set_mode(&p, st["mode"].as_u64().unwrap_or(0o644) as u32);
                }
                Some("link") => {
                    if fs::symlink_metadata(&p).is_ok() {
                        if p.is_dir() && !fs::symlink_metadata(&p).unwrap().file_type().is_symlink() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) }.map_err(|e| e.to_string())?;
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(st["target"].as_str().unwrap_or(""), &p).map_err(|e| e.to_string())?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn stat_json(&self, m: &fs::Metadata) -> Value {
        let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64() * 1e3).unwrap_or(0.0);
        // dev and ino: a file's stable identity, which just-bash's cp and mv need to tell two existing files apart (without
        // them `mv new.tsx edit.tsx` refused: "cannot safely determine whether ... are the same file"); as numbers, exact
        // to 2^53, which covers APFS's inode numbers
        #[cfg(unix)]
        let (dev, ino) = { use std::os::unix::fs::MetadataExt; (m.dev() as f64, m.ino() as f64) };
        #[cfg(not(unix))]
        let (dev, ino) = (0.0, 0.0);
        json!({"isFile": m.is_file(), "isDirectory": m.is_dir(), "isSymbolicLink": m.file_type().is_symlink(), "mode": mode_of(m), "size": m.len(), "mtimeMs": mtime, "dev": dev, "ino": ino})
    }
}

fn with<T>(v: &Value, f: impl FnOnce(&mut Project) -> R<T>) -> R<T> {
    let id = v.get("id").and_then(Value::as_u64).ok_or("id: an open project")?;
    let mut g = projects().lock().unwrap();
    let p = g.1.get_mut(&id).ok_or_else(|| format!("no open project {id}"))?;
    // a call outside any command is its own row
    let implicit = p.group.is_none();
    if implicit {
        p.begin("(api)");
    }
    let r = f(p);
    if implicit {
        let wrote = p.group.as_ref().is_some_and(|g| !g.before.is_empty());
        if wrote {
            p.end(if r.is_ok() { 0 } else { 1 }, r.as_ref().err().map(String::as_str), "cmd", None)?;
        } else {
            p.group = None;
        }
    }
    r
}

fn copy_tree(p: &mut Project, from: &Path, to: &Path, recursive: bool, vsrc: &str) -> R<()> {
    let m = fs::symlink_metadata(from).map_err(|_| err("ENOENT", "no such file or directory", "cp", vsrc))?;
    if m.is_dir() {
        if !recursive {
            return Err(err("EISDIR", "illegal operation on a directory", "cp", vsrc));
        }
        p.touch(to)?;
        fs::create_dir_all(to).map_err(|e| e.to_string())?;
        for e in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
            if from == p.root && e.file_name() == META {
                continue;
            }
            copy_tree(p, &e.path(), &to.join(e.file_name()), true, vsrc)?;
        }
    } else if m.file_type().is_symlink() {
        p.touch(to)?;
        let t = fs::read_link(from).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(to);
        #[cfg(unix)]
        std::os::unix::fs::symlink(t, to).map_err(|e| e.to_string())?;
    } else {
        p.touch(to)?;
        if fs::symlink_metadata(to).is_ok_and(|m| m.is_dir()) {
            return Err(err("EISDIR", "illegal operation on a directory", "cp", &p.virt(to).unwrap_or_default()));
        }
        let n = fs::copy(from, to).map_err(|e| e.to_string())?;
        if let Some(g) = p.group.as_mut() {
            g.bytes += n;
        }
    }
    Ok(())
}

/// The fs.* calls in the registry.
pub(crate) fn calls() -> Vec<crate::Call> {
    use std::sync::Arc;
    const STAT: &str = "{ isFile: boolean; isDirectory: boolean; isSymbolicLink: boolean; mode: number; size: number; mtimeMs: number; dev: number; ino: number }";
    const ROW: &str = "Record<string, unknown>";
    let mk = |name: &str, args: &str, answer: &str, doc: &str, f: crate::Handler| crate::Call { name: name.into(), args: args.into(), answer: answer.into(), doc: doc.into(), f };
    let p = |v: &Value| s(v, "path").map(str::to_string);
    vec![
        mk("fs.open", "{ root: string }", "{ id: number; root: string }", "a project folder opened as a filesystem (made if missing)", Arc::new(|v| {
            let root = Path::new(s(v, "root")?);
            fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
            let root = root.canonicalize().map_err(|e| e.to_string())?;
            let mut pr = Project { root: root.clone(), group: None, seq: 0, state: "0".repeat(64), done: Vec::new(), redo: Vec::new() };
            for r in pr.rows() {
                pr.seq = r["seq"].as_u64().unwrap_or(pr.seq);
                if let Some(st) = r["state_after"].as_str() {
                    pr.state = st.to_string();
                }
                let t = r["target"].as_u64();
                match r["kind"].as_str() {
                    Some("undo") => {
                        pr.done.pop();
                        pr.redo.extend(t);
                    }
                    Some("redo") => {
                        pr.redo.pop();
                        pr.done.extend(t);
                    }
                    _ if r["written"].as_array().is_some_and(|w| !w.is_empty()) => {
                        pr.done.push(r["seq"].as_u64().unwrap_or(0));
                        pr.redo.clear();
                    }
                    _ => {}
                }
            }
            let mut g = projects().lock().unwrap();
            g.0 += 1;
            let id = g.0;
            g.1.insert(id, pr);
            Ok(json!({"id": id, "root": root.display().to_string()}))
        })),
        mk("fs.begin", "{ id: number; cmd: string }", "null", "a command starts: what follows is its row", Arc::new(|v| {
            let id = v.get("id").and_then(Value::as_u64).ok_or("id")?;
            let mut g = projects().lock().unwrap();
            let pr = g.1.get_mut(&id).ok_or("no such project")?;
            if pr.group.is_some() {
                pr.end(-1, Some("interrupted by the next command"), "cmd", None)?;
            }
            pr.begin(s(v, "cmd")?);
            Ok(Value::Null)
        })),
        mk("fs.end", "{ id: number; exit?: number; error?: string }", &format!("{ROW} | null"), "the command ends: its action row", Arc::new(|v| {
            let id = v.get("id").and_then(Value::as_u64).ok_or("id")?;
            let mut g = projects().lock().unwrap();
            let pr = g.1.get_mut(&id).ok_or("no such project")?;
            pr.end(v.get("exit").and_then(Value::as_i64).unwrap_or(0), v.get("error").and_then(Value::as_str), "cmd", None)
        })),
        mk("fs.undo", "{ id: number }", &format!("{ROW} | null"), "the last command's changes taken back (null when there's none)", Arc::new(|v| undo_redo(v, true))),
        mk("fs.redo", "{ id: number }", &format!("{ROW} | null"), "the last undone command's changes made again", Arc::new(|v| undo_redo(v, false))),
        mk("fs.log", "{ id: number; last?: number }", &format!("{ROW}[]"), "the project's action rows (the last n)", Arc::new(|v| {
            let n = v.get("last").and_then(Value::as_u64).map(|n| n as usize);
            with(v, |pr| {
                let rows = pr.rows();
                let k = n.map(|n| rows.len().saturating_sub(n)).unwrap_or(0);
                Ok(json!(rows[k..]))
            })
        })),
        mk("fs.touch", "{ id: number; path: string }", "null", "a path's state kept as it is now, before a tool outside the filesystem changes it (so the command's row has it, and undo can restore it)", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.parent_followed(&path, "open")?;
                pr.touch(&r)?;
                Ok(Value::Null)
            })
        })),
        mk("fs.stat", "{ id: number; path: string }", STAT, "a path's facts, links followed", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.follow_read(&path, "stat")?;
                let m = fs::metadata(&r).map_err(|_| err("ENOENT", "no such file or directory", "stat", &path))?;
                Ok(pr.stat_json(&m))
            })
        })),
        mk("fs.lstat", "{ id: number; path: string }", STAT, "a path's facts, a link itself", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.parent_followed_read(&path, "lstat")?;
                let m = fs::symlink_metadata(&r).map_err(|_| err("ENOENT", "no such file or directory", "lstat", &path))?;
                Ok(pr.stat_json(&m))
            })
        })),
        mk("fs.exists", "{ id: number; path: string }", "boolean", "whether a path is there (links followed)", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| Ok(json!(pr.follow_read(&path, "access").is_ok())))
        })),
        mk("fs.read", "{ id: number; path: string }", "string", "a file's bytes, base64", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.follow_read(&path, "open")?;
                if r.is_dir() {
                    return Err(err("EISDIR", "illegal operation on a directory", "read", &path));
                }
                let b = fs::read(&r).map_err(|_| err("ENOENT", "no such file or directory", "open", &path))?;
                if let Some(g) = pr.group.as_mut() {
                    g.read.insert(clean(&path));
                }
                Ok(json!(enc64(&b)))
            })
        })),
        mk("fs.write", "{ id: number; path: string; data: string; append?: boolean; exclusive?: boolean; mode?: number }", "null", "bytes written (or appended) to a file", Arc::new(move |v| {
            let path = p(v)?;
            let data = dec64(s(v, "data")?)?;
            let append = v.get("append").and_then(Value::as_bool).unwrap_or(false);
            let exclusive = v.get("exclusive").and_then(Value::as_bool).unwrap_or(false);
            let mode = v.get("mode").and_then(Value::as_u64);
            with(v, |pr| {
                // a link is written through, inside the project
                let mut r = pr.parent_followed(&path, "open")?;
                if exclusive {
                    if fs::symlink_metadata(&r).is_ok() {
                        return Err(err("EEXIST", "file already exists", "open", &path));
                    }
                } else if fs::symlink_metadata(&r).is_ok_and(|m| m.file_type().is_symlink()) {
                    r = pr.follow(&path, "open")?;
                }
                if r.is_dir() {
                    return Err(err("EISDIR", "illegal operation on a directory", "write", &path));
                }
                pr.touch(&r)?;
                let mut f = fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&r).map_err(|e| format!("{path}: {e}"))?;
                f.write_all(&data).map_err(|e| e.to_string())?;
                if let Some(m) = mode {
                    let _ = set_mode(&r, m as u32);
                }
                if let Some(g) = pr.group.as_mut() {
                    g.bytes += data.len() as u64;
                }
                Ok(Value::Null)
            })
        })),
        mk("fs.mkdir", "{ id: number; path: string; recursive?: boolean; exclusive?: boolean; mode?: number }", "null", "a folder made", Arc::new(move |v| {
            let path = p(v)?;
            let recursive = v.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            let exclusive = v.get("exclusive").and_then(Value::as_bool).unwrap_or(false);
            let mode = v.get("mode").and_then(Value::as_u64);
            with(v, |pr| {
                let c = clean(&path);
                if recursive {
                    let mut cur = String::new();
                    for part in c.split('/').filter(|x| !x.is_empty()) {
                        cur = format!("{cur}/{part}");
                        let r = pr.parent_followed(&cur, "mkdir")?;
                        match fs::metadata(&r) {
                            Ok(m) if m.is_dir() => continue,
                            Ok(_) => return Err(err("EEXIST", "file already exists", "mkdir", &cur)),
                            Err(_) => {
                                pr.touch(&r)?;
                                fs::create_dir(&r).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    return Ok(Value::Null);
                }
                let r = pr.parent_followed(&c, "mkdir")?;
                if let Ok(m) = fs::symlink_metadata(&r) {
                    return Err(err("EEXIST", if m.is_dir() && !exclusive { "directory already exists" } else { "file already exists" }, "mkdir", &c));
                }
                pr.touch(&r)?;
                fs::create_dir(&r).map_err(|e| e.to_string())?;
                if let Some(m) = mode {
                    let _ = set_mode(&r, m as u32);
                }
                Ok(Value::Null)
            })
        })),
        mk("fs.readdir", "{ id: number; path: string }", "{ name: string; isFile: boolean; isDirectory: boolean; isSymbolicLink: boolean }[]", "a folder's entries, by name", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.follow_read(&path, "scandir")?;
                if !r.is_dir() {
                    return Err(err("ENOTDIR", "not a directory", "scandir", &path));
                }
                let mut out = Vec::new();
                for e in fs::read_dir(&r).map_err(|e| e.to_string())?.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if r == pr.root && name == META {
                        continue;
                    }
                    let ft = e.file_type().map_err(|e| e.to_string())?;
                    out.push(json!({"name": name, "isFile": ft.is_file(), "isDirectory": ft.is_dir(), "isSymbolicLink": ft.is_symlink()}));
                }
                out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                Ok(Value::Array(out))
            })
        })),
        mk("fs.rm", "{ id: number; path: string; recursive?: boolean; force?: boolean }", "null", "a file, link or folder removed", Arc::new(move |v| {
            let path = p(v)?;
            let recursive = v.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            let force = v.get("force").and_then(Value::as_bool).unwrap_or(false);
            with(v, |pr| {
                let c = clean(&path);
                if c == "/" {
                    return Err(err("EPERM", "operation not permitted", "rm", &c));
                }
                let r = match pr.parent_followed(&c, "rm") {
                    Ok(r) => r,
                    Err(_) if force => return Ok(Value::Null),
                    Err(e) => return Err(e),
                };
                let Ok(m) = fs::symlink_metadata(&r) else {
                    return if force { Ok(Value::Null) } else { Err(err("ENOENT", "no such file or directory", "rm", &c)) };
                };
                if m.is_dir() && !m.file_type().is_symlink() {
                    if !recursive && fs::read_dir(&r).map(|mut d| d.next().is_some()).unwrap_or(false) {
                        return Err(err("ENOTEMPTY", "directory not empty", "rm", &c));
                    }
                    pr.touch(&r)?;
                    fs::remove_dir_all(&r).map_err(|e| e.to_string())?;
                } else {
                    pr.touch(&r)?;
                    fs::remove_file(&r).map_err(|e| e.to_string())?;
                }
                Ok(Value::Null)
            })
        })),
        mk("fs.cp", "{ id: number; src: string; dest: string; recursive?: boolean }", "null", "a file (or with recursive, a folder) copied", Arc::new(|v| {
            let (src, dest) = (s(v, "src")?.to_string(), s(v, "dest")?.to_string());
            let recursive = v.get("recursive").and_then(Value::as_bool).unwrap_or(false);
            with(v, |pr| {
                let from = pr.follow_read(&src, "cp")?;
                let to = pr.parent_followed(&dest, "cp")?;
                if to.starts_with(&from) && from.is_dir() {
                    return Err(err("EINVAL", "can't copy a folder into itself", "cp", &dest));
                }
                copy_tree(pr, &from, &to, recursive, &src)?;
                Ok(Value::Null)
            })
        })),
        mk("fs.mv", "{ id: number; src: string; dest: string }", "null", "a path moved", Arc::new(|v| {
            let (src, dest) = (s(v, "src")?.to_string(), s(v, "dest")?.to_string());
            with(v, |pr| {
                let from = pr.parent_followed(&src, "rename")?;
                if fs::symlink_metadata(&from).is_err() {
                    return Err(err("ENOENT", "no such file or directory", "rename", &src));
                }
                let to = pr.parent_followed(&dest, "rename")?;
                if to.starts_with(&from) && to != from {
                    return Err(err("EINVAL", "can't move a folder into itself", "rename", &dest));
                }
                pr.touch(&from)?;
                pr.touch(&to)?;
                fs::rename(&from, &to).map_err(|e| format!("{dest}: {e}"))?;
                pr.touch(&to)?;
                Ok(Value::Null)
            })
        })),
        mk("fs.chmod", "{ id: number; path: string; mode: number }", "null", "a path's mode set", Arc::new(move |v| {
            let path = p(v)?;
            let mode = v.get("mode").and_then(Value::as_u64).ok_or("mode")? as u32;
            with(v, |pr| {
                let r = pr.follow(&path, "chmod")?;
                pr.touch(&r)?;
                set_mode(&r, mode).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
        })),
        mk("fs.symlink", "{ id: number; target: string; path: string }", "null", "a link made (an absolute target is a project path)", Arc::new(move |v| {
            let path = p(v)?;
            let target = s(v, "target")?.to_string();
            with(v, |pr| {
                let r = pr.parent_followed(&path, "symlink")?;
                if fs::symlink_metadata(&r).is_ok() {
                    return Err(err("EEXIST", "file already exists", "symlink", &path));
                }
                pr.touch(&r)?;
                // an absolute target names a project path: the link points at it under the root, and reads back as it
                let t: PathBuf = if target.starts_with('/') {
                    pr.root.join(clean(&target).trim_start_matches('/'))
                } else {
                    // a relative target that climbs out of the folder (lexically, from the link's own folder) is
                    // refused: links that lead outside are the host's to place, so reads through them stay in reach
                    let mut at = r.parent().unwrap_or(&pr.root).to_path_buf();
                    for c in Path::new(&target).components() {
                        match c {
                            Component::ParentDir => {
                                if !at.pop() || !at.starts_with(&pr.root) {
                                    return Err(err("EACCES", "permission denied (a link out of the project)", "symlink", &path));
                                }
                            }
                            Component::Normal(x) => at.push(x),
                            _ => {}
                        }
                    }
                    if !at.starts_with(&pr.root) {
                        return Err(err("EACCES", "permission denied (a link out of the project)", "symlink", &path));
                    }
                    PathBuf::from(&target)
                };
                #[cfg(unix)]
                std::os::unix::fs::symlink(&t, &r).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
        })),
        mk("fs.place_link", "{ id: number; path: string; target: string }", "null", "the host's: a link in the folder to a real path anywhere (a space's media), read-only through the shell", Arc::new(move |v| {
            let path = p(v)?;
            let target = s(v, "target")?.to_string();
            with(v, |pr| {
                let r = pr.parent_followed(&path, "symlink")?;
                if fs::symlink_metadata(&r).is_ok() {
                    return Err(err("EEXIST", "file already exists", "symlink", &path));
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &r).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
        })),
        mk("fs.link", "{ id: number; existing: string; path: string }", "null", "a hard link made", Arc::new(move |v| {
            let path = p(v)?;
            let existing = s(v, "existing")?.to_string();
            with(v, |pr| {
                let from = pr.follow(&existing, "link")?;
                let to = pr.parent_followed(&path, "link")?;
                if fs::symlink_metadata(&to).is_ok() {
                    return Err(err("EEXIST", "file already exists", "link", &path));
                }
                if from.is_dir() {
                    return Err(err("EPERM", "operation not permitted", "link", &existing));
                }
                pr.touch(&to)?;
                fs::hard_link(&from, &to).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
        })),
        mk("fs.readlink", "{ id: number; path: string }", "string", "a link's target (a project path when it points inside)", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.parent_followed_read(&path, "readlink")?;
                let t = fs::read_link(&r).map_err(|_| err("EINVAL", "invalid argument", "readlink", &path))?;
                Ok(json!(if t.is_absolute() { pr.virt(&t).unwrap_or_else(|| "(outside the project)".into()) } else { t.display().to_string() }))
            })
        })),
        mk("fs.realpath", "{ id: number; path: string }", "string", "a path with every link followed", Arc::new(move |v| {
            let path = p(v)?;
            with(v, |pr| {
                let r = pr.follow_read(&path, "realpath")?;
                Ok(json!(pr.virt(&r).unwrap_or_else(|| clean(&path))))
            })
        })),
        mk("fs.utimes", "{ id: number; path: string; mtimeMs: number }", "null", "a path's modification time set", Arc::new(move |v| {
            let path = p(v)?;
            let ms = v.get("mtimeMs").and_then(Value::as_f64).ok_or("mtimeMs")?;
            with(v, |pr| {
                let r = pr.follow(&path, "utime")?;
                let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64((ms / 1e3).max(0.0));
                fs::File::options().write(r.is_file()).read(true).open(&r).and_then(|f| f.set_modified(t)).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            })
        })),
        mk("fs.paths", "{ id: number; limit?: number }", "string[]", "every path in the project (to a limit)", Arc::new(|v| {
            let limit = v.get("limit").and_then(Value::as_u64).unwrap_or(100_000) as usize;
            with(v, |pr| {
                let mut out = vec!["/".to_string()];
                let mut stack = vec![pr.root.clone()];
                while let Some(d) = stack.pop() {
                    for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                        if d == pr.root && e.file_name() == META {
                            continue;
                        }
                        let p = e.path();
                        if let Some(v) = pr.virt(&p) {
                            out.push(v);
                        }
                        if out.len() >= limit {
                            return Ok(json!(out));
                        }
                        if e.file_type().is_ok_and(|t| t.is_dir()) {
                            stack.push(p);
                        }
                    }
                }
                out.sort();
                Ok(json!(out))
            })
        })),
    ]
}


fn undo_redo(v: &Value, undo: bool) -> R<Value> {
    let id = v.get("id").and_then(Value::as_u64).ok_or("id")?;
    let mut g = projects().lock().unwrap();
    let pr = g.1.get_mut(&id).ok_or("no such project")?;
    if pr.group.is_some() {
        pr.end(-1, Some("interrupted by undo"), "cmd", None)?;
    }
    let target = if undo { pr.done.last().copied() } else { pr.redo.last().copied() };
    let Some(t) = target else { return Ok(Value::Null) };
    let row = pr.rows().into_iter().find(|r| r["seq"].as_u64() == Some(t)).ok_or("that command's row is gone")?;
    pr.begin(&format!("{} {}", if undo { "undo" } else { "redo" }, row["cmd"].as_str().unwrap_or("")));
    let r = pr.apply(&row, if undo { "before" } else { "after" });
    let out = pr.end(if r.is_ok() { 0 } else { 1 }, r.as_ref().err().map(String::as_str), if undo { "undo" } else { "redo" }, Some(t))?;
    r.map(|_| out)
}
