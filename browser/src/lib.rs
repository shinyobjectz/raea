//! raea's browser: agent-browser (vercel-labs, Apache-2.0, vendored as a library: vendor/agent-browser/RAEA-CHANGES)
//! run in this process. `browser.run {args}` takes the same words as the `agent-browser` command line (open URL,
//! snapshot -i, click @e2, fill @e3 TEXT, get text @e1, screenshot PATH, eval JS, diff ..., batch ...), executes them on
//! one daemon state kept for the process (its browser stays open between calls, as the daemon's does), and answers the
//! daemon's response with the text the command line would print. The browser is Chrome over CDP (agent-browser's own
//! launch and discovery); the Blitz backend answers the same words without Chrome.

pub mod blitz;

#[cfg(feature = "chrome")]
use std::sync::OnceLock;

#[cfg(feature = "chrome")]
use agent_browser::connection::Response;
#[cfg(feature = "chrome")]
use agent_browser::native::actions::{DaemonState, execute_command};
use serde_json::{Value, json};

#[cfg(feature = "chrome")]
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("raea-browser").build().expect("a tokio runtime"))
}

#[cfg(feature = "chrome")]
/// One daemon state per session (agent-browser runs a daemon per session; here they share the process).
fn state(session: &str) -> std::sync::Arc<tokio::sync::Mutex<DaemonState>> {
    static ST: OnceLock<std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<DaemonState>>>>> = OnceLock::new();
    let mut m = ST.get_or_init(Default::default).lock().unwrap();
    m.entry(session.to_string()).or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(DaemonState::new()))).clone()
}

/// Which backend answers: `--engine blitz` (or no Chrome in this build) is raea's own; anything else is Chrome.
fn blitz_wanted(args: &[String]) -> bool {
    !cfg!(feature = "chrome") || args.windows(2).any(|w| w[0] == "--engine" && w[1] == "blitz")
}

/// `batch CMD...` on either backend: each command's words run in turn, stopping at a failure with --bail.
fn batch(args: &[String]) -> Option<Result<Value, String>> {
    let pos = args.iter().position(|a| a == "batch")?;
    if args[..pos].iter().any(|a| !a.starts_with('-') && !args[..pos].windows(2).any(|w| w[1] == *a && w[0].starts_with("--"))) {
        return None;
    }
    let prefix: Vec<String> = args[..pos].to_vec();
    let rest = &args[pos + 1..];
    let bail = rest.iter().any(|a| a == "--bail");
    let mut results = Vec::new();
    let mut text = String::new();
    let mut ok = true;
    for c in rest.iter().filter(|a| *a != "--bail") {
        let mut words = prefix.clone();
        words.extend(split_words(c));
        let r = run(&words).unwrap_or_else(|e| json!({"success": false, "error": e, "text": format!("{e}\n")}));
        text.push_str(r["text"].as_str().unwrap_or(""));
        let good = r["success"].as_bool().unwrap_or(false);
        results.push(json!({"command": c, "result": r}));
        if !good {
            ok = false;
            if bail {
                break;
            }
        }
    }
    Some(Ok(json!({"success": ok, "data": {"results": results}, "error": if ok { Value::Null } else { json!("a command in the batch failed") }, "code": Value::Null, "text": text})))
}

/// Shell-style words: quotes group, backslashes escape.
fn split_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q: Option<char> = None;
    let mut any = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (q, c) {
            (None, '"' | '\'') => {
                q = Some(c);
                any = true;
            }
            (Some(x), c) if c == x => q = None,
            (_, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (_, c) => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

/// One command line's words run in process: the backend's response and the command line's text.
pub fn run(args: &[String]) -> Result<Value, String> {
    if let Some(r) = batch(args) {
        return r;
    }
    if blitz_wanted(args) {
        return blitz::run(args);
    }
    #[cfg(feature = "chrome")]
    return chrome(args);
    #[cfg(not(feature = "chrome"))]
    unreachable!()
}

#[cfg(feature = "chrome")]
fn chrome(args: &[String]) -> Result<Value, String> {
    let flags = agent_browser::flags::parse_flags(args);
    let clean = agent_browser::flags::clean_args(args);
    let cmd = agent_browser::commands::parse_command(&clean, &flags).map_err(|e| e.format())?;
    let action = cmd.get("action").and_then(Value::as_str).map(str::to_string);
    // state files (save/load) are local, as the command line handles them
    let resp: Value = if let Some(r) = agent_browser::native::state::dispatch_state_command(&cmd) {
        match r {
            Ok(data) => json!({"success": true, "data": data}),
            Err(e) => json!({"success": false, "error": e}),
        }
    } else {
        runtime().block_on(async {
            let st = state(&flags.session);
            let mut st = st.lock().await;
            execute_command(&cmd, &mut st).await
        })
    };
    let typed: Response = serde_json::from_value(resp.clone()).map_err(|e| format!("agent-browser answered something unexpected: {e}"))?;
    let opts = agent_browser::output::OutputOptions::from_flags(&flags);
    let text = match raea_native::capture(&std::env::current_dir().unwrap_or_default(), || agent_browser::output::print_response_with_opts(&typed, action.as_deref(), &opts)) {
        Ok(((), out, err)) => format!("{out}{err}"),
        Err(e) => e,
    };
    Ok(json!({"success": typed.success, "data": typed.data, "error": typed.error, "code": typed.code, "text": text}))
}

/// The browser.* calls in raea-native's registry.
pub fn register() {
    raea_native::register(
        "browser.run",
        "{ args: string[] }",
        "{ success: boolean; data: unknown; error: string | null; code: string | null; text: string }",
        "agent-browser's command line words run in process (Chrome over CDP, or --engine blitz: raea's own engine): the response and the printed text",
        |v| {
            let args: Vec<String> = v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            run(&args)
        },
    );
}
