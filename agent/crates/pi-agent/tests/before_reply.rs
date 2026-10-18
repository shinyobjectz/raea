//! T-141: before a reply without tool calls ends the run, the caller's `before_reply` may send the model back once
//! with what's unfinished. Against a fake Anthropic-style server: the first final reply gets the note as a user
//! message and the loop goes on; the second final reply ends the run, whatever the hook would say.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use pi_agent::{run_agent_with_history, AgentConfig, AgentEvent, BeforeReply};
use pi_ai::{Content, Message, Model, ModelPricing};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn sse(events: &[(&str, Value)]) -> String {
    events.iter().map(|(name, data)| format!("event: {name}\ndata: {data}\n\n")).collect()
}

fn text(id: &str, t: &str) -> String {
    sse(&[
        ("message_start", json!({"type": "message_start", "message": {"id": id, "type": "message", "role": "assistant",
            "content": [], "model": "MiniMax-M3", "stop_reason": null, "usage": {"input_tokens": 120, "output_tokens": 1}}})),
        ("content_block_start", json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": t}})),
        ("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        ("message_delta", json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 12}})),
        ("message_stop", json!({"type": "message_stop"})),
    ])
}

/// Serves `bodies` in order as SSE; records each request's JSON body.
async fn mock(bodies: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (seen2, bodies, count) = (seen.clone(), Arc::new(bodies), Arc::new(AtomicUsize::new(0)));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let (seen, bodies, count) = (seen2.clone(), bodies.clone(), count.clone());
            tokio::spawn(async move {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 8192]);
                let body = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 { return; }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                        while buf.len() < pos + 4 + len {
                            let n = sock.read(&mut chunk).await.unwrap_or(0);
                            if n == 0 { break; }
                            buf.extend_from_slice(&chunk[..n]);
                        }
                        break buf[pos + 4..].to_vec();
                    }
                };
                seen.lock().unwrap().push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                let i = count.fetch_add(1, Ordering::SeqCst);
                let (status, body) = match bodies.get(i) { Some(b) => (200, b.clone()), None => (500, "no more scripted replies".to_string()) };
                let kind = if status == 200 { "text/event-stream" } else { "application/json" };
                let head = format!("HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(body.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), seen)
}

fn config(base: &str, max_turns: u32) -> AgentConfig {
    let model = Model {
        id: "MiniMax-M3".into(), name: "MiniMax-M3".into(), api: "anthropic-messages".into(), provider: "test".into(),
        base_url: base.into(), reasoning: false, context_window: 200_000, max_tokens: 16_384, pricing: ModelPricing::default(),
    };
    let mut c = AgentConfig::new(model, "test").with_max_turns(max_turns);
    c.stream_options.api_key = Some("test".into());
    c
}

fn user(t: &str) -> Message { Message::User { content: vec![Content::text(t)], timestamp: pi_ai::now_ms() } }

const NOTE: &str = "Before you reply: render: no export from this turn. Fix these, then reply.";

/// A hook that counts its calls and always says `says` (None: everything passed).
fn hook(calls: Arc<AtomicUsize>, says: Option<&'static str>) -> BeforeReply {
    Arc::new(move |_messages: &[Message]| -> BoxFuture<'static, Option<String>> {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { says.map(String::from) })
    })
}

fn text_of(m: &Message) -> String {
    let content = match m { Message::User { content, .. } => content, Message::Assistant(a) => &a.content, Message::ToolResult(t) => &t.content };
    content.iter().filter_map(|c| c.as_text()).collect::<Vec<_>>().join("")
}

#[tokio::test]
async fn before_reply_sends_the_first_final_reply_back_once() {
    let (base, seen) = mock(vec![text("m1", "Done, the cut is ready."), text("m2", "Rendered: exports/cut.mp4.")]).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut c = config(&base, 5);
    c.before_reply = Some(hook(calls.clone(), Some(NOTE)));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let run = run_agent_with_history(&c, vec![user("Cut it and render")], Some(tx)).await.unwrap();

    assert_eq!(seen.lock().unwrap().len(), 2, "the first final reply was sent back and the model asked again");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "asked once per run: the second final reply ends it");
    let said: Vec<String> = run.messages.iter().map(text_of).collect();
    assert_eq!(said, ["Cut it and render", "Done, the cut is ready.", NOTE, "Rendered: exports/cut.mp4."]);
    assert!(matches!(&run.messages[2], Message::User { .. }), "the note is a user message");
    let second = seen.lock().unwrap()[1].clone();
    assert!(second["messages"].to_string().contains("no export from this turn"), "the second request carries the note: {second}");
    // The note is an event too, so a host that saves messages from events keeps it.
    let mut notes = 0;
    while let Ok(ev) = rx.try_recv() {
        if let AgentEvent::UserMessage { message } = ev { if text_of(&message) == NOTE { notes += 1; } }
    }
    assert_eq!(notes, 1);
    assert!(!run.stopped_at_turn_limit);
}

#[tokio::test]
async fn before_reply_that_passes_lets_the_reply_end_the_run() {
    let (base, seen) = mock(vec![text("m1", "Rendered: exports/cut.mp4.")]).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut c = config(&base, 5);
    c.before_reply = Some(hook(calls.clone(), None));
    let run = run_agent_with_history(&c, vec![user("Cut it and render")], None).await.unwrap();
    assert_eq!((seen.lock().unwrap().len(), calls.load(Ordering::SeqCst), run.messages.len()), (1, 1, 2));
}

#[tokio::test]
async fn before_reply_is_not_asked_without_a_step_left() {
    let (base, seen) = mock(vec![text("m1", "Here's where it stands.")]).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut c = config(&base, 1);
    c.before_reply = Some(hook(calls.clone(), Some(NOTE)));
    let run = run_agent_with_history(&c, vec![user("Cut it and render")], None).await.unwrap();
    assert_eq!((seen.lock().unwrap().len(), calls.load(Ordering::SeqCst)), (1, 0), "a note with no step to act on is never sent");
    assert!(matches!(run.messages.last(), Some(Message::Assistant(_))));
}
