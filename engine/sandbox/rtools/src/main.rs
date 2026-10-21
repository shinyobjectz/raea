//! rtools: the agent's everyday file and text commands in one small WASI module (docs/media.feature M5's lean set),
//! `rtools UTIL ARGS...`: ls cat head tail wc sort uniq cut tr cp mv mkdir rm touch echo grep find sed basename
//! dirname. Written for the sandbox's size budget: std and regex-lite only, no locale tables.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

type R = Result<(), String>;

fn read(p: &str) -> Result<String, String> {
    if p == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s).map_err(|e| format!("-: {e}"))?;
        return Ok(s);
    }
    fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))
}

/// Flags (-x, -n N) and the rest; `with_value` names the flags that take a value.
fn flags<'a>(args: &'a [String], with_value: &[&str]) -> (Vec<(String, Option<&'a str>)>, Vec<&'a str>) {
    let (mut f, mut rest) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a.len() > 1 && a.starts_with('-') && !a.starts_with("--") && a.parse::<f64>().is_err() {
            let body = &a[1..];
            if with_value.contains(&body) {
                f.push((body.to_string(), args.get(i + 1).map(|s| s.as_str())));
                i += 1;
            } else if with_value.iter().any(|v| body.starts_with(v) && body.len() > v.len()) {
                let v = with_value.iter().find(|v| body.starts_with(**v)).unwrap();
                f.push((v.to_string(), Some(&a[1 + v.len()..])));
            } else {
                for c in body.chars() {
                    f.push((c.to_string(), None));
                }
            }
        } else {
            rest.push(a);
        }
        i += 1;
    }
    (f, rest)
}

fn has(f: &[(String, Option<&str>)], k: &str) -> bool {
    f.iter().any(|(n, _)| n == k)
}

fn val<'a>(f: &[(String, Option<&'a str>)], k: &str) -> Option<&'a str> {
    f.iter().rev().find(|(n, _)| n == k).and_then(|(_, v)| *v)
}

fn lines_of(files: &[&str]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for p in if files.is_empty() { &["-"][..] } else { files } {
        out.extend(read(p)?.lines().map(str::to_string));
    }
    Ok(out)
}

fn walk(p: &Path, out: &mut Vec<PathBuf>) {
    out.push(p.to_path_buf());
    if p.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            let mut kids: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            kids.sort();
            for k in kids {
                walk(&k, out);
            }
        }
    }
}

fn glob(pat: &str, name: &str) -> bool {
    let re = format!("^{}$", regex_lite::escape(pat).replace("\\*", ".*").replace("\\?", "."));
    regex_lite::Regex::new(&re).map(|r| r.is_match(name)).unwrap_or(false)
}

fn run(util: &str, args: &[String], out: &mut dyn Write) -> R {
    let w = |out: &mut dyn Write, s: &str| out.write_all(s.as_bytes()).map_err(|e| e.to_string());
    match util {
        "echo" => w(out, &format!("{}\n", args.join(" "))),
        "cat" => {
            for p in if args.is_empty() { vec!["-".to_string()] } else { args.to_vec() } {
                w(out, &read(&p)?)?;
            }
            Ok(())
        }
        "ls" => {
            let (f, rest) = flags(args, &[]);
            let (long, all) = (has(&f, "l"), has(&f, "a"));
            for p in if rest.is_empty() { vec!["."] } else { rest } {
                let md = fs::metadata(p).map_err(|e| format!("{p}: {e}"))?;
                if !md.is_dir() {
                    w(out, &format!("{p}\n"))?;
                    continue;
                }
                let mut es: Vec<(String, fs::Metadata)> = fs::read_dir(p).map_err(|e| format!("{p}: {e}"))?.filter_map(|e| e.ok()).filter_map(|e| Some((e.file_name().to_string_lossy().to_string(), e.metadata().ok()?))).collect();
                es.sort_by(|a, b| a.0.cmp(&b.0));
                for (n, m) in es.into_iter().filter(|(n, _)| all || !n.starts_with('.')) {
                    if long {
                        w(out, &format!("{} {:>10} {n}\n", if m.is_dir() { 'd' } else { '-' }, m.len()))?;
                    } else {
                        w(out, &format!("{n}\n"))?;
                    }
                }
            }
            Ok(())
        }
        "head" | "tail" => {
            let (f, rest) = flags(args, &["n"]);
            let n: usize = val(&f, "n").and_then(|v| v.parse().ok()).unwrap_or(10);
            let ls = lines_of(&rest)?;
            let pick: Vec<&String> = if util == "head" { ls.iter().take(n).collect() } else { ls.iter().skip(ls.len().saturating_sub(n)).collect() };
            for l in pick {
                w(out, &format!("{l}\n"))?;
            }
            Ok(())
        }
        "wc" => {
            let (f, rest) = flags(args, &[]);
            let only = ["l", "w", "c"].iter().filter(|k| has(&f, k)).copied().collect::<Vec<_>>();
            for p in if rest.is_empty() { vec!["-"] } else { rest } {
                let s = read(p)?;
                let (l, wd, c) = (s.lines().count(), s.split_whitespace().count(), s.len());
                let cols: Vec<String> = [("l", l), ("w", wd), ("c", c)].iter().filter(|(k, _)| only.is_empty() || only.contains(k)).map(|(_, v)| v.to_string()).collect();
                w(out, &format!("{} {p}\n", cols.join(" ")))?;
            }
            Ok(())
        }
        "sort" => {
            let (f, rest) = flags(args, &[]);
            let mut ls = lines_of(&rest)?;
            if has(&f, "n") {
                let key = |s: &String| s.split_whitespace().next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                ls.sort_by(|a, b| key(a).total_cmp(&key(b)).then(a.cmp(b)));
            } else {
                ls.sort();
            }
            if has(&f, "r") {
                ls.reverse();
            }
            if has(&f, "u") {
                ls.dedup();
            }
            for l in ls {
                w(out, &format!("{l}\n"))?;
            }
            Ok(())
        }
        "uniq" => {
            let (f, rest) = flags(args, &[]);
            let ls = lines_of(&rest)?;
            let mut i = 0;
            while i < ls.len() {
                let mut j = i;
                while j + 1 < ls.len() && ls[j + 1] == ls[i] {
                    j += 1;
                }
                w(out, &if has(&f, "c") { format!("{:>7} {}\n", j - i + 1, ls[i]) } else { format!("{}\n", ls[i]) })?;
                i = j + 1;
            }
            Ok(())
        }
        "cut" => {
            let (f, rest) = flags(args, &["d", "f"]);
            let d = val(&f, "d").unwrap_or("\t");
            let fields: Vec<usize> = val(&f, "f").unwrap_or("1").split(',').filter_map(|x| x.parse().ok()).collect();
            for l in lines_of(&rest)? {
                let parts: Vec<&str> = l.split(d).collect();
                w(out, &format!("{}\n", fields.iter().filter_map(|i| parts.get(i - 1)).copied().collect::<Vec<_>>().join(d)))?;
            }
            Ok(())
        }
        "tr" => {
            let (a, b) = (args.first().ok_or("tr SET1 SET2")?.chars().collect::<Vec<_>>(), args.get(1).ok_or("tr SET1 SET2")?.chars().collect::<Vec<_>>());
            let s = read("-")?;
            w(out, &s.chars().map(|c| a.iter().position(|x| *x == c).map(|i| *b.get(i).or(b.last()).unwrap_or(&c)).unwrap_or(c)).collect::<String>())
        }
        "cp" | "mv" => {
            let (src, dst) = (args.first().ok_or("cp SRC DST")?, args.get(1).ok_or("cp SRC DST")?);
            let to = if Path::new(dst).is_dir() { Path::new(dst).join(Path::new(src).file_name().unwrap_or_default()) } else { PathBuf::from(dst) };
            if util == "cp" { fs::copy(src, &to).map(|_| ()) } else { fs::rename(src, &to) }.map_err(|e| format!("{src}: {e}"))
        }
        "mkdir" => {
            let (f, rest) = flags(args, &[]);
            for p in rest {
                if has(&f, "p") { fs::create_dir_all(p) } else { fs::create_dir(p) }.map_err(|e| format!("{p}: {e}"))?;
            }
            Ok(())
        }
        "rm" => {
            let (f, rest) = flags(args, &[]);
            for p in rest {
                if Path::new(p).is_dir() && has(&f, "r") { fs::remove_dir_all(p) } else { fs::remove_file(p) }.map_err(|e| format!("{p}: {e}"))?;
            }
            Ok(())
        }
        "touch" => {
            for p in args {
                fs::OpenOptions::new().create(true).append(true).open(p).map_err(|e| format!("{p}: {e}"))?;
            }
            Ok(())
        }
        "basename" => w(out, &format!("{}\n", Path::new(args.first().ok_or("basename PATH")?).file_name().unwrap_or_default().to_string_lossy())),
        "dirname" => w(out, &format!("{}\n", Path::new(args.first().ok_or("dirname PATH")?).parent().map(|p| p.display().to_string()).filter(|s| !s.is_empty()).unwrap_or(".".into()))),
        "grep" => {
            let (f, rest) = flags(args, &["e"]);
            let mut rest = rest.into_iter();
            let pat = val(&f, "e").or_else(|| rest.next()).ok_or("grep PATTERN [PATH...]")?;
            let re = regex_lite::RegexBuilder::new(pat).case_insensitive(has(&f, "i")).build().map_err(|e| e.to_string())?;
            let paths: Vec<&str> = rest.collect();
            let mut files = Vec::new();
            for p in if paths.is_empty() { vec!["-"] } else { paths } {
                if p != "-" && Path::new(p).is_dir() {
                    let mut all = Vec::new();
                    walk(Path::new(p), &mut all);
                    files.extend(all.into_iter().filter(|x| x.is_file()).map(|x| x.display().to_string()));
                } else {
                    files.push(p.to_string());
                }
            }
            let named = files.len() > 1 || has(&f, "r");
            let mut hits = 0;
            for p in &files {
                let Ok(s) = read(p) else { continue };
                let mut n = 0;
                for (i, l) in s.lines().enumerate() {
                    if re.is_match(l) != has(&f, "v") {
                        n += 1;
                        if !has(&f, "c") && !has(&f, "l") {
                            let pre = if named { format!("{p}:") } else { String::new() };
                            w(out, &if has(&f, "n") { format!("{pre}{}:{l}\n", i + 1) } else { format!("{pre}{l}\n") })?;
                        }
                    }
                }
                if has(&f, "c") {
                    w(out, &if named { format!("{p}:{n}\n") } else { format!("{n}\n") })?;
                }
                if has(&f, "l") && n > 0 {
                    w(out, &format!("{p}\n"))?;
                }
                hits += n;
            }
            if hits == 0 {
                std::process::exit(1);
            }
            Ok(())
        }
        "find" => {
            let root = args.first().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).unwrap_or(".");
            let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
            let (name, ty) = (opt("-name"), opt("-type"));
            let mut all = Vec::new();
            walk(Path::new(root), &mut all);
            for p in all {
                let n = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                let kind_ok = match ty.as_deref() { Some("f") => p.is_file(), Some("d") => p.is_dir(), _ => true };
                if kind_ok && name.as_deref().is_none_or(|g| glob(g, &n)) {
                    w(out, &format!("{}\n", p.display()))?;
                }
            }
            Ok(())
        }
        "sed" => {
            // s/RE/REPLACEMENT/[g] on each line (the substitution the agent uses; anything else is refused)
            let (f, rest) = flags(args, &["e"]);
            let mut rest = rest.into_iter();
            let script = val(&f, "e").or_else(|| rest.next()).ok_or("sed 's/RE/REP/g' [FILE]")?;
            let sep = script.chars().nth(1).ok_or("sed: s/RE/REP/")?;
            let parts: Vec<&str> = script[2..].split(sep).collect();
            if !script.starts_with('s') || parts.len() < 2 {
                return Err(format!("sed: only s{sep}RE{sep}REP{sep}[g] is read here"));
            }
            let re = regex_lite::Regex::new(parts[0]).map_err(|e| e.to_string())?;
            let rep = parts[1].replace("\\1", "${1}").replace("\\2", "${2}").replace('&', "${0}");
            let global = parts.get(2).is_some_and(|g| g.contains('g'));
            let files: Vec<&str> = rest.collect();
            for l in lines_of(&files)? {
                let o = if global { re.replace_all(&l, rep.as_str()) } else { re.replace(&l, rep.as_str()) };
                w(out, &format!("{o}\n"))?;
            }
            Ok(())
        }
        u => Err(format!("no command {u}: ls cat head tail wc sort uniq cut tr cp mv mkdir rm touch echo grep find sed basename dirname")),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // `rtools UTIL ARGS` (or called by a util's own name)
    let me = Path::new(a.first().map(|s| s.as_str()).unwrap_or("rtools")).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (util, args) = if me == "rtools" { (a.get(1).cloned().unwrap_or_default(), a.get(2..).unwrap_or(&[]).to_vec()) } else { (me, a[1..].to_vec()) };
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let r = run(&util, &args, &mut out);
    let _ = out.flush();
    if let Err(e) = r {
        eprintln!("{util}: {e}");
        std::process::exit(1);
    }
}
