//! A case's world launched and waited up.

use super::*;

pub(crate) fn launch(o: &Opts, c: &Case, inst: &str, logs: &Path) -> Result<World, String> {
    if let Some(port) = o.attach {
        let ws = o.workspace.clone().unwrap_or_else(|| base().join("attached-workspace"));
        return Ok(World { proc: None, host: None, dir: None, workspace: ws, log: None, mcp: Mcp::new(port) });
    }
    let fx = fixture(&o.fixture)?;
    let dir = base().join("runs").join(std::process::id().to_string()).join(inst.replace(['/', '@'], "-"));
    let _ = std::fs::remove_dir_all(&dir);
    let (home, ws, tmp) = (dir.join("home"), dir.join("workspace"), dir.join("tmp"));
    for d in [&home.join(".raea"), &ws, &tmp] {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    // what the world starts with: a copy of the fixture (times kept), then dated fresh
    // clones where the filesystem can: macOS's cp -c (APFS), GNU cp's --reflink=auto (which has no -c)
    let clone: &[&str] = if cfg!(target_vendor = "apple") { &["-cRp"] } else { &["-Rp", "--reflink=auto"] };
    let cp = Command::new("cp").args(clone).arg(format!("{}/.", fx.display())).arg(&ws).status().map_err(|e| e.to_string())?;
    if !cp.success() {
        return Err("copying the fixture failed".into());
    }
    let _ = std::fs::remove_file(ws.join(".done"));
    // the fixture's .home/ is the world's home (the play items' games, which a feed item names by ~/ path)
    if ws.join(".home").is_dir() {
        let _ = std::fs::remove_dir_all(&home);
        std::fs::rename(ws.join(".home"), &home).map_err(|e| format!("the fixture's .home: {e}"))?;
        std::fs::create_dir_all(home.join(".raea")).map_err(|e| e.to_string())?;
    }
    date_files(&ws, &o.fixture.join("times.txt"));
    // an empty settings file: no key from ~/.raea/env reaches this world
    std::fs::write(home.join(".raea/env"), "").map_err(|e| e.to_string())?;

    let port = free_port()?;
    let real_home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{}:{real_home}/.bun/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin", std::env::var("PATH").unwrap_or_default());
    let page = is_page_host(&o.bin);
    let mut cmd = Command::new(&o.bin);
    if page {
        // the app's Blitz build at the case's route; its own-origin fetches from blitz/out, then public/
        let out = Some(o.app.join(CONFORMANCE_OUT)).filter(|d| d.join("app.js").is_file()).unwrap_or(o.app.join("blitz/out"));
        let js = std::fs::read_to_string(out.join("app.js")).map_err(|e| format!("{}: {e}", out.join("app.js").display()))?;
        let bundle = dir.join("app.js");
        std::fs::write(&bundle, format!("{}\n{js}", location(c.route.as_deref().unwrap_or(ROUTE)))).map_err(|e| e.to_string())?;
        let roots = std::env::join_paths([out.clone(), o.app.join("public")]).map_err(|e| e.to_string())?;
        cmd.arg(&bundle).arg("--css").arg(out.join("app.css")).arg("--root").arg(roots);
    } else {
        cmd.arg(&ws);
    }
    cmd.current_dir(&ws).env_clear().process_group(0);
    let port_env = o.port_env.clone().unwrap_or_else(|| if page { "RAEA_PAGE_MCP_PORT" } else { "RAEA_FEED_MCP_PORT" }.into());
    let mut env: Vec<(String, String)> = vec![
        ("PATH".into(), path),
        ("HOME".into(), home.display().to_string()),
        ("TMPDIR".into(), format!("{}/", tmp.display())),
        ("RAEA_FEED_HOME".into(), home.join(".raea-feed").display().to_string()),
        (port_env, port.to_string()),
        ("RAEA_FEED_HEADLESS".into(), "1".into()),
        ("RAEA_FEED_BACKGROUND".into(), "1".into()),
        ("RAEA_AUDIO_MUTE".into(), "1".into()),
        ("RAEA_FEED_NO_MIC".into(), "1".into()),
        ("RAEA_FEED_AHEAD".into(), "0".into()),
        // the chat's stub: echoes, never a model (conformance.feature, "A run's world is its own")
        ("RAEA_FEED_AGENT".into(), "echo".into()),
        // and should a substrate not know the stub yet: every model endpoint points at a closed local port
        ("RAEA_ENV_FILE".into(), home.join(".raea/env").display().to_string()),
        ("RAEA_LLM_PROVIDER".into(), "custom".into()),
        ("RAEA_LLM_BASE_URL".into(), "http://127.0.0.1:9".into()),
        ("RAEA_LLM_API_KEY".into(), "conformance-stub".into()),
        ("RAEA_LLM_MODEL".into(), "conformance-stub".into()),
        ("RAEA_API".into(), "http://127.0.0.1:9".into()),
    ];
    // the display a Linux host opens its window on (an X server such as Xvfb, or Wayland's socket); unset on a Mac
    for k in ["USER", "LOGNAME", "LANG", "LC_ALL", "SHELL", "DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"] {
        if let Ok(v) = std::env::var(k) {
            env.push((k.into(), v));
        }
    }
    // local models (voice, embeddings) and bun's package cache are read from the real home, never written
    let models = std::env::var("RAEA_MODELS").unwrap_or(format!("{real_home}/.local/share/raea-models"));
    if Path::new(&models).is_dir() {
        env.push(("RAEA_MODELS".into(), models));
    }
    env.push(("BUN_INSTALL_CACHE_DIR".into(), format!("{real_home}/.bun/install/cache")));
    if let Some(s) = &c.space {
        env.push(("RAEA_FEED_SPACE".into(), s.clone()));
    }
    env.extend(c.env.iter().cloned());
    env.extend(o.env.iter().cloned());
    std::fs::create_dir_all(logs).map_err(|e| e.to_string())?;
    let log = logs.join(format!("{}.log", inst.replace(['/', '@'], "-")));
    // the page's /host/* is the feed's host's, started first as raea.app does (without it, "raea is starting" for ever)
    let host = if page { Some(feed_host(o, &env, &log.with_extension("host.log"))?) } else { None };
    if let Some((_, hp)) = &host {
        env.push(("RAEA_PAGE_HOST".into(), format!("http://127.0.0.1:{hp}")));
    }
    for (k, v) in &env {
        cmd.env(k, v);
    }
    let f = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    cmd.stdout(Stdio::from(f.try_clone().map_err(|e| e.to_string())?)).stderr(Stdio::from(f)).stdin(Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("launching {}: {e}", o.bin.display()))?;
    PGID.store(child.id() as i32, Ordering::SeqCst);
    Ok(World { proc: Some(Proc { child, group: &PGID }), host: host.map(|h| h.0), dir: Some(dir), workspace: ws, log: Some(log), mcp: Mcp::new(port) })
}

/// Where build_app puts the app's Blitz build with its demo world (else an older app's blitz/out is run).
pub(crate) const CONFORMANCE_OUT: &str = "blitz/out-conformance";

/// The feed's host (the app's host/node.ts) on a free port in the world's environment, once it takes connections.
fn feed_host(o: &Opts, env: &[(String, String)], log: &Path) -> Result<(Proc, u16), String> {
    let (hp, f) = (free_port()?, std::fs::File::create(log).map_err(|e| e.to_string())?);
    let mut cmd = Command::new("node");
    cmd.args(["host/node.ts", "--port", &hp.to_string()]).current_dir(&o.app).env_clear().envs(env.iter().cloned()).process_group(0);
    cmd.stdout(Stdio::from(f.try_clone().map_err(|e| e.to_string())?)).stderr(Stdio::from(f)).stdin(Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("the feed's host (node host/node.ts in {}): {e}", o.app.display()))?;
    HOST_PGID.store(child.id() as i32, Ordering::SeqCst);
    let mut p = Proc { child, group: &HOST_PGID };
    let t = Instant::now();
    while std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], hp).into(), Duration::from_millis(200)).is_err() {
        match p.child.try_wait() {
            Ok(Some(st)) => return Err(format!("the feed's host exited ({st}) before it listened: {}", log.display())),
            _ if t.elapsed() > Duration::from_secs(30) => return Err(format!("the feed's host never listened on :{hp} in 30 s: {}", log.display())),
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    Ok((p, hp))
}

/// Until the substrate's MCP answers (or it dies, or `secs` pass); then its tools.
pub(crate) fn up(w: &mut World, secs: u64) -> Result<Vec<String>, String> {
    let t = Instant::now();
    loop {
        if let Some(p) = &mut w.proc {
            if let Ok(Some(st)) = p.child.try_wait() {
                return Err(format!("the substrate exited ({st}) before its MCP answered"));
            }
        }
        match w.mcp.tools() {
            Ok(t) => return Ok(t),
            Err(e) if t.elapsed() > Duration::from_secs(secs) => return Err(format!("its MCP never answered in {secs} s: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

// --- the steps
