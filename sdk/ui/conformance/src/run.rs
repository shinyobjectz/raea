//! The run: options, the substrate's launch (own HOME, port, workspace copy, stub agent), each case's steps, results.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::case::{self, Case};
use crate::mcp::{clip, frame_of, Mcp};
use crate::pred::{self, Rect, Snaps};

mod procs;
pub(crate) use procs::*;
mod launch;
pub(crate) use launch::*;
mod driver;
pub(crate) use driver::*;

pub const USAGE: &str = "usage: conformance [--bin PATH] [--substrate NAME] [--env K=V]... [--tier quick|full] [--only GLOB]...
                    [--cases DIR] [--fixture DIR] [--out DIR] [--keep] [--startup SECONDS] [--port-env NAME]
                    [--attach PORT [--workspace DIR]] [--settle-ms MS] [--app DIR] [--no-build] [--list]
  --bin        the substrate's binary (default: the repo's target/release/raea-page-host). raea-page-host runs the
               app's Blitz build (BIN BUNDLE --css --root, at the case's route); any other binary runs as BIN WORKSPACE
  --app        the app raea-page-host runs (default: the repo's apps/app), built with its demo world into
               blitz/out-conformance; --no-build: that build as it is (else blitz/out)
  --substrate  a name for the results (default: current)
  --env        more environment for it (e.g. RAEA_FEED_AGENT=echo), after the suite's own
  --tier       quick: each case at its first window size; full: at every size, and the full-only cases (default quick)
  --only       run only the cases whose name (area/name) matches; * is a wildcard
  --attach     drive a substrate already listening on PORT instead of launching one (no isolation: for debugging)
  --keep       keep each run's folder and the substrate's log, pass or fail
  --settle-ms  how long the tree may take to settle (no stale panel, the same rects two frames running) before the
               start snapshot and before a step's targets are resolved; a case fails past it (default 10000; 0: off)";

pub struct Opts {
    pub bin: PathBuf,
    pub substrate: String,
    pub env: Vec<(String, String)>,
    pub tier: String,
    pub only: Vec<String>,
    pub cases: PathBuf,
    pub fixture: PathBuf,
    pub out: Option<PathBuf>,
    pub keep: bool,
    pub attach: Option<u16>,
    pub workspace: Option<PathBuf>,
    pub list: bool,
    pub startup: u64,
    /// the substrate's MCP port setting (default: RAEA_PAGE_MCP_PORT on raea-page-host, else RAEA_FEED_MCP_PORT)
    pub port_env: Option<String>,
    pub settle_ms: u64,
    pub app: PathBuf,
    pub build: bool,
}

/// raea-page-host, by its file name; any other binary is launched as the legacy app is.
pub fn is_page_host(bin: &Path) -> bool {
    bin.file_name().is_some_and(|n| n == "raea-page-host")
}

/// The route a case opens at on raea-page-host when it names none.
const ROUTE: &str = "/feed/f-ides-all";

/// The page's location before its first render reads it, at the http://localhost/ it's served from (`ui-parity app`).
fn location(route: &str) -> String {
    let at = json!({ "hash": format!("#{route}"), "href": format!("http://localhost/#{route}"), "origin": "http://localhost", "protocol": "http:", "host": "localhost", "hostname": "localhost", "port": "", "pathname": "/", "search": "" });
    format!("globalThis.location ??= {at};")
}

/// The app's Blitz build with the demo world the cases are written against (RAEA_FIXTURES=1: since 11-14 a plain build
/// has none, as a shipped app has none), in blitz/out-conformance.
fn build_app(app: &Path) -> Result<(), String> {
    let mut vite = Command::new("npx");
    vite.args(["vite", "build", "-c", "blitz/vite.config.ts", "--logLevel", "error", "--outDir"]).arg(app.join(CONFORMANCE_OUT));
    let o = vite.env("RAEA_FIXTURES", "1").current_dir(app).output().map_err(|e| format!("vite: {e}"))?;
    if !o.status.success() {
        return Err(format!("{}: vite build -c blitz/vite.config.ts: {}", app.display(), String::from_utf8_lossy(&o.stderr).lines().rev().take(8).collect::<Vec<_>>().join(" | ")));
    }
    Ok(())
}

impl Opts {
    pub fn parse(args: &[String]) -> Result<Opts, String> {
        let here = crate::here();
        let repo = dunce::canonicalize(&here.join("../../..")).unwrap_or(here.join("../../.."));
        let mut o = Opts {
            bin: repo.join("target/release/raea-page-host"),
            substrate: "current".into(),
            env: vec![],
            tier: "quick".into(),
            only: vec![],
            cases: here.join("cases"),
            fixture: here.join("fixture"),
            out: None,
            keep: false,
            attach: None,
            workspace: None,
            list: false,
            startup: 180,
            port_env: None,
            settle_ms: 10_000,
            app: repo.join("apps/app"),
            build: true,
        };
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
            match a.as_str() {
                "--bin" => o.bin = PathBuf::from(val()?),
                "--substrate" => o.substrate = val()?,
                "--env" => {
                    let v = val()?;
                    let (k, x) = v.split_once('=').ok_or(format!("--env {v}: wants K=V"))?;
                    o.env.push((k.into(), x.into()));
                }
                "--tier" => {
                    o.tier = val()?;
                    if o.tier != "quick" && o.tier != "full" {
                        return Err(format!("--tier {}: quick or full", o.tier));
                    }
                }
                "--only" => o.only.push(val()?),
                "--cases" => o.cases = PathBuf::from(val()?),
                "--fixture" => o.fixture = PathBuf::from(val()?),
                "--out" => o.out = Some(PathBuf::from(val()?)),
                "--keep" => o.keep = true,
                "--attach" => o.attach = Some(val()?.parse().map_err(|_| "--attach PORT")?),
                "--workspace" => o.workspace = Some(PathBuf::from(val()?)),
                "--list" => o.list = true,
                "--startup" => o.startup = val()?.parse().map_err(|_| "--startup SECONDS")?,
                "--port-env" => o.port_env = Some(val()?),
                "--app" => o.app = PathBuf::from(val()?),
                "--no-build" => o.build = false,
                "--settle-ms" => o.settle_ms = val()?.parse().map_err(|_| "--settle-ms MS")?,
                "-h" | "--help" => return Err("help".into()),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(o)
    }
}

// --- the substrate's process: stopped on every way out, a signal included

#[derive(Debug, Clone)]
pub struct Outcome {
    pub status: &'static str,
    pub frames: Option<u64>,
    pub budget: u64,
    pub over: u64,
    pub detail: String,
}

fn run_case(o: &Opts, c: &Case, (w, h): (u32, u32), inst: &str, logs: &Path, have_tools: &mut Option<Result<(), String>>) -> (Outcome, Option<PathBuf>) {
    let fail = |d: String| Outcome { status: "fail", frames: None, budget: c.budget, over: 0, detail: d };
    if let Some(Err(e)) = have_tools {
        return (fail(e.clone()), None);
    }
    let mut world = match launch(o, c, inst, logs) {
        Ok(w) => w,
        Err(e) => return (fail(format!("couldn't start: {e}")), None),
    };
    let log = world.log.clone();
    let tools = match up(&mut world, o.startup) {
        Ok(t) => t,
        Err(e) => return (fail(e), log),
    };
    let missing: Vec<&str> = ["input", "ui"].into_iter().filter(|t| !tools.iter().any(|x| x == t)).collect();
    if !missing.is_empty() {
        let e = format!("the substrate has no MCP {} tool (it has: {})", missing.join(" or "), tools.join(", "));
        *have_tools = Some(Err(e.clone()));
        return (fail(e), log);
    }
    *have_tools = Some(Ok(()));
    let ws = world.workspace.clone();
    let mut d = Driver { mcp: &mut world.mcp, case: c, workspace: ws, snaps: Snaps::new(), at: None, timed: vec![], settle_ms: o.settle_ms, after_input: false, notes: vec![], seen: None };
    // the window at the case's size, something drawn in it, and its layout settled at that size (a substrate can report
    // the new window size frames before its panels have laid out at it)
    let ready = (|| -> Result<(), String> {
        d.mcp.input(json!({"type": "resize", "w": w, "h": h}))?;
        let p = json!({"all": [{"check": "window", "w": w, "h": h}, {"id": "*", "exists": true}]});
        d.until(&p, (o.startup * 1000).min(60_000)).map_err(|(e, _)| format!("the window never came up at {w}x{h} with anything in it: {e}"))?;
        let tree = d.settle().map_err(|e| format!("at {w}x{h}: {e}"))?;
        d.snaps.insert("start".into(), tree.clone());
        d.snaps.insert("last".into(), tree);
        d.at = None;
        Ok(())
    })();
    if let Err(e) = ready {
        return (fail(e), log);
    }
    for (i, s) in c.steps.iter().enumerate() {
        if let Err(e) = d.step(s) {
            let notes = if d.notes.is_empty() { String::new() } else { format!(" ({})", d.notes.join("; ")) };
            return (fail(format!("step {}: {e}{notes}", i + 1)), log);
        }
    }
    let frames = d.timed.iter().map(|t| t.frames).max();
    let over = d.timed.iter().map(|t| t.frames.saturating_sub(t.budget)).max().unwrap_or(0);
    let budget = d.timed.iter().map(|t| t.budget).max().unwrap_or(c.budget);
    let mut detail: Vec<String> = vec![];
    if over > 0 {
        detail.push(format!("held {over} frame{} past the budget", if over == 1 { "" } else { "s" }));
    }
    detail.extend(d.notes.iter().cloned());
    let detail = detail.join("; ");
    let o = if over > 0 { Outcome { status: "slow", frames, budget, over, detail } } else { Outcome { status: "pass", frames, budget, over: 0, detail } };
    (o, log)
}

fn percentile(v: &mut [u64], p: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    let k = ((p * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1;
    v[k]
}

pub fn main(o: Opts) -> i32 {
    let cases = match case::all(&o.cases) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("conformance: {e}");
            return 2;
        }
    };
    let chosen: Vec<&Case> = cases
        .iter()
        .filter(|c| o.only.is_empty() || o.only.iter().any(|g| pred::glob(g, &c.name)))
        .filter(|c| o.tier == "full" || !c.full_only)
        .collect();
    let insts: Vec<(&Case, (u32, u32), String)> = chosen
        .iter()
        .flat_map(|c| {
            let ws = if o.tier == "full" { c.windows.clone() } else { c.windows[..1].to_vec() };
            ws.into_iter().map(move |(w, h)| (*c, (w, h), format!("{}@{w}x{h}", c.name)))
        })
        .collect();
    if o.list {
        for (c, _, name) in &insts {
            println!("{name:<44} budget {}  {}", c.budget, c.title);
        }
        return 0;
    }
    if insts.is_empty() {
        eprintln!("conformance: no cases chosen");
        return 2;
    }
    if o.attach.is_none() && !o.bin.is_file() {
        eprintln!("conformance: no substrate at {} (build it: lane build -- cargo build --release -p raea-page-host -j 6)", o.bin.display());
        return 2;
    }
    if o.attach.is_none() && is_page_host(&o.bin) && o.build {
        if let Err(e) = build_app(&o.app) {
            eprintln!("conformance: {e}");
            return 2;
        }
    }
    sweep();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let out = o.out.clone().unwrap_or_else(|| base().join(format!("out-{stamp}")));
    let logs = out.join("logs");
    if let Err(e) = std::fs::create_dir_all(&logs) {
        eprintln!("conformance: {}: {e}", out.display());
        return 2;
    }
    if o.attach.is_none() {
        if let Err(e) = fixture(&o.fixture) {
            eprintln!("conformance: {e}");
            return 2;
        }
    }
    let results = out.join("results.jsonl");
    let mut file = match std::fs::File::create(&results) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("conformance: {}: {e}", results.display());
            return 2;
        }
    };
    println!("conformance: {} cases on {} ({}, {} tier) -> {}", insts.len(), o.substrate, if o.attach.is_some() { "attached".into() } else { o.bin.display().to_string() }, o.tier, out.display());
    let (mut n_pass, mut n_fail, mut n_slow) = (0, 0, 0);
    let mut per_area: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut have_tools = None;
    for (c, size, inst) in &insts {
        let t = Instant::now();
        let (r, log) = run_case(&o, c, *size, inst, &logs, &mut have_tools);
        let secs = t.elapsed().as_secs_f64();
        match r.status {
            "pass" => n_pass += 1,
            "slow" => n_slow += 1,
            _ => n_fail += 1,
        }
        if let Some(f) = r.frames {
            if r.status != "fail" {
                per_area.entry(c.area.clone()).or_default().push(f);
            }
        }
        let frames = r.frames.map(|f| format!("{f} frame{} (budget {})", if f == 1 { "" } else { "s" }, r.budget)).unwrap_or_default();
        let word = match r.status { "pass" => "pass", "slow" => "slow", _ => "FAIL" };
        println!("  {word}  {inst:<44} {frames}{}", if r.detail.is_empty() { String::new() } else { format!("  {}", r.detail) });
        let keep_log = o.keep || r.status == "fail";
        if let Some(l) = &log {
            if !keep_log {
                let _ = std::fs::remove_file(l);
            }
        }
        let line = json!({
            "case": c.name, "area": c.area, "window": [size.0, size.1], "status": r.status, "frames": r.frames,
            "budget": r.budget, "over": r.over, "detail": r.detail, "substrate": o.substrate, "tier": o.tier,
            "seconds": (secs * 10.0).round() / 10.0, "log": if keep_log { log.map(|l| l.display().to_string()) } else { None },
        });
        let _ = writeln!(file, "{line}");
    }
    let total = insts.len();
    println!("gate: conformance {n_pass} pass, {n_fail} fail, {n_slow} slow of {total} on {} ({} tier)", o.substrate, o.tier);
    let mut areas = serde_json::Map::new();
    for (a, v) in per_area.iter_mut() {
        let (p50, p95) = (percentile(v, 0.5), percentile(v, 0.95));
        println!("frames: {a} p50 {p50} p95 {p95} ({} cases)", v.len());
        areas.insert(a.clone(), json!({"p50": p50, "p95": p95, "cases": v.len()}));
    }
    let summary = json!({"substrate": o.substrate, "tier": o.tier, "pass": n_pass, "fail": n_fail, "slow": n_slow, "cases": total, "areas": areas, "results": results.display().to_string()});
    let _ = std::fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap());
    if let Some(Err(e)) = &have_tools {
        println!("note: {e}");
    }
    if n_fail > 0 { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&mut [1, 2, 3, 4], 0.5), 2);
        assert_eq!(percentile(&mut [5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9], 0.95), 5);
        assert_eq!(percentile(&mut [], 0.5), 0);
    }
}
