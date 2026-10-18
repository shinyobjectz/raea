//! T-120: a reply that hits the output limit inside a tool call. Before the fix the loop ended the turn with the call
//! unanswered, and the provider rejected every later request in that chat ("tool call result does not follow tool
//! call"). Against a fake Anthropic-style server: the cut-off call isn't run, it's answered, and the loop carries on;
//! and a history that already holds an unanswered call is repaired before the first request.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_agent::{run_agent_with_history, AgentConfig, AgentTool, AgentToolResult};
use pi_ai::{AssistantMessage, Content, Message, Model, ModelPricing, StopReason, Usage};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn sse(events: &[(&str, Value)]) -> String {
    events.iter().map(|(name, data)| format!("event: {name}\ndata: {data}\n\n")).collect()
}

fn start(id: &str) -> (&'static str, Value) {
    ("message_start", json!({"type": "message_start", "message": {"id": id, "type": "message", "role": "assistant",
        "content": [], "model": "MiniMax-M3", "stop_reason": null, "usage": {"input_tokens": 120, "output_tokens": 1}}}))
}

/// A reply cut off by max_tokens halfway through a `write` call's arguments.
fn cut_off_write() -> String {
    sse(&[
        start("m1"),
        ("content_block_start", json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Let me build the new file:"}})),
        ("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        ("content_block_start", json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "tu_1", "name": "write", "input": {}}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"path\": \"edits/v2.w.html\", \"content\": \"<!doctype html><html><body><div id=\\\"root\\\" data-dur"}})),
        ("message_delta", json!({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}, "usage": {"output_tokens": 16384}})),
        ("message_stop", json!({"type": "message_stop"})),
    ])
}

fn text(id: &str, t: &str) -> String {
    sse(&[
        start(id),
        ("content_block_start", json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": t}})),
        ("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        ("message_delta", json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 12}})),
        ("message_stop", json!({"type": "message_stop"})),
    ])
}

/// A reply that calls `write` with whole arguments.
fn full_write(id: &str, tu: &str) -> String {
    sse(&[
        start(id),
        ("content_block_start", json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": tu, "name": "write", "input": {}}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"path\": \"a.txt\", \"content\": \"hi\"}"}})),
        ("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        ("message_delta", json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 30}})),
        ("message_stop", json!({"type": "message_stop"})),
    ])
}

/// A reply whose stream just stops: text, then no stop reason and no message_stop (a dropped connection).
fn cut_mid_reply() -> String {
    sse(&[
        start("m0"),
        ("content_block_start", json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})),
        ("content_block_delta", json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "I'll inline the end card directly into the edit"}})),
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

/// A `write` that counts its calls: a cut-off call must never reach it.
struct Write(Arc<AtomicUsize>);

#[async_trait]
impl AgentTool for Write {
    fn name(&self) -> &str { "write" }
    fn description(&self) -> &str { "write a file" }
    fn parameters(&self) -> Value { json!({"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}}) }
    async fn execute(&self, _id: &str, _a: Value) -> Result<AgentToolResult, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(AgentToolResult::text("wrote it"))
    }
}

fn config(base: &str, writes: Arc<AtomicUsize>) -> AgentConfig {
    let model = Model {
        id: "MiniMax-M3".into(), name: "MiniMax-M3".into(), api: "anthropic-messages".into(), provider: "test".into(),
        base_url: base.into(), reasoning: false, context_window: 200_000, max_tokens: 16_384, pricing: ModelPricing::default(),
    };
    let mut c = AgentConfig::new(model, "test").with_tools(vec![Arc::new(Write(writes))]).with_max_turns(5);
    c.stream_options.api_key = Some("test".into());
    c
}

fn user(t: &str) -> Message { Message::User { content: vec![Content::text(t)], timestamp: pi_ai::now_ms() } }

fn assistant(content: Vec<Content>, stop_reason: StopReason) -> Message {
    Message::Assistant(AssistantMessage { content, api: "anthropic-messages".into(), provider: "test".into(), model: "MiniMax-M3".into(),
        usage: Usage::default(), stop_reason, error_message: None, timestamp: 0 })
}

/// Whether the request's messages answer tool call `id` (an Anthropic tool_result block for it).
fn answers(request: &Value, id: &str) -> bool {
    request["messages"].as_array().into_iter().flatten()
        .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
        .any(|b| b["type"] == "tool_result" && b["tool_use_id"] == id)
}

#[tokio::test]
async fn length_stop_inside_a_tool_call_is_answered_and_the_turn_goes_on() {
    let (base, seen) = mock(vec![cut_off_write(), text("m2", "That file is big, so I'll write it in two parts.")]).await;
    let writes = Arc::new(AtomicUsize::new(0));
    let run = run_agent_with_history(&config(&base, writes.clone()), vec![user("Make a 90 s version")], None).await.unwrap();

    assert_eq!(writes.load(Ordering::SeqCst), 0, "a cut-off call never runs");
    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 2, "the loop asked the model again instead of ending the turn");
    assert!(answers(&requests[1], "tu_1"), "the second request answers the cut-off call: {}", requests[1]);
    let result = run.messages.iter().find_map(|m| match m { Message::ToolResult(t) if t.tool_call_id == "tu_1" => Some(t), _ => None }).unwrap();
    assert!(result.is_error);
    assert!(format!("{:?}", result.content).contains("output limit"), "it says why: {:?}", result.content);
    assert!(matches!(run.messages.last(), Some(Message::Assistant(a)) if a.stop_reason == StopReason::Stop));
}

#[tokio::test]
async fn length_stop_left_in_a_history_is_repaired_before_the_first_request() {
    let (base, seen) = mock(vec![text("m3", "Here's the punchier version.")]).await;
    let open = assistant(vec![Content::text("Let me build the new file:"),
        Content::ToolCall { id: "tu_old".into(), name: "write".into(), arguments: json!({}) }], StopReason::Length);
    let history = vec![user("Make a 90 s version"), open, user("Try again")];
    let run = run_agent_with_history(&config(&base, Arc::new(AtomicUsize::new(0))), history, None).await.unwrap();
    let requests = seen.lock().unwrap().clone();
    assert!(answers(&requests[0], "tu_old"), "the old call is answered in the first request: {}", requests[0]);
    assert!(run.messages.iter().any(|m| matches!(m, Message::ToolResult(t) if t.tool_call_id == "tu_old" && t.is_error)));
}

#[test]
fn length_stop_helper_answers_only_what_is_open() {
    let call = |id: &str| Content::ToolCall { id: id.into(), name: "write".into(), arguments: json!({}) };
    let result = |id: &str| Message::ToolResult(pi_ai::ToolResultMessage { tool_call_id: id.into(), tool_name: "write".into(),
        content: vec![Content::text("ok")], is_error: false, timestamp: 0 });
    let mut messages = vec![user("go"), assistant(vec![call("a"), call("b")], StopReason::ToolUse), result("a"), user("next")];
    assert_eq!(pi_agent::agent_loop::answer_unanswered(&mut messages), 1);
    assert!(matches!(&messages[3], Message::ToolResult(t) if t.tool_call_id == "b" && t.is_error), "b answered right after a's result");
    assert!(matches!(&messages[4], Message::User { .. }));
    assert_eq!(pi_agent::agent_loop::answer_unanswered(&mut messages), 0, "nothing left to answer");
}

#[tokio::test]
async fn stream_ended_early_is_retried_once() {
    let (base, seen) = mock(vec![cut_mid_reply(), text("m4", "Rendered: exports/end-card.mp4.")]).await;
    let run = run_agent_with_history(&config(&base, Arc::new(AtomicUsize::new(0))), vec![user("Add the end card and render")], None)
        .await.unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2, "the cut-off step is asked again");
    let last = match run.messages.last() { Some(Message::Assistant(a)) => a.clone(), other => panic!("{other:?}") };
    assert!(format!("{:?}", last.content).contains("Rendered"), "the turn ends on the retried reply: {:?}", last.content);
    assert_eq!(run.messages.iter().filter(|m| matches!(m, Message::Assistant(_))).count(), 1, "the half reply isn't kept");
}

#[tokio::test]
async fn stream_ended_early_twice_is_an_error_that_says_so() {
    let (base, _seen) = mock(vec![cut_mid_reply(), cut_mid_reply()]).await;
    let err = run_agent_with_history(&config(&base, Arc::new(AtomicUsize::new(0))), vec![user("Add the end card and render")], None)
        .await.err().expect("two cut-off replies end the turn with an error");
    assert!(err.to_string().contains("cut off"), "{err}");
}

#[tokio::test]
async fn near_the_step_limit_the_tool_results_say_so_once() {
    let (base, seen) = mock(vec![full_write("m1", "tu_1"), full_write("m2", "tu_2"), text("m3", "Rendered what I had.")]).await;
    let writes = Arc::new(AtomicUsize::new(0));
    let mut c = config(&base, writes.clone()).with_max_turns(3);
    c.wrap_up = Some((1, "1 step left: render what you have.".into()));
    run_agent_with_history(&c, vec![user("Cut it and render")], None).await.unwrap();
    let requests = seen.lock().unwrap().clone();
    assert_eq!((requests.len(), writes.load(Ordering::SeqCst)), (3, 2));
    let said = |r: &Value| r.to_string().matches("1 step left").count();
    assert_eq!((said(&requests[1]), said(&requests[2])), (0, 1), "only the step with one left carries it, once");
}
