//! The shell's network, natively (raea's replacement for just-bash's guarded fetch): a request goes out only when a
//! policy the host set allows its URL (origin and path prefix) and method, every redirect hop is checked again, and the
//! policy's header injections (credentials) are added here, at the boundary, from the host's environment: JavaScript
//! names a policy, never sees a secret, and can't override an injected header.
//!
//!   net.policy {allow: [{prefix, methods?, headers?: {name: value} , secretHeaders?: {name: ENV_VAR}}], timeoutMs?, maxRedirects?, maxBytes?} -> {id}
//!   net.fetch {policy, url, method?, headers?, body?, followRedirects?, maxRedirects?, timeoutMs?} -> {status, statusText, headers, body, url, redirectChain}

use std::collections::HashMap;
use std::io::Read as _;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value, json};

use crate::{Call, Handler, R, dec64, enc64, s};

struct Rule {
    origin: String,
    path: String,
    methods: Vec<String>,
    headers: Vec<(String, String)>,
}

struct Policy {
    rules: Vec<Rule>,
    timeout_ms: u64,
    max_redirects: u32,
    max_bytes: u64,
}

fn policies() -> &'static Mutex<(u64, HashMap<u64, Arc<Policy>>)> {
    static P: OnceLock<Mutex<(u64, HashMap<u64, Arc<Policy>>)>> = OnceLock::new();
    P.get_or_init(|| Mutex::new((0, HashMap::new())))
}

impl Policy {
    /// The rule allowing `url` with `method`, if any: the same origin (scheme, host, port) and a path under the prefix's.
    fn rule(&self, url: &url::Url, method: &str) -> Option<&Rule> {
        let origin = url.origin().ascii_serialization();
        self.rules.iter().find(|r| r.origin == origin && url.path().starts_with(&r.path) && r.methods.iter().any(|m| m.eq_ignore_ascii_case(method)))
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    }
}

pub(crate) fn calls() -> Vec<Call> {
    let mk = |name: &str, args: &str, answer: &str, doc: &str, f: Handler| Call { name: name.into(), args: args.into(), answer: answer.into(), doc: doc.into(), f };
    vec![
        mk(
            "net.policy",
            "{ allow: { prefix: string; methods?: string[]; headers?: Record<string, string>; secretHeaders?: Record<string, string> }[]; timeoutMs?: number; maxRedirects?: number; maxBytes?: number }",
            "{ id: number }",
            "a network policy (allowed URL prefixes and methods; headers injected at the boundary, secrets from the host's environment)",
            Arc::new(|v| {
                let mut rules = Vec::new();
                for a in v.get("allow").and_then(Value::as_array).ok_or("allow: a list")? {
                    let prefix = a.get("prefix").and_then(Value::as_str).ok_or("allow[].prefix")?;
                    let u = url::Url::parse(prefix).map_err(|e| format!("{prefix}: {e}"))?;
                    if !matches!(u.scheme(), "http" | "https") {
                        return Err(format!("{prefix}: only http and https"));
                    }
                    let methods = a.get("methods").and_then(Value::as_array).map(|m| m.iter().filter_map(|x| x.as_str().map(str::to_uppercase)).collect()).unwrap_or_else(|| vec!["GET".into(), "HEAD".into()]);
                    let mut headers: Vec<(String, String)> = a.get("headers").and_then(Value::as_object).map(|h| h.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.to_ascii_lowercase(), v.to_string()))).collect()).unwrap_or_default();
                    for (k, env) in a.get("secretHeaders").and_then(Value::as_object).into_iter().flatten() {
                        let name = env.as_str().ok_or("secretHeaders: environment variable names")?;
                        let val = std::env::var(name).map_err(|_| format!("secret {name} isn't set in the host's environment"))?;
                        headers.push((k.to_ascii_lowercase(), val));
                    }
                    rules.push(Rule { origin: u.origin().ascii_serialization(), path: u.path().to_string(), methods, headers });
                }
                let p = Policy {
                    rules,
                    timeout_ms: v.get("timeoutMs").and_then(Value::as_u64).unwrap_or(30_000),
                    max_redirects: v.get("maxRedirects").and_then(Value::as_u64).unwrap_or(10) as u32,
                    max_bytes: v.get("maxBytes").and_then(Value::as_u64).unwrap_or(64 << 20),
                };
                let mut g = policies().lock().unwrap();
                g.0 += 1;
                let id = g.0;
                g.1.insert(id, Arc::new(p));
                Ok(json!({"id": id}))
            }),
        ),
        mk(
            "net.fetch",
            "{ policy: number; url: string; method?: string; headers?: Record<string, string>; body?: string; followRedirects?: boolean; maxRedirects?: number; timeoutMs?: number }",
            "{ status: number; statusText: string; headers: Record<string, string>; body: string; url: string; redirectChain: { status: number; statusText: string; headers: Record<string, string> }[] }",
            "an HTTP request under a policy (body base64 both ways)",
            Arc::new(fetch),
        ),
    ]
}

fn fetch(v: &Value) -> R<Value> {
    let pid = v.get("policy").and_then(Value::as_u64).ok_or("policy: a network policy's id")?;
    let policy = policies().lock().unwrap().1.get(&pid).cloned().ok_or("no such network policy")?;
    let mut method = v.get("method").and_then(Value::as_str).unwrap_or("GET").to_uppercase();
    let mut url = url::Url::parse(s(v, "url")?).map_err(|e| format!("curl: (3) URL rejected: {e}"))?;
    let follow = v.get("followRedirects").and_then(Value::as_bool).unwrap_or(true);
    let max_redirects = v.get("maxRedirects").and_then(Value::as_u64).map(|m| (m as u32).min(policy.max_redirects)).unwrap_or(policy.max_redirects);
    let timeout = v.get("timeoutMs").and_then(Value::as_u64).map(|t| t.min(policy.timeout_ms)).unwrap_or(policy.timeout_ms);
    let mut body = match v.get("body").and_then(Value::as_str) {
        Some(b) => Some(dec64(b)?),
        None => None,
    };
    let given: Vec<(String, String)> = v.get("headers").and_then(Value::as_object).map(|h| h.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.to_ascii_lowercase(), v.to_string()))).collect()).unwrap_or_default();
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(std::time::Duration::from_millis(timeout)).build();
    let mut chain = Vec::new();
    loop {
        let rule = policy.rule(&url, &method).ok_or_else(|| format!("Network access denied: {method} {url} isn't in the allow-list"))?;
        let mut req = agent.request(&method, url.as_str());
        // the caller's headers, then the policy's on top (a caller can't override an injected credential)
        let injected: Vec<&str> = rule.headers.iter().map(|(k, _)| k.as_str()).collect();
        for (k, val) in &given {
            if !injected.contains(&k.as_str()) {
                req = req.set(k, val);
            }
        }
        for (k, val) in &rule.headers {
            req = req.set(k, val);
        }
        let resp = match &body {
            Some(b) => req.send_bytes(b),
            None => req.call(),
        };
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(format!("curl: (7) {e}")),
        };
        let status = resp.status();
        let mut headers = Map::new();
        for name in resp.headers_names() {
            if let Some(val) = resp.header(&name) {
                headers.insert(name.to_ascii_lowercase(), json!(val));
            }
        }
        let location = resp.header("location").map(str::to_string);
        if follow && (300..400).contains(&status) && location.is_some() {
            if chain.len() as u32 >= max_redirects {
                return Err(format!("curl: (47) Maximum ({max_redirects}) redirects followed"));
            }
            chain.push(json!({"status": status, "statusText": reason(status), "headers": headers}));
            url = url.join(location.as_deref().unwrap()).map_err(|e| format!("redirect: {e}"))?;
            if status == 303 || ((status == 301 || status == 302) && method == "POST") {
                method = "GET".into();
                body = None;
            }
            continue;
        }
        let mut bytes = Vec::new();
        resp.into_reader().take(policy.max_bytes + 1).read_to_end(&mut bytes).map_err(|e| format!("curl: (56) {e}"))?;
        if bytes.len() as u64 > policy.max_bytes {
            return Err(format!("curl: (63) the response is over {} bytes", policy.max_bytes));
        }
        return Ok(json!({"status": status, "statusText": reason(status), "headers": headers, "body": enc64(&bytes), "url": url.as_str(), "redirectChain": chain}));
    }
}
