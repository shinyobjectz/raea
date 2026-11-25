use super::*;

/// A headless script bundled for the engine: imported where it lives (its own node_modules resolve), with the engine's
/// resolution on top (raea-bundle's script.rs: the SDK from its source, the test API, raea-runtime-react's node_modules as the
/// fallback); its default export, if a function, is called and awaited, then the run ends.
pub fn bundle_script(ts: &Path, out: &Path) -> R<()> {
    bundle_module(ts, out, SCRIPT_ENTRY)
}

/// bundle_script's entry: the answer printed, a throw on stderr with its message (#168)
const SCRIPT_ENTRY: &str = include_str!("script_entry.ts");

/// What a spec's run does (brief.ts, script.ts, taste.ts, a controller: docs/projects.feature PJ0), for bundle_module:
/// it prints the module's default export as one line of JSON and ends, or prints the error and exits 1. The reader is
/// engine/cli spec.rs.
pub const PRINT_DEFAULT: &str = "try { const v = (M as any).default; if (v === undefined) throw new Error(\"it has no default export\"); call(\"print\", JSON.stringify({ text: JSON.stringify(v) })); call(\"done\", JSON.stringify({ code: 0 })); } catch (e: any) { call(\"print\", JSON.stringify({ text: \"error: \" + (e && e.message ? e.message : String(e)) })); call(\"done\", JSON.stringify({ code: 1 })); }";

/// An entry that imports the module as M, with the engine's `call`, then runs `body` (PRINT_DEFAULT for a spec).
pub fn bundle_module(ts: &Path, out: &Path, body: &str) -> R<()> {
    let entry = format!("import * as M from {:?};\nconst call = (globalThis as any).__raea_call;\n{body}\n", spec(ts));
    bundle_entry(&entry, out, &[])
}

/// A path as an entry's import specifier. On Windows a canonical path is a verbatim one (\\?\C:\...), which Rolldown
/// can't resolve as an import (`raea run` on windows-latest, PT20); dunce drops the prefix where the plain path means the
/// same file, and changes nothing elsewhere.
fn spec(p: &Path) -> String {
    dunce::simplified(p).display().to_string()
}

/// `entry` (TypeScript source) bundled into `out` by raea-bundle's library (engine/bundle/src/script.rs's resolution); `extra`
/// are bundle.ts's flags (--define K=V, --alias SPEC=PATH, --node-stub a,b).
pub fn bundle_entry(entry: &str, out: &Path, extra: &[&str]) -> R<()> {
    let out = &std::path::absolute(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let work = std::env::temp_dir().join(format!("raea-cli-e{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    std::fs::write(work.join("entry.ts"), entry).map_err(|e| e.to_string())?;
    let js = raea_io_home::home().join("sdk/react/js");
    std::fs::copy(js.join("tsconfig.json"), work.join("tsconfig.json")).map_err(|e| e.to_string())?;
    // in process by raea-bundle's library: the engine's resolution (engine/bundle/src/script.rs) and the flags
    {
        let (mut defines, mut aliases, mut stubs) = (serde_json::Map::new(), serde_json::Map::new(), Vec::new());
        let mut it = extra.iter();
        while let Some(flag) = it.next() {
            let Some(v) = it.next() else { break };
            match *flag {
                "--define" => {
                    if let Some((k, x)) = v.split_once('=') {
                        defines.insert(k.into(), x.into());
                    }
                }
                "--alias" => {
                    if let Some((k, p)) = v.split_once('=') {
                        aliases.insert(k.into(), std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p)).display().to_string().into());
                    }
                }
                "--node-stub" => stubs.extend(v.split(',').filter(|s| !s.is_empty()).map(|s| Value::from(s))),
                _ => {}
            }
        }
        let root = raea_io_home::home();
        let opts = json!({
            "script": {"js": js, "motion": root.join("sdk/raea/src"), "shell": root.join("sdk/shell/src/index.ts"),
                "aliases": aliases, "stubs": stubs},
            "defines": defines,
        });
        let r = inproc::bundle(&work.join("entry.ts"), opts);
        let _ = std::fs::remove_dir_all(&work);
        let (code, _css) = r.map_err(|e| format!("couldn't bundle it: {}", e.lines().filter(|l| !l.trim().is_empty()).take(8).collect::<Vec<_>>().join(" | ")))?;
        std::fs::write(out, code).map_err(|e| format!("{}: {e}", out.display()))
    }
}

/// Test files (bun:test style) bundled with the engine's test API and run on its JavaScriptCore: each case a PASS or FAIL line,
/// a summary, and the exit code 1 when any failed. `filter` keeps cases whose full name contains it.
pub fn bundle_tests(files: &[PathBuf], out: &Path, filter: Option<&str>, extra: &[&str]) -> R<()> {
    let mut entry = String::new();
    for f in files {
        entry += &format!("import {:?};\n", spec(f));
    }
    entry += &format!(
        "import {{ run }} from \"bun:test\";\nconst call = (globalThis as any).__raea_call;\nconst print = (text: string) => call(\"print\", JSON.stringify({{ text }}));\nrun(print, {}).then((r) => call(\"done\", JSON.stringify({{ code: r.fail ? 1 : 0 }})), (e: any) => {{ print(\"error: \" + (e && e.stack ? e.stack : String(e))); call(\"done\", JSON.stringify({{ code: 1 }})); }});\n",
        filter.map(|f| format!("{f:?}")).unwrap_or("undefined".into())
    );
    bundle_entry(&entry, out, extra)
}

/// raea-bundle's library, loaded at run time (runtime/bundle/lib: raea-runtime-bundle-lib)
pub use raea_runtime_bundle_lib as inproc;

/// Whether a composition's elements are stamped with their place in the code (engine/bundle stamp.rs): the app's editor
/// sets RAEA_SOURCE_STAMPS=1 on the renders it asks for, so its render manifest says which JSX drew what
pub fn stamps() -> bool {
    raea_io_home::env_on("RAEA_SOURCE_STAMPS")
}

pub fn bundle_fresh(tsx: &Path, out_js: &Path, o: &CompOpts, portable: Option<&Path>) -> R<()> {
    let t0 = std::time::Instant::now();
    let dir = tsx.parent().unwrap().to_path_buf();
    // one at a time: the work folder is the process's (void's stage re-bakes on a worker while another object opens)
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let work = std::env::temp_dir().join(format!("raea-cli-b{}", std::process::id()));
    remove_work(&work);
    copy_dir(&dir, &work.join("src"), 0)?;
    drop_tsconfigs(&work.join("src"), 0);
    let motion_src = dunce::canonicalize(&raea_io_home::home().join("sdk/raea/src")).map_err(|e| format!("raea/motion/src: {e}"))?;
    copy_dir(&motion_src, &work.join("motion"), 0)?;
    let sdks = sdks();
    for rel in sources(&work.join("motion"))? {
        let p = work.join("motion").join(&rel);
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let pointed = point_sdks(&text, &"../".repeat(rel.components().count()), &sdks);
        if pointed != text {
            std::fs::write(&p, pointed).map_err(|e| e.to_string())?;
        }
    }
    for (_, d) in &sdks {
        let from = raea_io_home::home().join(d);
        let to = work.join(d.rsplit('/').nth(1).unwrap_or(d));
        copy_dir(&from, &to, 0)?;
        // a file in a folder of its package (animate/timelines.ts) is a level further from motion/ and the others
        for rel in sources(&to)? {
            let p = to.join(&rel);
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let up = "../".repeat(rel.components().count());
            let rewritten = rewrite_into(&text, &from.join(rel.parent().unwrap_or(Path::new(""))), &motion_src, &format!("{up}motion"), portable)?;
            if rewritten != text {
                std::fs::write(&p, rewritten).map_err(|e| e.to_string())?;
            }
        }
    }
    // what the edit imports from outside its folder (an example of examples/), at ext/ under the work folder, its
    // imports pointed at the copies and its SDK's at motion/ as the folder's own files' are (outside.rs)
    let outs = outside(&dir)?;
    for o in &outs {
        let to = work.join(&o.copy);
        std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        let from_dir = o.file.parent().unwrap();
        let at = o.copy.parent().unwrap();
        let text = std::fs::read_to_string(&o.file).map_err(|e| format!("{}: {e}", o.file.display()))?;
        let to_motion = format!("{}motion", "../".repeat(at.components().count()));
        let rewritten = rewrite_into(&point_outside(&text, from_dir, at, &dir, &outs), from_dir, &motion_src, &to_motion, portable).map_err(|e| format!("{}: {e}", o.file.display()))?;
        std::fs::write(&to, rewritten).map_err(|e| e.to_string())?;
    }
    let file = tsx.file_name().unwrap().to_string_lossy().to_string();
    let src = point_outside(&std::fs::read_to_string(tsx).map_err(|e| e.to_string())?, &dir, Path::new("src"), &dir, &outs);
    std::fs::write(work.join("src").join(&file), rewrite_into(&src, &dir, &motion_src, "../motion", portable)?).map_err(|e| e.to_string())?;
    // every other source in the edit's folder too (components/captions.tsx importing the SDK): one copy of the SDK,
    // so a component's hooks read the Clock the engine drives
    for rel in sources(&work.join("src"))? {
        let p = work.join("src").join(&rel);
        if p == work.join("src").join(&file) {
            continue;
        }
        let to_motion = format!("{}motion", "../".repeat(rel.components().count()));
        let from_dir = dir.join(rel.parent().unwrap_or(Path::new("")));
        // compared with the file as it was: one whose only change is an outside import pointed at its copy (a helper
        // importing ../floors/generate and nothing of the SDK) is written too
        let was = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        let text = point_outside(&was, &from_dir, &Path::new("src").join(rel.parent().unwrap_or(Path::new(""))), &dir, &outs);
        let rewritten = rewrite_into(&text, &from_dir, &motion_src, &to_motion, portable).map_err(|e| format!("{}: {e}", rel.display()))?;
        if rewritten != was {
            std::fs::write(&p, rewritten).map_err(|e| e.to_string())?;
        }
    }
    let js = raea_io_home::home().join("sdk/react/js");
    link_node_modules(&js, &work)?;
    // the command line's props, where getInputProps() and a Composition read them (the entry imports it first)
    let global = |v: Option<String>| v.unwrap_or("undefined".into());
    std::fs::write(work.join("props.ts"), format!("(globalThis as any).__raea_input_props = {};\n(globalThis as any).__raea_scale = {};\n(globalThis as any).__raea_hold = {};\n", global(o.props.as_ref().map(Value::to_string)), global(o.scale.map(|s| s.to_string())), global(o.hold.map(|(n, m)| format!("[{n}, {m}]"))))).map_err(|e| e.to_string())?;
    if o.lite {
        // the game's code and the SDK's resolve "react" to the live view runtime (tsconfig paths cover the files under
        // the work folder; packages in node_modules keep their own React)
        copy_dir(&js.join("lite"), &work.join("lite"), 0)?;
        let bridge = js.join("node_modules/bevy-react/src/bridge.ts");
        let index = std::fs::read_to_string(work.join("lite/index.ts")).map_err(|e| e.to_string())?.replace("\"../node_modules/bevy-react/src/bridge\"", &format!("{:?}", bridge.display().to_string()));
        std::fs::write(work.join("lite/index.ts"), index).map_err(|e| e.to_string())?;
        std::fs::write(work.join("tsconfig.json"), r#"{ "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "react", "strict": false, "module": "esnext", "target": "es2022", "moduleResolution": "bundler", "baseUrl": ".", "paths": { "react": ["./lite/index.ts"], "react/jsx-runtime": ["./lite/jsx-runtime.ts"], "react/jsx-dev-runtime": ["./lite/jsx-runtime.ts"] } } }"#).map_err(|e| e.to_string())?;
        std::fs::write(work.join("entry.tsx"), entry_lite(&file, o)).map_err(|e| e.to_string())?;
    } else {
        std::fs::copy(js.join("tsconfig.json"), work.join("tsconfig.json")).map_err(|e| e.to_string())?;
        std::fs::write(work.join("entry.tsx"), entry(&file, o)).map_err(|e| e.to_string())?;
    }
    let t_work = t0.elapsed();
    // in process by raea-bundle's library (Rolldown; a live bundle with its Z3 compile in Rust)
    let (js_text, css) = inproc::bundle(&work.join("entry.tsx"), serde_json::json!({"z3": o.lite, "edit": {"dir": dir, "js": js}, "stamps": stamps()})).map_err(|e| format!("couldn't bundle {file}: {}", e.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join(" | ")))?;
    // its CSS rides in the bundle's last line (bundle_css), so every copy and cache of it carries it
    let code = if css.is_empty() { js_text } else { format!("{js_text}\n//# raea-css={}\n", serde_json::to_string(&css).unwrap_or_default()) };
    if raea_io_trace::stat("RAEA_BUNDLE_TRACE") {
        eprintln!("raea bundle: {} in {:.1} ms: the work folder {:.1} ms, Rolldown {:.1} ms", tsx.display(), t0.elapsed().as_secs_f64() * 1e3, t_work.as_secs_f64() * 1e3, (t0.elapsed() - t_work).as_secs_f64() * 1e3);
    }
    std::fs::write(out_js, code).map_err(|e| format!("{}: {e}", out_js.display()))?;
    remove_work(&work);
    Ok(())
}

/// The work folder's node_modules: the engine's (sdk/react/js's), linked. On Windows a junction, which needs none of the
/// rights a symlink does (Developer Mode or an admin), as raea-io-npm links packages. Windows had no link at all, so the
/// live runtime's `../../node_modules/bevy-react/src/bridge` resolved nothing and `raea play` couldn't bundle a game.
fn link_node_modules(js: &Path, work: &Path) -> R<()> {
    let (to, at) = (js.join("node_modules"), work.join("node_modules"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&to, &at).map_err(|e| format!("node_modules: {e}"))?;
    #[cfg(windows)]
    junction::create(&to, &at).map_err(|e| format!("node_modules: {e}"))?;
    Ok(())
}

/// The work folder removed: on Windows its node_modules junction first, by remove_dir, which takes the link and never
/// the engine's packages it points at
fn remove_work(work: &Path) {
    #[cfg(windows)]
    let _ = std::fs::remove_dir(work.join("node_modules"));
    let _ = std::fs::remove_dir_all(work);
}

#[cfg(test)]
mod link_tests {
    #[test]
    fn the_work_folder_reads_the_engines_packages_and_its_removal_keeps_them() {
        let base = std::env::temp_dir().join(format!("raea-link-test-{}", std::process::id()));
        let (js, work) = (base.join("js"), base.join("work"));
        std::fs::create_dir_all(js.join("node_modules/bevy-react/src")).unwrap();
        std::fs::write(js.join("node_modules/bevy-react/src/bridge.ts"), "export {};").unwrap();
        std::fs::create_dir_all(work.join("lite/parts")).unwrap();
        super::link_node_modules(&js, &work).unwrap();
        // what lite/parts/root.ts imports, read through the link
        assert_eq!(std::fs::read_to_string(work.join("lite/parts/../../node_modules/bevy-react/src/bridge.ts")).unwrap(), "export {};");
        super::remove_work(&work);
        assert!(!work.exists(), "the work folder is gone");
        assert!(js.join("node_modules/bevy-react/src/bridge.ts").exists(), "the engine's packages are kept");
        let _ = std::fs::remove_dir_all(&base);
    }
}
