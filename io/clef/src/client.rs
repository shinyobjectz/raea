//! One call to the decision model (docs/ask.feature AK1b): the state, the questions by id and the media, sent to Workers
//! AI with the person's keys, or to raea-api (directly with an agent key, or through the app, which adds its sign-in).
//! The answers come back by question id with the call's cost. A transport is handed in, so tests replay recorded
//! answers and never spend.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use raea_core_ask::{Question, limits};
use serde_json::{Map, Value, json};

/// The media one call reads: stills as JPEG, videos as MP4, sounds as MP3, each already cut to clef's limits.
#[derive(Debug, Clone, Default)]
pub struct Media {
    pub stills: Vec<Vec<u8>>,
    pub videos: Vec<Vec<u8>>,
    pub sounds: Vec<Vec<u8>>,
}

/// One call: what the piece is (the state), every question about this media, and the media.
#[derive(Debug, Clone, Default)]
pub struct Call {
    pub state: String,
    pub questions: Vec<Question>,
    pub media: Media,
}

/// What came back: the answers by question id (clef's own shape: `noul`, or `choice` and `probabilities`, or `score`),
/// what it cost, whether it came from a cache, the route it took and the pool left after it (raea's routes only).
#[derive(Debug, Clone, Default)]
pub struct Answered {
    pub answers: Map<String, Value>,
    pub cost: f64,
    pub cached: bool,
    pub route: String,
    pub pool_left: Option<f64>,
}

/// Why a route didn't answer: `Next` means try the next one (no plan, signed out, not running, no ask scope; from
/// `route::ask`, that none did); `Stop` means asking again won't help (the call itself is wrong); `Spent` means the
/// month's pool is used up and nothing else is set, so a batch's remaining calls wait for the pool.
#[derive(Debug, Clone, PartialEq)]
pub enum Fail {
    Next(String),
    Stop(String),
    Spent(String),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Fail::Next(s) | Fail::Stop(s) | Fail::Spent(s)) = self;
        f.write_str(s)
    }
}

/// An HTTP POST of JSON: its status and body. `Http` is the real one; a test hands in its own.
pub trait Transport {
    fn post(&self, url: &str, bearer: Option<&str>, body: &Value) -> Result<(u16, Value), String>;
}

pub struct Http;

impl Transport for Http {
    fn post(&self, url: &str, bearer: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
        let mut r = ureq::post(url).timeout(std::time::Duration::from_secs(180));
        if let Some(b) = bearer {
            r = r.set("authorization", &format!("Bearer {b}"));
        }
        match r.send_json(body.clone()) {
            Ok(res) => Ok((res.status(), res.into_json().map_err(|e| format!("{url}: {e}"))?)),
            Err(ureq::Error::Status(code, res)) => Ok((code, res.into_json().unwrap_or_else(|_| json!({})))),
            Err(e) => Err(format!("{url}: {e}")),
        }
    }
}

impl Call {
    /// Why it can't be sent as it stands, or None: clef's counts and sizes, every question's own limits.
    pub fn problem(&self) -> Option<String> {
        let m = &self.media;
        if self.questions.is_empty() || self.questions.len() > limits::QUESTIONS {
            return Some(format!("a call asks 1 to {} questions, not {}", limits::QUESTIONS, self.questions.len()));
        }
        if let Some(p) = self.questions.iter().find_map(limits::check) {
            return Some(p);
        }
        if m.stills.is_empty() && m.videos.is_empty() && m.sounds.is_empty() {
            return Some("a call needs something to look at or listen to".into());
        }
        let over = |v: &[Vec<u8>], most: usize, each: usize| v.len() > most || v.iter().any(|b| b.len() > each);
        if over(&m.stills, limits::STILLS, limits::STILL_BYTES) || m.stills.iter().map(Vec::len).sum::<usize>() > limits::STILLS_BYTES {
            return Some(format!("at most {} stills of 4 MiB, 8 MiB in all", limits::STILLS));
        }
        if over(&m.videos, limits::VIDEOS, limits::VIDEO_BYTES) || over(&m.sounds, limits::SOUNDS, limits::SOUND_BYTES) {
            return Some(format!("at most {} videos of 16 MiB and {} sounds of 8 MiB", limits::VIDEOS, limits::SOUNDS));
        }
        (m.videos.iter().chain(&m.sounds).map(Vec::len).sum::<usize>() > limits::AV_BYTES).then(|| "the videos and sounds together are over 16 MiB".into())
    }

    fn questions(&self) -> Value {
        Value::Object(self.questions.iter().map(|q| (q.id.clone(), q.wire())).collect())
    }

    fn state(&self) -> String {
        self.state.chars().take(limits::STATE_CHARS).collect()
    }

    /// The body Workers AI takes: the media as data URLs.
    pub fn clef_body(&self) -> Value {
        let urls = |v: &[Vec<u8>], mime: &str| v.iter().map(|b| format!("data:{mime};base64,{}", B64.encode(b))).collect::<Vec<_>>();
        let mut body = json!({"state": self.state(), "questions": self.questions()});
        for (k, v, mime) in [("images", &self.media.stills, "image/jpeg"), ("videos", &self.media.videos, "video/mp4"), ("audio", &self.media.sounds, "audio/mpeg")] {
            if !v.is_empty() {
                body[k] = json!(urls(v, mime));
            }
        }
        body
    }

    /// raea-api's body: the media as bare base64 (it adds the data URL's type).
    pub fn api_body(&self) -> Value {
        let mut body = json!({"state": self.state(), "questions": self.questions()});
        for (k, v) in [("images", &self.media.stills), ("videos", &self.media.videos), ("audio", &self.media.sounds)] {
            if !v.is_empty() {
                body[k] = json!(v.iter().map(|b| B64.encode(b)).collect::<Vec<_>>());
            }
        }
        body
    }
}

fn said(body: &Value, code: u16) -> String {
    body["message"].as_str().or(body["error"].as_str()).map(str::to_string).unwrap_or_else(|| format!("HTTP {code}"))
}

/// Workers AI, on the person's own Cloudflare account.
pub fn workers_ai(t: &dyn Transport, call: &Call, account: &str, token: &str) -> Result<Answered, Fail> {
    let url = format!("https://api.cloudflare.com/client/v4/accounts/{account}/ai/run/@cf/cloudflare/clef-omni");
    let (code, j) = t.post(&url, Some(token), &call.clef_body()).map_err(Fail::Stop)?;
    if code == 401 || code == 403 {
        return Err(Fail::Next(format!("your Cloudflare keys were refused ({code}): check CLOUDFLARE_API_TOKEN")));
    }
    if j["success"] != true {
        return Err(Fail::Stop(format!("clef: {}", j["errors"].to_string().chars().take(300).collect::<String>())));
    }
    let r = &j["result"];
    let cost = r["usage"]["input_tokens"].as_f64().unwrap_or(0.0) * limits::USD_PER_M / 1e6;
    Ok(Answered { answers: r["answers"].as_object().cloned().unwrap_or_default(), cost, cached: false, route: "your Cloudflare account".into(), pool_left: None })
}

/// raea-api's /v1/clef/ask at `url` (the API itself with an agent key, or the app's /host/ask, which signs it), charged
/// to the account's pool. `own_keys` says whether a spent pool may fall through to the person's own account.
pub fn raea_api(t: &dyn Transport, call: &Call, url: &str, bearer: Option<&str>, route: &str, own_keys: bool) -> Result<Answered, Fail> {
    let (code, j) = t.post(url, bearer, &call.api_body()).map_err(|e| Fail::Next(format!("{route} didn't answer: {e}")))?;
    match (code, j["error"].as_str()) {
        (200..=299, _) => Ok(Answered {
            answers: j["answers"].as_object().cloned().unwrap_or_default(),
            cost: j["cost_usd"].as_f64().unwrap_or(0.0),
            cached: j["cached"] == true,
            route: route.into(),
            pool_left: j["pool_left_usd"].as_f64(),
        }),
        (402, Some("pool_spent")) if !own_keys => Err(Fail::Spent(said(&j, code))),
        (400 | 413, _) => Err(Fail::Stop(said(&j, code))),
        _ => Err(Fail::Next(format!("{route}: {}", said(&j, code)))),
    }
}
