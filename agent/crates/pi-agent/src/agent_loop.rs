//! Agent loop — Rust port of `packages/agent/src/agent-loop.ts`.
//!
//! Streams assistant deltas, executes tool calls, and surfaces permission
//! decisions. Cancellation is honored via `StreamOptions::cancel`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::StreamExt;
use pi_ai::{
    stream_simple, AssistantMessageEvent, Content, Context, Message, StopReason, ToolResultMessage,
};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::instrument;

use crate::error::{AgentError, Result};
use crate::types::{AgentConfig, AgentEvent, AgentTool, AgentToolResult, PermissionDecision};

pub struct AgentRun {
    pub messages: Vec<Message>,
    pub stopped_at_turn_limit: bool,
}

/// What a turn ends with when the reply stream was cut off twice in a row (a dropped connection, not an answer).
pub const REPLY_CUT_OFF: &str = "My reply was cut off before it finished (the connection dropped twice), so I stopped there. \
    Send your message again and I'll pick it up.";

/// What a tool call cut off by the output limit is answered with: with a size, since "smaller pieces" alone got five
/// whole rewrites in a row (output-limit-mid-write, 2026-10-13).
pub fn cut_off(limit: u32) -> String {
    format!("Not run: your reply reached the output limit ({limit} tokens) before this call's arguments were finished, so \
        nothing was written or changed. Do it in smaller pieces, about {} lines per call: write the start of a big file, then \
        add the rest with edit, one piece per call. Don't send the whole thing again.", (limit / 20).max(5))
}

/// Gives every assistant tool call that has no result a "not run" error result, right after its message's other results.
/// Returns how many it added. The provider rejects a conversation with an unanswered call.
pub fn answer_unanswered(messages: &mut Vec<Message>) -> usize {
    let mut out = Vec::with_capacity(messages.len());
    let mut added = 0;
    let mut rest = std::mem::take(messages).into_iter().peekable();
    while let Some(m) = rest.next() {
        let calls: Vec<(String, String)> = match &m {
            Message::Assistant(a) => a.content.iter().filter_map(|c| match c {
                Content::ToolCall { id, name, .. } => Some((id.clone(), name.clone())),
                _ => None,
            }).collect(),
            _ => Vec::new(),
        };
        out.push(m);
        if calls.is_empty() { continue; }
        let mut answered = HashSet::new();
        while let Some(Message::ToolResult(tr)) = rest.peek() {
            answered.insert(tr.tool_call_id.clone());
            out.push(rest.next().unwrap());
        }
        for (id, name) in calls {
            if answered.contains(&id) { continue; }
            out.push(Message::ToolResult(ToolResultMessage {
                tool_call_id: id, tool_name: name, is_error: true, timestamp: pi_ai::now_ms(),
                content: vec![Content::text("Not run: the turn ended before this call finished.")],
            }));
            added += 1;
        }
    }
    *messages = out;
    added
}

#[instrument(skip(config, initial_prompt, events), fields(model = %config.model.id))]
pub async fn run_agent(
    config: &AgentConfig,
    initial_prompt: Message,
    events: Option<mpsc::UnboundedSender<AgentEvent>>,
) -> Result<AgentRun> {
    run_agent_with_history(config, vec![initial_prompt], events).await
}

/// The request a run's first step sends for this transcript, built as the loop builds it. A speculative turn (T-200)
/// sends it early and hands its stream back as `seeded_stream`, so the run starts exactly where that request left off.
pub fn first_request(config: &AgentConfig, mut messages: Vec<Message>) -> (Context, pi_ai::StreamOptions) {
    answer_unanswered(&mut messages);
    if conversation_chars(&messages) > config.compact_at_chars { compact(&mut messages, 6); }
    let ctx = Context {
        system_prompt: Some(config.system_prompt.clone()),
        system_suffix: (!config.system_suffix.is_empty()).then(|| config.system_suffix.clone()),
        trailing: (!config.trailing.is_empty()).then(|| config.trailing.clone()),
        messages: with_recent_images(&messages, config.keep_images),
        tools: config.tools.iter().map(|t| crate::types::tool_def(t.as_ref())).collect(),
    };
    let mut options = config.stream_options.clone();
    if options.reasoning.is_none() && config.thinking_level != pi_ai::ThinkingLevel::Off {
        options.reasoning = Some(config.thinking_level);
    }
    (ctx, options)
}

/// Continue a run with an existing transcript. Use this for `pi --resume`.
pub async fn run_agent_with_history(
    config: &AgentConfig,
    mut messages: Vec<Message>,
    events: Option<mpsc::UnboundedSender<AgentEvent>>,
) -> Result<AgentRun> {
    answer_unanswered(&mut messages);
    if let Some(last) = messages.last().cloned() {
        emit(&events, AgentEvent::UserMessage { message: last });
    }
    emit(&events, AgentEvent::AgentStart);

    let tool_index: HashMap<String, Arc<dyn AgentTool>> = config
        .tools
        .iter()
        .map(|t| (t.name().to_string(), t.clone()))
        .collect();
    let tool_defs: Vec<pi_ai::Tool> = config
        .tools
        .iter()
        .map(|t| crate::types::tool_def(t.as_ref()))
        .collect();

    let mut session_allowed: HashSet<String> = HashSet::new();
    let mut compacted_size = 0usize;
    let mut turn: u32 = 0;
    let mut stopped_at_turn_limit = false;
    let mut asked_before_reply = false;
    let mut seed = config.seeded_stream.as_ref().and_then(|s| s.take());

    'outer: while turn < config.max_turns {
        turn += 1;
        emit(&events, AgentEvent::TurnStart);

        // Compact rarely: past the limit, and only once the conversation has grown half again since the last
        // compaction (each one costs a prompt-cache miss; doing it every step costs more than it saves).
        let size = conversation_chars(&messages);
        if size > config.compact_at_chars && size > compacted_size + compacted_size / 2 {
            compact(&mut messages, 6);
            compacted_size = conversation_chars(&messages);
        }
        let ctx = Context {
            system_prompt: Some(config.system_prompt.clone()),
            system_suffix: (!config.system_suffix.is_empty()).then(|| config.system_suffix.clone()),
        trailing: (!config.trailing.is_empty()).then(|| config.trailing.clone()),
            messages: with_recent_images(&messages, config.keep_images),
            tools: tool_defs.clone(),
        };

        let mut options = config.stream_options.clone();
        if options.reasoning.is_none() && config.thinking_level != pi_ai::ThinkingLevel::Off {
            options.reasoning = Some(config.thinking_level);
        }

        let mut drops = 0;
        let (stop, msg) = loop {
            // A speculative turn's stream (T-200) stands in for the first call: what it buffered comes out at once.
            let mut stream = match seed.take() { Some(s) => s, None => stream_simple(&config.model, &ctx, &options).await? };

            let mut final_message: Option<pi_ai::AssistantMessage> = None;
            let mut stop = StopReason::Stop;
            let mut ended_early = false;

            while let Some(ev) = stream.next().await {
                let ev = match ev {
                    // A dropped connection: retry this step once rather than end the turn on half a reply.
                    Err(pi_ai::Error::InvalidResponse(m)) if m == pi_ai::STREAM_ENDED_EARLY => { ended_early = true; break; }
                    other => other?,
                };
                match ev {
                    AssistantMessageEvent::Done { reason, message } => {
                        stop = reason;
                        final_message = Some(message);
                        break;
                    }
                    AssistantMessageEvent::Error { reason: _, error } => {
                        let err_msg = error
                            .error_message
                            .clone()
                            .unwrap_or_else(|| "provider error".into());
                        return Err(AgentError::Other(err_msg));
                    }
                    AssistantMessageEvent::TextDelta { delta, .. } => {
                        emit(&events, AgentEvent::TextDelta { delta });
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        emit(&events, AgentEvent::ThinkingDelta { delta });
                    }
                    _ => {}
                }
            }

            if ended_early {
                drops += 1;
                if drops < 2 { continue; }
                return Err(AgentError::Other(REPLY_CUT_OFF.into()));
            }
            let Some(msg) = final_message else {
                return Err(AgentError::Other(
                    "provider stream produced no terminal event".into(),
                ));
            };
            break (stop, msg);
        };

        let assistant_message = Message::Assistant(msg.clone());
        messages.push(assistant_message.clone());
        emit(
            &events,
            AgentEvent::AssistantMessage {
                message: assistant_message,
            },
        );

        let tool_calls: Vec<(String, String, Value)> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                Content::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some((id.clone(), name.clone(), arguments.clone())),
                _ => None,
            })
            .collect();

        if !tool_calls.is_empty() && stop == StopReason::Length {
            // The reply hit the output limit inside a tool call: its arguments are cut off, so nothing runs. Answer every
            // call (a call without a result makes the provider reject the conversation from here on) and go on, so the
            // model can redo it in smaller pieces.
            let limit = config.stream_options.max_tokens.unwrap_or(config.model.max_tokens);
            for (id, name, args) in tool_calls {
                let content = vec![Content::text(cut_off(limit))];
                emit(&events, AgentEvent::ToolExecutionStart { tool_call_id: id.clone(), tool_name: name.clone(), args });
                emit(&events, AgentEvent::ToolExecutionEnd { tool_call_id: id.clone(), tool_name: name.clone(), is_error: true, content: content.clone() });
                messages.push(Message::ToolResult(ToolResultMessage { tool_call_id: id, tool_name: name, content, is_error: true, timestamp: pi_ai::now_ms() }));
            }
            emit(&events, AgentEvent::TurnEnd);
            continue 'outer;
        }
        if tool_calls.is_empty() || stop != StopReason::ToolUse {
            // A reply that would end the run: once per run, and only with a step left to act on, the caller may send the
            // model back with what's still unfinished, as a user message, instead of ending here (T-141).
            if tool_calls.is_empty() && !asked_before_reply && turn < config.max_turns
                && !matches!(stop, StopReason::Error | StopReason::Aborted) {
                if let Some(hook) = &config.before_reply {
                    asked_before_reply = true;
                    if let Some(text) = hook(messages.as_slice()).await {
                        let note = Message::User { content: vec![Content::text(text)], timestamp: pi_ai::now_ms() };
                        messages.push(note.clone());
                        emit(&events, AgentEvent::UserMessage { message: note });
                        emit(&events, AgentEvent::TurnEnd);
                        continue 'outer;
                    }
                }
            }
            emit(&events, AgentEvent::TurnEnd);
            break 'outer;
        }

        let mut any_terminate = !tool_calls.is_empty();
        for (id, name, args) in tool_calls {
            // Permission gate (only for tools that require it, and only once
            // per name per run if the user said "allow session").
            let tool_obj = tool_index.get(&name);
            let needs_perm = tool_obj.map(|t| t.requires_permission()).unwrap_or(false)
                && !session_allowed.contains(&name);
            if needs_perm {
                match config.permission.check(&name, &args).await {
                    PermissionDecision::Allow => {}
                    PermissionDecision::AllowSession => {
                        session_allowed.insert(name.clone());
                    }
                    PermissionDecision::Deny { reason } => {
                        emit(
                            &events,
                            AgentEvent::PermissionDenied {
                                tool_name: name.clone(),
                                reason: reason.clone(),
                            },
                        );
                        let tr = ToolResultMessage {
                            tool_call_id: id,
                            tool_name: name,
                            content: vec![Content::text(format!("permission denied: {reason}"))],
                            is_error: true,
                            timestamp: pi_ai::now_ms(),
                        };
                        messages.push(Message::ToolResult(tr));
                        any_terminate = false;
                        continue;
                    }
                }
            }

            emit(
                &events,
                AgentEvent::ToolExecutionStart {
                    tool_call_id: id.clone(),
                    tool_name: name.clone(),
                    args: args.clone(),
                },
            );
            let (content, is_error, terminate) = match tool_obj {
                Some(tool) => match tool.execute(&id, args).await {
                    Ok(AgentToolResult {
                        content,
                        details: _,
                        terminate,
                    }) => (content, false, terminate),
                    Err(e) => (vec![Content::text(format!("tool error: {e}"))], true, false),
                },
                None => (
                    vec![Content::text(format!("unknown tool: {name}"))],
                    true,
                    false,
                ),
            };
            if !terminate {
                any_terminate = false;
            }
            emit(
                &events,
                AgentEvent::ToolExecutionEnd {
                    tool_call_id: id.clone(),
                    tool_name: name.clone(),
                    is_error,
                    content: content.clone(),
                },
            );
            let tr = ToolResultMessage {
                tool_call_id: id,
                tool_name: name,
                content,
                is_error,
                timestamp: pi_ai::now_ms(),
            };
            messages.push(Message::ToolResult(tr));
        }
        if let Some((left, note)) = &config.wrap_up {
            if config.max_turns.saturating_sub(turn) == *left {
                if let Some(Message::ToolResult(tr)) = messages.last_mut() { tr.content.push(Content::text(note.clone())); }
            }
        }
        emit(&events, AgentEvent::TurnEnd);
        if any_terminate {
            break;
        }
    }

    if turn >= config.max_turns {
        stopped_at_turn_limit = true;
    }

    emit(
        &events,
        AgentEvent::AgentEnd {
            messages: messages.clone(),
        },
    );
    Ok(AgentRun {
        messages,
        stopped_at_turn_limit,
    })
}

fn emit(sink: &Option<mpsc::UnboundedSender<AgentEvent>>, ev: AgentEvent) {
    if let Some(s) = sink {
        let _ = s.send(ev);
    }
}

/// The conversation with all but the newest `keep` images in tool results replaced by a placeholder.
fn with_recent_images(messages: &[Message], keep: usize) -> Vec<Message> {
    let total: usize = messages.iter().map(|m| match m {
        Message::ToolResult(t) => t.content.iter().filter(|c| matches!(c, Content::Image { .. })).count(),
        _ => 0,
    }).sum();
    if total <= keep { return messages.to_vec(); }
    let mut drop = total - keep;
    messages.iter().map(|m| match m {
        Message::ToolResult(t) if drop > 0 && t.content.iter().any(|c| matches!(c, Content::Image { .. })) => {
            let mut t = t.clone();
            t.content = t.content.into_iter().map(|c| match c {
                Content::Image { .. } if drop > 0 => { drop -= 1; Content::Text { text: "[an earlier image, no longer shown]".into() } }
                other => other,
            }).collect();
            Message::ToolResult(t)
        }
        other => other.clone(),
    }).collect()
}

/// Characters of text in the conversation (images not counted: they're limited separately).
fn conversation_chars(messages: &[Message]) -> usize {
    fn content(c: &[Content]) -> usize {
        c.iter().map(|x| match x {
            Content::Text { text } => text.len(),
            Content::ToolCall { arguments, .. } => arguments.to_string().len(),
            _ => 0,
        }).sum()
    }
    messages.iter().map(|m| match m {
        Message::User { content: c, .. } => content(c),
        Message::Assistant(a) => content(&a.content),
        Message::ToolResult(t) => content(&t.content),
    }).sum()
}

const TRIMMED: &str = "… [shortened to save space; run the tool again if you need this]";

fn shorten(text: &str, keep: usize) -> String {
    if text.len() <= keep + TRIMMED.len() { return text.to_string(); }
    let cut = text.char_indices().map(|(i, _)| i).take_while(|&i| i <= keep).last().unwrap_or(0);
    format!("{}{TRIMMED}", &text[..cut])
}

/// Shortens everything but the last `keep` tool results and assistant messages: long tool outputs to their start,
/// and long string arguments of old tool calls (whole files passed to write/edit) to theirs.
fn compact(messages: &mut [Message], keep: usize) {
    let results: Vec<usize> = messages.iter().enumerate().filter(|(_, m)| matches!(m, Message::ToolResult(_))).map(|(i, _)| i).collect();
    let assistants: Vec<usize> = messages.iter().enumerate().filter(|(_, m)| matches!(m, Message::Assistant(_))).map(|(i, _)| i).collect();
    let old_results: std::collections::HashSet<usize> = results.iter().rev().skip(keep).copied().collect();
    let old_assistants: std::collections::HashSet<usize> = assistants.iter().rev().skip(keep).copied().collect();
    for (i, m) in messages.iter_mut().enumerate() {
        match m {
            Message::ToolResult(t) if old_results.contains(&i) => {
                for c in t.content.iter_mut() {
                    if let Content::Text { text } = c { if text.len() > 900 { *text = shorten(text, 600); } }
                }
            }
            Message::Assistant(a) if old_assistants.contains(&i) => {
                for c in a.content.iter_mut() {
                    if let Content::ToolCall { arguments, .. } = c {
                        if let Some(obj) = arguments.as_object_mut() {
                            for v in obj.values_mut() {
                                if let Some(s) = v.as_str() { if s.len() > 900 { *v = Value::String(shorten(s, 300)); } }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;
    use pi_ai::ToolResultMessage;

    fn result(text: &str) -> Message {
        Message::ToolResult(ToolResultMessage { tool_call_id: "c".into(), tool_name: "read".into(), content: vec![Content::Text { text: text.into() }], is_error: false, timestamp: 0 })
    }

    #[test]
    fn compaction_shortens_only_old_bulky_parts() {
        let big = "x".repeat(5000);
        let mut msgs: Vec<Message> = (0..14).map(|_| result(&big)).collect();
        msgs.push(result("short"));
        let before = conversation_chars(&msgs);
        compact(&mut msgs, 10);
        let after = conversation_chars(&msgs);
        assert!(after < before - 20_000, "{before} → {after}");   // the five oldest of fifteen shrink
        // The newest ten are untouched; older ones keep their start.
        if let Message::ToolResult(t) = &msgs[14] { assert!(matches!(&t.content[0], Content::Text { text } if text == "short")); }
        if let Message::ToolResult(t) = &msgs[13] { assert!(matches!(&t.content[0], Content::Text { text } if text.len() == 5000)); }
        if let Message::ToolResult(t) = &msgs[0] { assert!(matches!(&t.content[0], Content::Text { text } if text.starts_with("xxx") && text.ends_with("need this]"))); }
    }
}
