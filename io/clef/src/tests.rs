use std::cell::RefCell;

use raea_core_ask::Question;
use serde_json::{Value, json};

use crate::client::{Call, Media, Transport};
use crate::route::{self, Route};

/// Canned replies by URL ending, and every URL asked.
struct Fake {
    replies: Vec<(&'static str, u16, Value)>,
    seen: RefCell<Vec<String>>,
}

impl Transport for Fake {
    fn post(&self, url: &str, _bearer: Option<&str>, _body: &Value) -> Result<(u16, Value), String> {
        self.seen.borrow_mut().push(url.to_string());
        self.replies.iter().find(|(end, ..)| url.ends_with(end)).map(|(_, c, j)| (*c, j.clone())).ok_or_else(|| format!("{url}: connection refused"))
    }
}

fn setup() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: set once, before any test reads it, to the same value for every test
    ONCE.call_once(|| unsafe { std::env::set_var("RAEA_ASK_DIR", std::env::temp_dir().join(format!("raea-clef-test-{}", std::process::id()))) });
}

/// A call unique to its test (the cache is shared), with one question and a little media.
fn call(test: &str) -> Call {
    setup();
    Call { state: format!("a test piece: {test}"), questions: vec![Question::plain("q", "Is it loud?")], media: Media { sounds: vec![vec![1, 2, 3]], ..Default::default() } }
}

fn routes() -> Vec<Route> {
    vec![Route::App("http://127.0.0.1:9".into()), Route::Agent { api: "https://api.test".into(), key: "k".into() }, Route::Own { account: "a".into(), token: "t".into() }]
}

const ANSWER: &str = r#"{"success": true, "result": {"answers": {"q": {"noul": 0.7}}, "usage": {"input_tokens": 10000}}}"#;

#[test]
fn each_route_that_cant_answer_hands_on_to_the_next() {
    let f = Fake {
        replies: vec![
            ("/host/ask", 402, json!({"error": "plan_required", "message": "checks come with Pro and Studio"})),
            ("/v1/clef/ask", 403, json!({"error": "agent_scope", "message": "this key can't ask"})),
            ("/clef-omni", 200, serde_json::from_str(ANSWER).unwrap()),
        ],
        seen: RefCell::default(),
    };
    let a = route::ask(&f, &call("hand-on"), &routes()).unwrap();
    assert_eq!((a.route.as_str(), a.answers["q"]["noul"].as_f64()), ("your Cloudflare account", Some(0.7)));
    assert!((a.cost - 0.0015).abs() < 1e-9, "{}", a.cost);
    assert_eq!(f.seen.borrow().len(), 3);
}

#[test]
fn a_spent_pool_stops_unless_the_person_has_their_own_keys() {
    let spent = || Fake { replies: vec![("/host/ask", 402, json!({"error": "pool_spent", "message": "this month's checks are used up"}))], seen: RefCell::default() };
    let e = route::ask(&spent(), &call("spent"), &routes()[..1]).unwrap_err();
    assert!(matches!(&e, crate::Fail::Spent(m) if m.contains("used up")), "{e:?}");
    let mut f = spent();
    f.replies.push(("/clef-omni", 200, serde_json::from_str(ANSWER).unwrap()));
    let a = route::ask(&f, &call("spent-own"), &[routes()[0].clone(), routes()[2].clone()]).unwrap();
    assert_eq!(a.route, "your Cloudflare account");
}

#[test]
fn the_plan_answers_with_whats_left_and_a_second_ask_costs_nothing() {
    let f = Fake { replies: vec![("/host/ask", 200, json!({"answers": {"q": {"noul": 0.2}}, "cost_usd": 0.004, "cached": false, "pool_left_usd": 1.62}))], seen: RefCell::default() };
    let c = call("plan");
    let a = route::ask(&f, &c, &routes()).unwrap();
    assert_eq!((a.route.as_str(), a.cost, a.pool_left), ("your raea plan, through the app", 0.004, Some(1.62)));
    let again = route::ask(&f, &c, &routes()).unwrap();
    assert!(again.cached && again.cost == 0.0);
    assert_eq!(f.seen.borrow().len(), 1, "the second came from the cache");
}

#[test]
fn with_nothing_open_it_says_how_to_get_one() {
    let f = Fake { replies: vec![], seen: RefCell::default() };
    let e = route::ask(&f, &call("none"), &[]).unwrap_err().to_string();
    assert!(e.contains("RAEA_AGENT_KEY") && e.contains("CLOUDFLARE_API_TOKEN"), "{e}");
    let e = route::ask(&f, &call("refused"), &routes()[..1]).unwrap_err().to_string();
    assert!(e.contains("connection refused") && e.contains("Pro or Studio"), "{e}");
}

#[test]
fn a_call_that_breaks_the_limits_is_refused_before_anything_is_sent() {
    let f = Fake { replies: vec![], seen: RefCell::default() };
    let mut c = call("limits");
    c.media = Media::default();
    assert!(route::ask(&f, &c, &routes()).unwrap_err().to_string().contains("look at or listen to"));
    let mut c = call("too-many");
    c.questions = (0..65).map(|i| Question::plain(&format!("q{i}"), "Is it loud?")).collect();
    assert!(route::ask(&f, &c, &routes()).unwrap_err().to_string().contains("1 to 64"));
    assert!(f.seen.borrow().is_empty());
}

#[test]
fn workers_ai_gets_data_urls_and_raea_api_bare_base64() {
    let c = Call { state: "s".into(), questions: vec![Question::plain("q", "Is it loud?")], media: Media { stills: vec![vec![0xff]], videos: vec![], sounds: vec![vec![1]] } };
    let clef = c.clef_body();
    assert_eq!(clef["images"][0], "data:image/jpeg;base64,/w==");
    assert_eq!(clef["audio"][0], "data:audio/mpeg;base64,AQ==");
    assert!(clef.get("videos").is_none());
    assert_eq!(c.api_body()["audio"][0], "AQ==");
    assert_eq!(clef["questions"]["q"]["type"], "noul");
}
