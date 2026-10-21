use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anthropic prompt-cache retention hint. When non-`None`, the Anthropic
/// provider stamps `cache_control` markers on the system prompt, the last tool
/// definition, and the last text block of the last user message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheRetention {
    #[default]
    None,
    Short,
    Long,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        thinking_signature: Option<String>,
    },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        #[serde(default)]
        arguments: Value,
    },
}

impl Content {
    pub fn text(s: impl Into<String>) -> Self {
        Content::Text { text: s.into() }
    }
    pub fn as_text(&self) -> Option<&str> {
        if let Content::Text { text } = self {
            Some(text)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub cost: Cost,
}

/// Accumulated USD cost for a single assistant message, broken down by
/// input / output / cache-read / cache-write token classes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub total: f64,
}

/// Per-million-token USD pricing for a model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPricing {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
}

impl Usage {
    /// Compute the USD cost of this usage given a model's pricing.
    pub fn compute_cost(&self, p: &ModelPricing) -> Cost {
        let input = self.input as f64 * p.input / 1_000_000.0;
        let output = self.output as f64 * p.output / 1_000_000.0;
        let cache_read = self.cache_read as f64 * p.cache_read / 1_000_000.0;
        let cache_write = self.cache_write as f64 * p.cache_write / 1_000_000.0;
        Cost {
            input,
            output,
            cache_read,
            cache_write,
            total: input + output + cache_read + cache_write,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum Message {
    #[serde(rename = "user")]
    User {
        content: Vec<Content>,
        #[serde(default = "now_ms")]
        timestamp: i64,
    },
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "toolResult")]
    ToolResult(ToolResultMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<Content>,
    pub api: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub usage: Usage,
    pub stop_reason: StopReason,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_message: Option<String>,
    #[serde(default = "now_ms")]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<Content>,
    pub is_error: bool,
    #[serde(default = "now_ms")]
    pub timestamp: i64,
}

impl Message {
    pub fn user_text(s: impl Into<String>) -> Self {
        Message::User {
            content: vec![Content::text(s)],
            timestamp: now_ms(),
        }
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Tool schema definition matching the unified pi-ai Tool interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool parameters (object schema).
    pub parameters: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub system_prompt: Option<String>,
    /// Instructions that change now and then (the date, the projects), sent after the system prompt as their own
    /// cached block. the host app addition.
    pub system_suffix: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    /// This turn's moving lines (what's on screen, a spoken turn's note), sent after the conversation, past the last
    /// cache breakpoint: a new screen doesn't cost the cached history (T-251; before, they sat ahead of it). the host app
    /// addition.
    pub trailing: Option<String>,
}

impl Context {
    /// The system prompt, the suffix and this turn's trailing lines as one text, for providers without separate blocks.
    pub fn full_system(&self) -> Option<String> {
        let parts: Vec<&str> = [&self.system_prompt, &self.system_suffix, &self.trailing].into_iter().flatten().map(String::as_str).collect();
        (!parts.is_empty()).then(|| parts.concat())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub api_key: Option<String>,
    pub reasoning: Option<ThinkingLevel>,
    /// Cancellation token honored by the provider HTTP client and SSE loop.
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// Override base URL (used by OpenAI-compatible passthrough providers).
    pub base_url: Option<String>,
    /// Optional custom request headers.
    pub headers: std::collections::BTreeMap<String, String>,
    /// Anthropic prompt-cache marker mode. Defaults to `None` (no markers).
    pub cache_retention: CacheRetention,
}

/// Model descriptor — analogous to the Model<TApi> interface in pi-ai.
#[derive(Debug, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    pub base_url: String,
    pub reasoning: bool,
    pub context_window: u32,
    pub max_tokens: u32,
    pub pricing: ModelPricing,
}

impl Model {
    /// Anthropic Claude Sonnet 4.6.
    pub fn anthropic_claude_sonnet_4_6() -> Self {
        Self {
            id: "claude-sonnet-4-6".into(),
            name: "Claude Sonnet 4.6".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reasoning: true,
            context_window: 200_000,
            max_tokens: 8_192,
            pricing: ModelPricing {
                input: 3.0,
                output: 15.0,
                cache_read: 0.30,
                cache_write: 3.75,
            },
        }
    }

    pub fn anthropic_claude_opus_4_7() -> Self {
        Self {
            id: "claude-opus-4-7".into(),
            name: "Claude Opus 4.7".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            reasoning: true,
            context_window: 200_000,
            max_tokens: 8_192,
            pricing: ModelPricing {
                input: 15.0,
                output: 75.0,
                cache_read: 1.50,
                cache_write: 18.75,
            },
        }
    }

    pub fn openai_gpt_4o_mini() -> Self {
        Self {
            id: "gpt-4o-mini".into(),
            name: "GPT-4o mini".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            reasoning: false,
            context_window: 128_000,
            max_tokens: 16_384,
            pricing: ModelPricing {
                input: 0.15,
                output: 0.60,
                cache_read: 0.0,
                cache_write: 0.0,
            },
        }
    }

    pub fn openai_gpt_4o() -> Self {
        Self {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            api: "openai-completions".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            reasoning: false,
            context_window: 128_000,
            max_tokens: 16_384,
            pricing: ModelPricing {
                input: 2.50,
                output: 10.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
        }
    }

    /// OpenAI GPT-5 — reasoning model accessed via the Responses API.
    pub fn openai_gpt_5() -> Self {
        Self {
            id: "gpt-5".into(),
            name: "GPT-5".into(),
            api: "openai-responses".into(),
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            reasoning: true,
            context_window: 200_000,
            max_tokens: 16_384,
            // Pricing TBD — placeholder zeros until OpenAI publishes the rate card.
            pricing: ModelPricing::default(),
        }
    }

    pub fn gemini_2_0_flash() -> Self {
        Self {
            id: "gemini-2.0-flash".into(),
            name: "Gemini 2.0 Flash".into(),
            api: "google-generative-ai".into(),
            provider: "google".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            reasoning: false,
            context_window: 1_000_000,
            max_tokens: 8_192,
            pricing: ModelPricing {
                input: 0.10,
                output: 0.40,
                cache_read: 0.0,
                cache_write: 0.0,
            },
        }
    }

    /// OpenAI-compatible passthrough: any provider whose API matches OpenAI Chat
    /// Completions (OpenRouter, Together, Groq, Cerebras, DeepSeek, Fireworks,
    /// xAI, etc.). Construct via `openai_compat("groq", "...", "...", ...)`
    /// or by overriding `base_url`.
    pub fn openai_compat(
        provider: impl Into<String>,
        id: impl Into<String>,
        base_url: impl Into<String>,
        context_window: u32,
        max_tokens: u32,
    ) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            api: "openai-completions".into(),
            provider: provider.into(),
            base_url: base_url.into(),
            reasoning: false,
            context_window,
            max_tokens,
            pricing: ModelPricing::default(),
        }
    }
}

/// Events emitted by streaming completions, matching `AssistantMessageEvent` in pi-ai.
#[derive(Debug, Clone)]
pub enum AssistantMessageEvent {
    Start,
    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: String,
    },
    TextEnd {
        content_index: usize,
        content: String,
    },
    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
    },
    ThinkingEnd {
        content_index: usize,
        content: String,
    },
    ToolCallStart {
        content_index: usize,
        id: String,
        name: String,
    },
    ToolCallDelta {
        content_index: usize,
        delta: String,
    },
    ToolCallEnd {
        content_index: usize,
        id: String,
        name: String,
        arguments: Value,
    },
    Done {
        reason: StopReason,
        message: AssistantMessage,
    },
    Error {
        reason: StopReason,
        error: AssistantMessage,
    },
}
