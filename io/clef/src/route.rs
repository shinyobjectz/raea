//! Which way a call goes (docs/ask.feature AK1b), the first that answers: the raea app on this computer when it's signed
//! in with a pool (it adds its sign-in, which stays the app's alone); an agent key (`RAEA_AGENT_KEY` with `RAEA_API`, the
//! address the key came with, for a pipeline, CI or raea cloud); the person's own Cloudflare account. Unchanged questions on unchanged media come from the cache
//! first. `RAEA_CLEF_REPLAY` names a file of recorded answers, for tests that must never spend.

use std::path::PathBuf;

use raea_io_home::setting;
use serde_json::{Map, Value};

use crate::cache;
use crate::client::{Answered, Call, Fail, Transport, raea_api, workers_ai};

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// the raea app's host, at the address ~/.raea/app-host.json gives
    App(String),
    /// raea-api with an agent key that has the `ask` scope
    Agent { api: String, key: String },
    /// the person's own Workers AI
    Own { account: String, token: String },
    /// recorded answers by question id, for tests
    Replay(PathBuf),
}

impl Route {
    pub fn name(&self) -> &'static str {
        match self {
            Route::App(_) => "your raea plan, through the app",
            Route::Agent { .. } => "your raea plan, with an agent key",
            Route::Own { .. } => "your Cloudflare account",
            Route::Replay(_) => "recorded answers",
        }
    }
}

/// The address the running raea app listens at, if it left one (RAEA_HOST_FILE, else ~/.raea/app-host.json).
fn app() -> Option<String> {
    let file = raea_io_home::env_path("RAEA_HOST_FILE", ".raea/app-host.json");
    let j: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    j["url"].as_str().map(|u| u.trim_end_matches('/').to_string())
}

/// The routes open here, in the order they're tried. `RAEA_ASK_ROUTE` (app, agent or own) keeps one.
pub fn open() -> Vec<Route> {
    if let Some(p) = setting("RAEA_CLEF_REPLAY") {
        return vec![Route::Replay(p.into())];
    }
    let mut out = Vec::new();
    if let Some(url) = app() {
        out.push(Route::App(url));
    }
    if let (Some(key), Some(api)) = (setting("RAEA_AGENT_KEY"), setting("RAEA_API")) {
        out.push(Route::Agent { api: api.trim_end_matches('/').to_string(), key });
    }
    if let (Some(account), Some(token)) = (setting("CLOUDFLARE_ACCOUNT_ID"), setting("CLOUDFLARE_API_TOKEN")) {
        out.push(Route::Own { account, token });
    }
    match setting("RAEA_ASK_ROUTE").as_deref() {
        Some("app") => out.retain(|r| matches!(r, Route::App(_))),
        Some("agent") => out.retain(|r| matches!(r, Route::Agent { .. })),
        Some("own") => out.retain(|r| matches!(r, Route::Own { .. })),
        _ => {}
    }
    out
}

pub const NONE: &str = "sign in to raea on Pro or Studio (the app), set RAEA_AGENT_KEY and RAEA_API, or add CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN to ~/.raea/env (docs/settings.md)";

fn replay(call: &Call, file: &PathBuf) -> Result<Answered, Fail> {
    let read = || -> Result<Map<String, Value>, Box<dyn std::error::Error>> { Ok(serde_json::from_str(&std::fs::read_to_string(file)?)?) };
    let name = file.display();
    let all = read().map_err(|e| Fail::Stop(format!("{name} isn't a recorded answer: {e}")))?;
    let answers = call.questions.iter().filter_map(|q| all.get(&q.id).map(|a| (q.id.clone(), a.clone()))).collect();
    Ok(Answered { answers, cost: 0.0, cached: false, route: Route::Replay(file.clone()).name().into(), pool_left: None })
}

/// One call answered: from the cache, else by the first route that answers. Every route's reason is kept for the
/// refusal when none does.
pub fn ask(t: &dyn Transport, call: &Call, routes: &[Route]) -> Result<Answered, Fail> {
    if let Some(p) = call.problem() {
        return Err(Fail::Stop(p));
    }
    if let Some(Route::Replay(f)) = routes.first() {
        return replay(call, f);
    }
    let key = cache::key(call);
    if let Some(answers) = cache::get(&key) {
        return Ok(Answered { answers, cost: 0.0, cached: true, route: "asked before (cache)".into(), pool_left: None });
    }
    if routes.is_empty() {
        return Err(Fail::Next(format!("nothing to ask with: {NONE}")));
    }
    let own = routes.iter().any(|r| matches!(r, Route::Own { .. }));
    let mut why = Vec::new();
    for r in routes {
        let got = match r {
            Route::App(url) => raea_api(t, call, &format!("{url}/host/ask"), None, r.name(), own),
            Route::Agent { api, key } => raea_api(t, call, &format!("{api}/v1/clef/ask"), Some(key), r.name(), own),
            Route::Own { account, token } => workers_ai(t, call, account, token),
            Route::Replay(f) => replay(call, f),
        };
        match got {
            Ok(a) => {
                cache::put(&key, &a.answers);
                return Ok(a);
            }
            Err(Fail::Next(e)) => why.push(e),
            Err(f) => return Err(f),
        }
    }
    Err(Fail::Next(format!("nothing answered ({}): {NONE}", why.join("; "))))
}
