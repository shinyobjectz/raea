//! raea's agent traces (agent/agent.feature: one harness, two hosts). A span is OpenTelemetry's: a trace id, its
//! parent, a name, start and end, attributes and events, and an error status if it failed. Each finished span goes
//! two ways:
//! - one JSONL line beside the run, in the host app's shape (ugc-agent's trace.rs), so evals keep it with their results
//!   and anything can read it without a collector;
//! - OTLP/HTTP protobuf to a collector (OTEL_EXPORTER_OTLP_ENDPOINT, default Arize Phoenix at http://127.0.0.1:6006),
//!   batched once a second from a thread of its own, so tracing never slows a turn.
//!
//! Attribute names follow OpenTelemetry's GenAI conventions (gen_ai.*) and OpenInference's (the names Phoenix reads:
//! openinference.span.kind, llm.*, tool.*, input.value, output.value); `llm_call` and `tool_call` set both.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{span, status, ResourceSpans, ScopeSpans, Span as PSpan, Status};
use prost::Message;
use serde_json::{json, Map, Value};

fn now_ns() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    // time, a counter and the address of a stack value, mixed: unique enough for span ids without a rand crate
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let mut out = String::new();
    let mut x = now_ns() ^ N.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed) ^ (&out as *const _ as u64);
    while out.len() < bytes * 2 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push_str(&format!("{x:016x}"));
    }
    out.truncate(bytes * 2);
    out
}

/// A finished span, as both outputs need it.
#[derive(Clone, Debug)]
struct Done {
    trace_id: String,
    span_id: String,
    parent: Option<String>,
    name: String,
    kind: i32,
    start: u64,
    end: u64,
    attrs: Map<String, Value>,
    events: Vec<(u64, String, Map<String, Value>)>,
    error: Option<String>,
}

struct Inner {
    resource: Map<String, Value>,
    file: Mutex<Option<PathBuf>>,
    export: Option<mpsc::Sender<Done>>,
}

/// Where spans go. Cheap to clone; every clone writes to the same places.
#[derive(Clone)]
pub struct Tracer {
    inner: Arc<Inner>,
}

impl Tracer {
    /// `service` names the program (void, raea, ugc-agent); `project` is the Phoenix project spans are filed under
    /// (`openinference.project.name`); `resource` adds who ran it on what. OTLP export is off when
    /// OTEL_EXPORTER_OTLP_ENDPOINT is "off".
    pub fn new(service: &str, project: &str, mut resource: Map<String, Value>) -> Self {
        resource.insert("service.name".into(), json!(service));
        resource.insert("openinference.project.name".into(), json!(project));
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:6006".into());
        let export = (endpoint != "off").then(|| {
            let (tx, rx) = mpsc::channel::<Done>();
            let res = resource.clone();
            std::thread::Builder::new().name("raea-otel".into()).spawn(move || exporter(rx, res, endpoint)).ok();
            tx
        });
        Self { inner: Arc::new(Inner { resource, file: Mutex::new(None), export }) }
    }

    /// Also write each finished span as a JSONL line to `file` (the host app's shape), e.g. an eval run's trace.jsonl.
    pub fn to_file(&self, file: Option<PathBuf>) {
        *self.inner.file.lock().unwrap() = file;
    }

    /// A new trace's first span.
    pub fn root(&self, name: &str) -> Span {
        Span::new(self.clone(), random_hex(16), None, name)
    }

    fn finish(&self, d: Done) {
        if let Some(path) = self.inner.file.lock().unwrap().clone() {
            let line = json!({
                "traceId": d.trace_id, "spanId": d.span_id, "parentSpanId": d.parent, "name": d.name, "kind": d.kind,
                "startTimeUnixNano": d.start.to_string(), "endTimeUnixNano": d.end.to_string(),
                "status": match &d.error { Some(m) => json!({"code": 2, "message": m}), None => json!({"code": 1}) },
                "attributes": d.attrs,
                "events": d.events.iter().map(|(t, n, a)| json!({"timeUnixNano": t.to_string(), "name": n, "attributes": a})).collect::<Vec<_>>(),
                "resource": self.inner.resource,
            });
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                use std::io::Write;
                let _ = writeln!(f, "{line}");
            }
        }
        if let Some(tx) = &self.inner.export {
            let _ = tx.send(d);
        }
    }
}

/// An open span. Ends when dropped (or with `end`), so a turn cut short still records what it did.
pub struct Span {
    tracer: Tracer,
    done: Option<Done>,
}

impl Span {
    fn new(tracer: Tracer, trace_id: String, parent: Option<String>, name: &str) -> Self {
        let done = Done {
            trace_id,
            span_id: random_hex(8),
            parent,
            name: name.into(),
            kind: 1,
            start: now_ns(),
            end: 0,
            attrs: Map::new(),
            events: Vec::new(),
            error: None,
        };
        Self { tracer, done: Some(done) }
    }

    /// Moves the start back by `d`: for work measured where no span could be held (another thread, another crate),
    /// recorded when its report arrives.
    pub fn backdate(&mut self, d: std::time::Duration) -> &mut Self {
        if let Some(done) = &mut self.done {
            done.start = done.start.saturating_sub(d.as_nanos() as _);
        }
        self
    }

    /// A span inside this one.
    pub fn child(&self, name: &str) -> Span {
        let d = self.done.as_ref().unwrap();
        Span::new(self.tracer.clone(), d.trace_id.clone(), Some(d.span_id.clone()), name)
    }

    pub fn trace_id(&self) -> String {
        self.done.as_ref().map(|d| d.trace_id.clone()).unwrap_or_default()
    }

    pub fn set(&mut self, key: &str, value: impl Into<Value>) -> &mut Self {
        if let Some(d) = self.done.as_mut() {
            d.attrs.insert(key.into(), value.into());
        }
        self
    }

    /// OpenInference's span kind (LLM, TOOL, CHAIN, AGENT, RETRIEVER, EVALUATOR): how Phoenix draws it.
    pub fn kind(&mut self, k: &str) -> &mut Self {
        self.set("openinference.span.kind", k)
    }

    pub fn event(&mut self, name: &str, attrs: Map<String, Value>) -> &mut Self {
        if let Some(d) = self.done.as_mut() {
            d.events.push((now_ns(), name.into(), attrs));
        }
        self
    }

    pub fn error(&mut self, message: &str) -> &mut Self {
        if let Some(d) = self.done.as_mut() {
            d.error = Some(message.into());
        }
        self
    }

    /// A model call's facts, in both conventions: the model, tokens in, out and read from cache, cost, latency to
    /// first output, and (cut to 32 KB) what went in and came out.
    #[allow(clippy::too_many_arguments)]
    pub fn llm(&mut self, provider: &str, model: &str, input_tokens: u64, output_tokens: u64, cache_read: u64, cache_write: u64, cost_usd: f64) -> &mut Self {
        self.kind("LLM")
            .set("gen_ai.system", provider)
            .set("gen_ai.request.model", model)
            .set("gen_ai.usage.input_tokens", input_tokens)
            .set("gen_ai.usage.output_tokens", output_tokens)
            .set("gen_ai.usage.cache_read_input_tokens", cache_read)
            .set("gen_ai.usage.cache_creation_input_tokens", cache_write)
            .set("llm.provider", provider)
            .set("llm.model_name", model)
            .set("llm.token_count.prompt", input_tokens + cache_read + cache_write)
            .set("llm.token_count.completion", output_tokens)
            .set("llm.token_count.total", input_tokens + cache_read + cache_write + output_tokens)
            .set("llm.token_count.prompt_details.cache_read", cache_read)
            .set("llm.token_count.prompt_details.cache_write", cache_write)
            .set("llm.cost.total", cost_usd)
    }

    /// What went in and what came out, cut to 32 KB each (Phoenix's input.value / output.value).
    pub fn io(&mut self, input: &str, output: &str) -> &mut Self {
        let cut = |s: &str| s.chars().take(32 * 1024).collect::<String>();
        self.set("input.value", cut(input)).set("output.value", cut(output))
    }

    /// A tool call: its name and arguments (both conventions).
    pub fn tool(&mut self, name: &str, args: &Value) -> &mut Self {
        self.kind("TOOL")
            .set("tool.name", name)
            .set("gen_ai.tool.name", name)
            .set("tool.parameters", args.to_string())
            .set("input.value", args.to_string().chars().take(32 * 1024).collect::<String>())
    }

    pub fn end(mut self) {
        self.close();
    }

    fn close(&mut self) {
        if let Some(mut d) = self.done.take() {
            d.end = now_ns();
            self.tracer.finish(d);
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        self.close();
    }
}

fn any(v: &Value) -> AnyValue {
    let value = match v {
        Value::String(s) => any_value::Value::StringValue(s.clone()),
        Value::Bool(b) => any_value::Value::BoolValue(*b),
        Value::Number(n) if n.is_i64() || n.is_u64() => any_value::Value::IntValue(n.as_i64().unwrap_or(n.as_u64().unwrap_or(0) as i64)),
        Value::Number(n) => any_value::Value::DoubleValue(n.as_f64().unwrap_or(0.0)),
        other => any_value::Value::StringValue(other.to_string()),
    };
    AnyValue { value: Some(value) }
}

fn kvs(m: &Map<String, Value>) -> Vec<KeyValue> {
    m.iter().map(|(k, v)| KeyValue { key: k.clone(), value: Some(any(v)), ..Default::default() }).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

fn proto(d: &Done) -> PSpan {
    PSpan {
        trace_id: unhex(&d.trace_id),
        span_id: unhex(&d.span_id),
        parent_span_id: d.parent.as_deref().map(unhex).unwrap_or_default(),
        name: d.name.clone(),
        kind: d.kind,
        start_time_unix_nano: d.start,
        end_time_unix_nano: d.end,
        attributes: kvs(&d.attrs),
        events: d.events.iter().map(|(t, n, a)| span::Event { time_unix_nano: *t, name: n.clone(), attributes: kvs(a), ..Default::default() }).collect(),
        status: Some(Status {
            code: if d.error.is_some() { status::StatusCode::Error as i32 } else { status::StatusCode::Ok as i32 },
            message: d.error.clone().unwrap_or_default(),
        }),
        ..Default::default()
    }
}

/// The export thread: whatever has finished, once a second, as one OTLP request. A collector that's down loses that
/// batch from Phoenix only; the JSONL beside the run keeps it.
fn exporter(rx: mpsc::Receiver<Done>, resource: Map<String, Value>, endpoint: String) {
    let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).build() {
        Ok(c) => c,
        Err(_) => return,
    };
    let url = format!("{}/v1/traces", endpoint.trim_end_matches('/'));
    loop {
        let first = match rx.recv() {
            Ok(d) => d,
            Err(_) => return,
        };
        let mut batch = vec![first];
        let until = std::time::Instant::now() + Duration::from_secs(1);
        while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(d) => batch.push(d),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource { attributes: kvs(&resource), ..Default::default() }),
                scope_spans: vec![ScopeSpans {
                    scope: Some(InstrumentationScope { name: "raea-otel".into(), version: env!("CARGO_PKG_VERSION").into(), ..Default::default() }),
                    spans: batch.iter().map(proto).collect(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let _ = client.post(&url).header("content-type", "application/x-protobuf").body(req.encode_to_vec()).send();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_nest_and_land_in_the_file() {
        let dir = std::env::temp_dir().join(format!("raea-otel-{}", std::process::id()));
        let file = dir.join("trace.jsonl");
        let _ = std::fs::remove_file(&file);
        // no collector in a unit test
        unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "off") };
        let t = Tracer::new("raea-otel-test", "tests", Map::new());
        t.to_file(Some(file.clone()));
        {
            let mut turn = t.root("agent.turn");
            turn.kind("AGENT").io("make the title red", "Red it is.");
            let mut call = turn.child("chat MiniMax-M3");
            call.llm("ugc", "MiniMax-M3", 120, 40, 4000, 0, 0.0012);
            call.end();
            let mut tool = turn.child("execute_tool apply");
            tool.tool("apply", &json!({"rrl": "hook.color = #ff3b30"})).error("REFUSED");
        }
        let lines: Vec<Value> = std::fs::read_to_string(&file).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 3);
        let turn = lines.iter().find(|l| l["name"] == "agent.turn").unwrap();
        assert!(lines.iter().filter(|l| l["name"] != "agent.turn").all(|l| l["parentSpanId"] == turn["spanId"] && l["traceId"] == turn["traceId"]));
        assert_eq!(lines.iter().find(|l| l["name"] == "chat MiniMax-M3").unwrap()["attributes"]["llm.token_count.prompt_details.cache_read"], 4000);
        assert_eq!(lines.iter().find(|l| l["name"] == "execute_tool apply").unwrap()["status"]["code"], 2);
    }
}
