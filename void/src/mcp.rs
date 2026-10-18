//! void's own MCP server, for an agent to debug, drive, test and look at the running app (void.feature, "An agent
//! can drive it"). Streamable HTTP, JSON answers only (no SSE), on 127.0.0.1:7457/mcp (RAEA_VOID_MCP_PORT; 0 turns it
//! off). Register it once with:
//!
//!     claude mcp add --transport http void http://127.0.0.1:7457/mcp
//!
//! Tools that need the app's state go through a channel to the UI (app.rs answers them); the screenshot, logs and
//! hearing are answered here.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use base64::Engine as _;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use raea_void::voice::runtime::{Voice, VoiceEvent};

/// The window's number (NSWindow.windowNumber), for screencapture -l. Set by app.rs once the window exists.
pub static WINDOW: AtomicIsize = AtomicIsize::new(0);

static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// One line in the app's log: the MCP `logs` tool reads it, and stderr gets it too.
pub fn log(line: impl AsRef<str>) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let line = format!("{:.3} {}", t % 86400.0, line.as_ref());
    eprintln!("void: {line}");
    let mut l = LOG.lock().unwrap();
    l.push_back(line);
    if l.len() > 2000 {
        l.pop_front();
    }
}

/// What app.rs answers.
#[derive(Debug)]
pub enum Call {
    State,
    Send { text: String, spoken: bool },
    Act { action: String, value: Value },
    Wait(f64),
}

pub type Calls = mpsc::UnboundedSender<(Call, oneshot::Sender<Result<Value, String>>)>;

pub fn port() -> u16 {
    std::env::var("RAEA_VOID_MCP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7457)
}

fn tools() -> Value {
    let obj = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
    json!([
        {"name": "state", "description": "The app now: open space, spaces, the stage (size, frame), voice (listening, speaking, voice, ready), brain (thinking, model), status, open sheets, and the last 30 transcript lines.",
         "inputSchema": obj(json!({}), &[])},
        {"name": "screenshot", "description": "The void window as a PNG (captured by its window number, so it works behind other windows).",
         "inputSchema": obj(json!({"max": {"type": "integer", "description": "longest side in px (default 1400)"}}), &[])},
        {"name": "send", "description": "Submit text in the composer, as if typed (or as a finished spoken turn with spoken=true). Slash commands work. Returns at once; use wait for the reply.",
         "inputSchema": obj(json!({"text": {"type": "string"}, "spoken": {"type": "boolean"}}), &["text"])},
        {"name": "act", "description": "One UI action: open_object (value: slug or name), new_object (value: name), delete_space (value: slug), objects (open the cube's list), answer (value: the label chosen, labels joined by \", \", or \"other: words\"; answers the waiting question), new_session, open_session (value: id), sessions (open the chats sheet), open_version (value: OBJECT@N, looks at version N), restore (value: OBJECT@N), back (from a version to the object as it is), scrub (value: frame), play, pause, listen, mute, voice (value: name), spaces (open the box), close (close sheets and menus), terminal (value: true/false), clear, files (show the Files tab), tab (value: terminal or files), import (value: a path or a list of paths, brought into the open space), remove_asset (value: a path in the space, e.g. media/cj.mov), open_workspace (value: a folder), new_workspace (value: a folder, made if missing), chooser (the workspace chooser; value: true/false), click / right_click (value: {x, y} in window points: a click as a person makes it, posted to void's own event queue), drop (value: {paths, x, y}: files dropped as Finder drops them).",
         "inputSchema": obj(json!({"action": {"type": "string"}, "value": {}}), &["action"])},
        {"name": "wait", "description": "Wait until the brain has finished its turn and the voice is quiet (or timeout seconds pass). Returns the state.",
         "inputSchema": obj(json!({"timeout": {"type": "number", "description": "seconds (default 60)"}}), &[])},
        {"name": "logs", "description": "The app's last n log lines (status, voice, brain, stage, errors).",
         "inputSchema": obj(json!({"n": {"type": "integer"}}), &[])},
        {"name": "hear", "description": "Play an audio file (any format ffmpeg reads) into the listening pipeline at real time, as if the mic heard it. Starts listening if needed. Returns once it has all been heard; then wait for the turn.",
         "inputSchema": obj(json!({"path": {"type": "string"}}), &["path"])},
        {"name": "say", "description": "Speak a line with the current voice (tests Pocket and the speaker).",
         "inputSchema": obj(json!({"text": {"type": "string"}}), &["text"])}
    ])
}

/// Starts the server thread. `voice` and `events` let `hear` and `say` reach the voice directly.
pub fn serve(calls: Calls, voice: Voice, events: mpsc::UnboundedSender<VoiceEvent>) {
    let port = port();
    if port == 0 {
        return;
    }
    std::thread::Builder::new()
        .name("void-mcp".into())
        .spawn(move || {
            let server = match tiny_http::Server::http(("127.0.0.1", port)) {
                Ok(s) => s,
                Err(e) => return log(format!("mcp: can't listen on {port}: {e}")),
            };
            log(format!("mcp: http://127.0.0.1:{port}/mcp"));
            for mut req in server.incoming_requests() {
                let (calls, voice, events) = (calls.clone(), voice.clone(), events.clone());
                // each request on its own thread: wait and hear take a while
                std::thread::spawn(move || {
                    if req.method() != &tiny_http::Method::Post {
                        let _ = req.respond(tiny_http::Response::empty(405));
                        return;
                    }
                    let mut body = String::new();
                    let _ = req.as_reader().read_to_string(&mut body);
                    let answer = match serde_json::from_str::<Value>(&body) {
                        Ok(msg) => handle(&msg, &calls, &voice, &events),
                        Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
                    };
                    let resp = match answer {
                        Some(a) => tiny_http::Response::from_string(a.to_string())
                            .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap())
                            .boxed(),
                        None => tiny_http::Response::empty(202).boxed(),
                    };
                    let _ = req.respond(resp);
                });
            }
        })
        .unwrap();
}

fn handle(msg: &Value, calls: &Calls, voice: &Voice, events: &mpsc::UnboundedSender<VoiceEvent>) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg["method"].as_str().unwrap_or("");
    // notifications (no id) get 202 and no body
    let id = id?;
    let result: Result<Value, String> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "void", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "raea void, running. Look with screenshot and state; drive it with send and act (clicks, drops); test the voice with hear and say; wait after send for the reply.",
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or("");
            let args = &msg["params"]["arguments"];
            log(format!("mcp: {name} {}", args.to_string().chars().take(120).collect::<String>()));
            Ok(match call(name, args, calls, voice, events) {
                Ok(content) => json!({"content": content}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            })
        }
        other => Err(format!("no method {other}")),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": e}}),
    })
}

fn text(v: impl Into<String>) -> Value {
    json!([{"type": "text", "text": v.into()}])
}

/// Ask app.rs and wait for its answer.
fn ask(calls: &Calls, c: Call) -> Result<Value, String> {
    let (tx, rx) = oneshot::channel();
    calls.send((c, tx)).map_err(|_| "the app isn't running".to_string())?;
    rx.blocking_recv().map_err(|_| "the app dropped the call".to_string())?
}

fn call(name: &str, args: &Value, calls: &Calls, voice: &Voice, events: &mpsc::UnboundedSender<VoiceEvent>) -> Result<Value, String> {
    let s = |k: &str| args[k].as_str().map(str::to_string).ok_or(format!("{k} is required"));
    match name {
        "state" => Ok(text(serde_json::to_string_pretty(&ask(calls, Call::State)?).unwrap())),
        "send" => {
            ask(calls, Call::Send { text: s("text")?, spoken: args["spoken"].as_bool().unwrap_or(false) })?;
            Ok(text("sent"))
        }
        "act" => Ok(text(ask(calls, Call::Act { action: s("action")?, value: args["value"].clone() })?.to_string())),
        "wait" => Ok(text(serde_json::to_string_pretty(&ask(calls, Call::Wait(args["timeout"].as_f64().unwrap_or(60.0)))?).unwrap())),
        "logs" => {
            let n = args["n"].as_u64().unwrap_or(100) as usize;
            let l = LOG.lock().unwrap();
            Ok(text(l.iter().skip(l.len().saturating_sub(n)).cloned().collect::<Vec<_>>().join("\n")))
        }
        "screenshot" => screenshot(args["max"].as_u64().unwrap_or(1400)),
        "hear" => {
            let path = s("path")?;
            let samples = decode16k(&path)?;
            let secs = samples.len() as f64 / 16_000.0;
            voice.hear(samples, events.clone()).map_err(|e| e.to_string())?;
            Ok(text(format!("heard {secs:.1} s of {path}")))
        }
        "say" => {
            voice.say(&s("text")?);
            Ok(text("queued"))
        }
        other => Err(format!("no tool {other}")),
    }
}

/// The window as PNG content, via screencapture -l (works while the window is covered).
fn screenshot(max: u64) -> Result<Value, String> {
    let id = WINDOW.load(Ordering::Relaxed);
    if id == 0 {
        return Err("the window isn't up yet".into());
    }
    let path = std::env::temp_dir().join(format!("void-shot-{}.png", std::process::id()));
    let ok = std::process::Command::new("screencapture")
        .args(["-x", "-o", &format!("-l{id}")])
        .arg(&path)
        .status()
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("screencapture failed (does the terminal that launched void have Screen Recording?)".into());
    }
    let _ = std::process::Command::new("sips").args(["-Z", &max.to_string()]).arg(&path).output();
    let png = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(json!([
        {"type": "image", "data": base64::engine::general_purpose::STANDARD.encode(png), "mimeType": "image/png"},
        {"type": "text", "text": format!("{}", path.display())}
    ]))
}

/// Any audio file as 16 kHz mono f32 (ffmpeg).
fn decode16k(path: &str) -> Result<Vec<f32>, String> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i", path, "-f", "f32le", "-ac", "1", "-ar", "16000", "-"])
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}
