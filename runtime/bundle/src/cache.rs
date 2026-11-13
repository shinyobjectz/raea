use super::*;

/// Where a whole-video export of `tsx` is kept (lightspeed T1): named by everything it's made from. That's the bundle's
/// bytes; this binary; the arguments but -o; every RAEA_ setting that isn't a timing or a trace; and each media file
/// the edit can read, by path, size and modified time (every file under its folder, links followed, and every absolute
/// media path the bundle names). The output's own extension goes in too, and paths as they really are; where it and a
/// --manifest go, a generation session's token and ledger, playback's mute, and the app's own RAEA_FEED_ settings don't. With --manifest the manifest is kept beside it and comes back with it. None for anything but
/// an .mp4 or .mov, when
/// RAEA_AUDIO_BOUNCE asks for a side file, with RAEA_FRAME_LOG, or with RAEA_EXPORT_CACHE=0; RAEA_EXPORT_CACHE=DIR moves it from
/// ~/Library/Caches/raea/exports. A hit is the earlier export's file, so it's the same bytes.
pub fn export_cache(tsx: &Path, js: &Path, out: &Path, args: &[String]) -> Option<PathBuf> {
    let ext = out.extension()?.to_str()?.to_ascii_lowercase();
    // RAEA_FRAME_LOG measures a render, so it renders (a hit would hand back the earlier file and write no log)
    // RAEA_HIDE_MEDIA and RAEA_HIDE_LAYERS draw a page the bundle doesn't say (CSS the render adds): never the cache's
    let hides = std::env::var_os("RAEA_HIDE_MEDIA").is_some() || std::env::var_os("RAEA_HIDE_LAYERS").is_some();
    if !matches!(ext.as_str(), "mp4" | "mov") || hides || std::env::var_os("RAEA_AUDIO_BOUNCE").is_some() || std::env::var_os("RAEA_FRAME_LOG").is_some() {
        return None;
    }
    let root = match std::env::var("RAEA_EXPORT_CACHE") {
        Ok(v) if v == "0" => return None,
        Ok(v) => PathBuf::from(v),
        Err(_) => raea_io_home::caches()?.join("exports"),
    };
    let bundle = std::fs::read(js).ok()?;
    let mut key: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for x in b {
            key = (key ^ *x as u64).wrapping_mul(0x100_0000_01b3);
        }
        key = (key ^ 0xff).wrapping_mul(0x100_0000_01b3);
    };
    eat(&bundle);
    eat(ext.as_bytes());
    // this build (its binary's modification time and size): another engine paints other bytes
    if let Some(m) = std::env::current_exe().ok().and_then(|e| std::fs::canonicalize(e).ok()).and_then(|e| e.metadata().ok()) {
        let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        eat(&t.to_le_bytes());
        eat(&m.len().to_le_bytes());
    }
    let mut skip = false;
    for a in args.iter().skip(1) {
        if std::mem::take(&mut skip) {
            continue;
        }
        // where the video and its manifest go isn't what's in them
        if a == "-o" || a == "--manifest" {
            skip = true;
            continue;
        }
        // a path by where it really is (/var and /private/var are one folder)
        match std::fs::canonicalize(a) {
            Ok(p) => eat(p.as_os_str().as_encoded_bytes()),
            Err(_) => eat(a.as_bytes()),
        }
    }
    let mut env: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k.starts_with("RAEA_") && !k.starts_with("RAEA_FEED_") && !matches!(k.as_str(), "RAEA_TIMES" | "RAEA_STARTUP_TRACE" | "RAEA_BAKE_TRACE" | "RAEA_LIVE_TRACE" | "RAEA_GPU_TIMES" | "RAEA_MASTER_TRACE" | "RAEA_SOUND_TRACE" | "RAEA_GEN_TOKEN" | "RAEA_GEN_LEDGER" | "RAEA_AUDIO_MUTE")).collect();
    env.sort();
    for (k, v) in &env {
        eat(k.as_bytes());
        eat(v.as_bytes());
    }
    let mut files: Vec<PathBuf> = Vec::new();
    fn walk(d: &Path, depth: usize, out: &Path, files: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') || p == out {
                continue;
            }
            if p.is_dir() {
                if depth < 6 {
                    walk(&p, depth + 1, out, files);
                }
            } else {
                // every file, whatever its kind: a page loads what it names (an <img> of an .svg or a .jpeg, a CSS
                // url(), a fetch), and a list of extensions left an edited logo.svg out, so its old picture came back
                // from the cache (#28). A false miss costs a render, a false hit a stale video
                files.push(p);
            }
        }
    }
    let out_abs = std::fs::canonicalize(out).unwrap_or_else(|_| out.to_path_buf());
    walk(tsx.parent()?, 0, &out_abs, &mut files);
    // exports this cache wrote (an earlier render's OUT.mp4 beside the edit) are outputs, not inputs, unless the
    // composition plays them: a cut laid from its shots' renders (anything/cut/intro.tsx over intro/renders/*.mp4)
    // must miss when a shot is rendered again, and kept them out, it came back from the cache unchanged (10-21). A
    // render counts as played when the bundle names it (its name without the extension in the bundle's text, as a path
    // built from a template holds only that: `renders/${name}.mp4`; a name that only looks played costs a render,
    // never a stale export)
    let text = String::from_utf8_lossy(&bundle);
    let made: std::collections::HashSet<PathBuf> = std::fs::read_to_string(root.join("outputs.list")).unwrap_or_default().lines().map(PathBuf::from).collect();
    // (a file is one of them only if its name is one of theirs, or it is a link to one: only those are resolved, where
    // every file under the edit's folder was, thousands of them before a render's first frame)
    let names: std::collections::HashSet<&std::ffi::OsStr> = made.iter().filter_map(|m| m.file_name()).collect();
    let may_be_made = |p: &Path| p.file_name().is_some_and(|n| names.contains(n)) || p.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
    files.retain(|p| !may_be_made(p) || !made.contains(&std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())) || p.file_stem().is_some_and(|n| text.contains(&*n.to_string_lossy())));
    // absolute media paths the bundle names (an import from outside the edit's folder becomes its path)
    for q in text.split('"').skip(1).step_by(2) {
        if q.starts_with('/') && q.len() < 4096 && Path::new(q).extension().and_then(|x| x.to_str()).is_some_and(|x| MEDIA_EXTS.contains(&x.to_ascii_lowercase().as_str())) {
            files.push(PathBuf::from(q));
        }
    }
    files.push(std::env::current_exe().ok().and_then(|e| std::fs::canonicalize(e).ok())?);
    files.sort();
    files.dedup();
    // the optimized copies there are, listed once. Asking each media file for its own (raea_io_media::mezz::has and
    // ranges) read the file's first and last MiB and listed this folder again, for every picture, sound and video
    // under the edit's folder: 492 files in one real project and 2,417 in another, before a render's first frame.
    // Only a video has copies, and with none in the folder no file has one
    let copies: Vec<String> = std::fs::read_dir(raea_io_media::mezz::dir()).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    for p in &files {
        eat(p.as_os_str().as_encoded_bytes());
        match std::fs::metadata(p) {
            Ok(m) => {
                eat(&m.len().to_le_bytes());
                eat(&m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0).to_le_bytes());
            }
            Err(_) => eat(b"missing"),
        }
        // which file its decoders read: a video's optimized copy, once complete, changes the pixels without touching
        // the source (raea_io_media::mezz)
        let video = matches!(p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str(), "mp4" | "mov" | "m4v");
        if video && !copies.is_empty() && let Some(fp) = raea_io_media::mezz::fingerprint(p) {
            if copies.iter().any(|n| n.strip_prefix(fp.as_str()).is_some_and(|rest| rest.starts_with('.') && !rest.contains("partial"))) {
                eat(b"mezz");
            }
            // its range copies too: the frames inside them come from the copies
            if copies.iter().any(|n| n.strip_prefix(fp.as_str()).is_some_and(|rest| rest.starts_with('@'))) {
                for r in raea_io_media::mezz::ranges(p) {
                    eat(format!("mezz@{}-{}", r.first, r.last).as_bytes());
                }
            }
        }
    }
    Some(root.join(format!("{key:016x}.{ext}")))
}

/// Where the bundle of `tsx` with `o` is kept: named by everything the bundle is made from, each file by its path, size
/// and modified time (nanoseconds) as copy_dir would take it (the edit's folder, raea/motion/src, the lite runtime),
/// the JS packages' manifests, bun itself, this binary (the rewriting is its code) and the entry it writes. None when
/// there's nowhere to keep it (RAEA_BUNDLE_CACHE=0 turns it off; RAEA_BUNDLE_CACHE=DIR moves it from
/// ~/Library/Caches/raea/bundles).
pub fn bundle_cache(tsx: &Path, o: &CompOpts) -> Option<PathBuf> {
    let root = match std::env::var("RAEA_BUNDLE_CACHE") {
        Ok(v) if v == "0" => return None,
        Ok(v) => PathBuf::from(v),
        Err(_) => raea_io_home::caches()?.join("bundles"),
    };
    let mut key: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for x in b {
            key = (key ^ *x as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    fn stat(p: &Path) -> Option<(u64, u128)> {
        let m = std::fs::metadata(p).ok()?;
        Some((m.len(), m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos()))
    }
    fn walk(from: &Path, depth: usize, out: &mut Vec<(PathBuf, u64, u128)>) {
        let Ok(rd) = std::fs::read_dir(from) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                if depth < 6 {
                    walk(&p, depth + 1, out);
                }
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "json" | "css")) {
                if let Some((n, t)) = stat(&p) {
                    out.push((p, n, t));
                }
            }
        }
    }
    let file = tsx.file_name()?.to_string_lossy().to_string();
    let js = raea_io_home::home().join("sdk/react/js");
    let mut files = Vec::new();
    walk(tsx.parent()?, 0, &mut files);
    // what it imports from outside its folder (outside.rs): editing an example re-bundles what imports it
    for o in outside(tsx.parent()?).unwrap_or_default() {
        files.extend(stat(&o.file).map(|(n, t)| (o.file, n, t)));
    }
    walk(&raea_io_home::home().join("sdk/raea/src"), 0, &mut files);
    for (_, d) in sdks() {
        walk(&raea_io_home::home().join(d), 0, &mut files);
    }
    if o.lite {
        walk(&js.join("lite"), 0, &mut files);
        files.extend(stat(&js.join("node_modules/bevy-react/src/bridge.ts")).map(|(n, t)| (js.join("node_modules/bevy-react/src/bridge.ts"), n, t)));
    }
    // sdk/tsconfig.base.json: what the SDK's import names mean (rewrite_into); and the edit itself when it is a dot file, which the walk passes over (the app's preview wrapper, .raea-view-NAME.tsx, is rewritten at one path and got the bundle of the last one)
    for p in [js.join("tsconfig.json"), js.join("package.json"), js.join("bun.lock"), raea_io_home::home().join("sdk").join(raea_io_home::sdk::LIST)].into_iter().chain(file.starts_with('.').then(|| tsx.to_path_buf())) {
        files.extend(stat(&p).map(|(n, t)| (p, n, t)));
    }
    files.extend(std::env::current_exe().ok().and_then(|p| stat(&p).map(|(n, t)| (p, n, t))));
    files.sort();
    for (p, n, t) in &files {
        eat(p.as_os_str().as_encoded_bytes());
        eat(&n.to_le_bytes());
        eat(&t.to_le_bytes());
    }
    eat(file.as_bytes());
    eat(if crate::fresh::stamps() { b"stamps" } else { b"plain" });
    eat(if o.lite { entry_lite(&file, o) } else { entry(&file, o) }.as_bytes());
    // the bundler: raea-bundle's library, by its path, size and time
    if let Some((p, n, t)) = inproc::lib().ok().and_then(|l| stat(&l.path).map(|(n, t)| (l.path.clone(), n, t))) {
        eat(p.as_os_str().as_encoded_bytes());
        eat(&n.to_le_bytes());
        eat(&t.to_le_bytes());
    }
    Some(root.join(format!("{key:016x}.js")))
}

/// The newest `keep` files with extension `ext` in `dir` kept, by when they were last written or used.
pub fn prune_cache(dir: &Path, ext: &str, keep: usize) {
    let mut all = vec![];
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let (p, t) = (e.path(), e.metadata().and_then(|m| m.modified()).ok());
        // a write that never finished (its process killed between the copy and the rename) goes after an hour
        if p.extension().is_some_and(|x| x == "tmp") && t.and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() > 3600) {
            let _ = std::fs::remove_file(&p);
        } else if let (Some(t), true) = (t, p.extension().is_some_and(|x| x == ext)) {
            all.push((t, p));
        }
    }
    all.sort_by(|a, b| b.0.cmp(&a.0));
    // the oldest go, with what an export keeps beside it: its message, manifest and findings
    for (_, p) in all.into_iter().skip(keep) {
        for f in ["said", "manifest", "findings"].map(|s| p.with_extension(s)).into_iter().chain([p]) {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// A render's work folder, removed when the render ends (any way it ends but a kill).
pub struct Gone(pub PathBuf);

impl Drop for Gone {
    fn drop(&mut self) {
        // RAEA_KEEP keeps it, to look at the pages
        if std::env::var_os("RAEA_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Render work folders left by a render that was killed: at once when the process its name carries is gone (a 4K
/// render's pictures run to 14 GB; four killed in eight minutes filled the disk, 10-24), else when nothing has touched
/// them for two hours.
pub fn sweep_stale_work() {
    sweep_stale_work_in(&std::env::temp_dir(), std::time::SystemTime::now());
}

/// The sweep of DIR as it would run at NOW (a test passes a later NOW to age every folder)
pub(crate) fn sweep_stale_work_in(dir: &Path, now: std::time::SystemTime) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !["raea-cli-r", "raea-cli-b"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|d| d.as_secs() > 2 * 3600);
        // raea-cli-r{pid}-ThreadId(n), raea-cli-b{pid}: gone only when the system says no such process is left, running
        // when it says it runs (this process's own folders too)
        let pid = name.get(10..).and_then(|s| s.split('-').next()).and_then(|p| p.parse::<u32>().ok());
        let running = pid.map(|p| if p == std::process::id() { Some(true) } else { crate::pid::running(p) });
        let gone = running == Some(Some(false));
        // the age rule is for a folder whose process isn't known to run: a long render stops touching its folder's top
        // level well before it ends (#31), and a killed render's PID handed to another process keeps its folder
        // until that one ends
        if gone || (old && running != Some(Some(true))) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}
