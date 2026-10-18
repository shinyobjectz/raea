//! Integration test for Anthropic prompt-cache markers.
//!
//! Asserts that the Anthropic request body emits `cache_control` on the
//! system prompt block, the last tool definition, and the last text block
//! of the last user message when `StreamOptions::cache_retention` is
//! non-`None`. With the default (`None`), no markers are emitted and the
//! system prompt stays a plain string for backward compatibility.

use pi_ai::providers::anthropic::build_request_body;
use pi_ai::{CacheRetention, Content, Context, Message, Model, StreamOptions, Tool};
use serde_json::json;

fn ctx_with_one_of_each() -> Context {
    Context {
        system_prompt: Some("you are pi.".into()),
        system_suffix: None,
        trailing: None,
        messages: vec![
            Message::User {
                content: vec![Content::text("hello")],
                timestamp: 0,
            },
            Message::user_text("second turn"),
        ],
        tools: vec![
            Tool {
                name: "first".into(),
                description: "first tool".into(),
                parameters: json!({"type": "object", "properties": {}}),
            },
            Tool {
                name: "last".into(),
                description: "last tool".into(),
                parameters: json!({"type": "object", "properties": {}}),
            },
        ],
    }
}

#[test]
fn cache_retention_short_marks_system_tools_and_last_user() {
    let model = Model::anthropic_claude_sonnet_4_6();
    let ctx = ctx_with_one_of_each();
    let opt = StreamOptions {
        cache_retention: CacheRetention::Short,
        ..Default::default()
    };
    let body = build_request_body(&model, &ctx, &opt);

    // System prompt is now an array, first block has cache_control: ephemeral.
    let sys = &body["system"];
    assert!(sys.is_array(), "system should be an array form");
    assert_eq!(sys[0]["type"], "text");
    assert_eq!(sys[0]["text"], "you are pi.");
    assert_eq!(sys[0]["cache_control"]["type"], "ephemeral");
    assert!(sys[0]["cache_control"].get("ttl").is_none());

    // Tools: last entry has cache_control, earlier entries do not.
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools[0].get("cache_control").is_none());
    assert_eq!(tools[1]["cache_control"]["type"], "ephemeral");

    // Last user message's last text block has cache_control.
    let messages = body["messages"].as_array().unwrap();
    let last_user = messages
        .iter()
        .rfind(|m| m["role"] == "user")
        .expect("last user");
    let blocks = last_user["content"].as_array().unwrap();
    let last_text = blocks
        .iter()
        .rfind(|b| b["type"] == "text")
        .expect("text block");
    assert_eq!(last_text["cache_control"]["type"], "ephemeral");

    // Earlier user messages should not be marked.
    let first_user = messages
        .iter()
        .find(|m| m["role"] == "user")
        .expect("first user");
    let first_blocks = first_user["content"].as_array().unwrap();
    assert!(first_blocks[0].get("cache_control").is_none());
}

#[test]
fn cache_retention_long_adds_ttl_1h() {
    let model = Model::anthropic_claude_sonnet_4_6();
    let ctx = ctx_with_one_of_each();
    let opt = StreamOptions {
        cache_retention: CacheRetention::Long,
        ..Default::default()
    };
    let body = build_request_body(&model, &ctx, &opt);
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(body["system"][0]["cache_control"]["ttl"], "1h");
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.last().unwrap()["cache_control"]["ttl"], "1h");
}

#[test]
fn cache_retention_none_keeps_legacy_shape() {
    let model = Model::anthropic_claude_sonnet_4_6();
    let ctx = ctx_with_one_of_each();
    let opt = StreamOptions::default();
    assert_eq!(opt.cache_retention, CacheRetention::None);
    let body = build_request_body(&model, &ctx, &opt);
    // System stays a plain string.
    assert_eq!(body["system"], json!("you are pi."));
    // Tools have no cache_control.
    for t in body["tools"].as_array().unwrap() {
        assert!(t.get("cache_control").is_none());
    }
    // Last user message text has no cache_control.
    let messages = body["messages"].as_array().unwrap();
    let last_user = messages.iter().rfind(|m| m["role"] == "user").unwrap();
    for b in last_user["content"].as_array().unwrap() {
        assert!(b.get("cache_control").is_none());
    }
}

#[test]
fn the_suffix_is_cached_and_a_turns_moving_lines_follow_the_conversation() {
    // T-251: the slow-moving suffix (the date, the projects) is its own cached block; what's on screen and a spoken
    // turn's note go after the last message's breakpoint, so a new screen doesn't cost the cached history.
    let model = Model::anthropic_claude_sonnet_4_6();
    let mut ctx = ctx_with_one_of_each();
    ctx.system_suffix = Some("\n\nToday is Sunday.".into());
    ctx.trailing = Some("\n\nThe user is on the Brief card.".into());
    let cached = build_request_body(&model, &ctx, &StreamOptions { cache_retention: CacheRetention::Short, ..Default::default() });
    let sys = cached["system"].as_array().expect("blocks");
    assert_eq!(sys.len(), 2);
    assert_eq!((sys[0]["text"].as_str(), sys[0]["cache_control"]["type"].as_str()), (Some("you are pi."), Some("ephemeral")));
    assert_eq!((sys[1]["text"].as_str(), sys[1]["cache_control"]["type"].as_str()), (Some("\n\nToday is Sunday."), Some("ephemeral")));
    let last = cached["messages"].as_array().unwrap().last().unwrap()["content"].as_array().unwrap().clone();
    assert_eq!((last[0]["text"].as_str(), last[0]["cache_control"]["type"].as_str()), (Some("second turn"), Some("ephemeral")));
    assert_eq!(last[1], json!({"type": "text", "text": "\n\nThe user is on the Brief card."}), "after the breakpoint, unmarked");
    assert!(!cached["system"].to_string().contains("Brief card"));
    // Without caching: the prompt and suffix as one text, the moving lines still after the conversation.
    let plain = build_request_body(&model, &ctx, &StreamOptions::default());
    assert_eq!(plain["system"], json!("you are pi.\n\nToday is Sunday."));
    assert!(plain["messages"].to_string().contains("Brief card"));
}

#[test]
fn a_tool_steps_breakpoint_is_on_its_tool_result() {
    // T-251: in a tool step the last user-role message holds only tool results; it carries the breakpoint, so the
    // step after reads the turn so far from the cache.
    use pi_ai::{AssistantMessage, StopReason, ToolResultMessage};
    let model = Model::anthropic_claude_sonnet_4_6();
    let mut ctx = ctx_with_one_of_each();
    ctx.messages.push(Message::Assistant(AssistantMessage {
        content: vec![Content::ToolCall { id: "t1".into(), name: "first".into(), arguments: json!({}) }],
        api: "anthropic-messages".into(), provider: "anthropic".into(), model: "m".into(), usage: Default::default(),
        stop_reason: StopReason::ToolUse, error_message: None, timestamp: 0 }));
    ctx.messages.push(Message::ToolResult(ToolResultMessage { tool_call_id: "t1".into(), tool_name: "first".into(),
        content: vec![Content::text("done")], is_error: false, timestamp: 0 }));
    ctx.trailing = Some("\n\nOn screen: the shot list.".into());
    let body = build_request_body(&model, &ctx, &StreamOptions { cache_retention: CacheRetention::Short, ..Default::default() });
    let messages = body["messages"].as_array().unwrap();
    let last = messages.last().unwrap()["content"].as_array().unwrap();
    assert_eq!(last[0]["type"], "tool_result");
    assert_eq!(last[0]["cache_control"]["type"], "ephemeral");
    assert_eq!(last[1]["text"], "\n\nOn screen: the shot list.");
    let marks = body.to_string().matches("\"cache_control\"").count();
    assert!(marks <= 4, "at most 4 breakpoints, found {marks}");
}
