//! `pi-ai` — Unified multi-provider LLM API.
//!
//! Rust port of the TypeScript package `@earendil-works/pi-ai`. The original
//! project supports many providers; this port focuses on the two most common
//! agent backends: Anthropic Messages and OpenAI Chat Completions. Both share
//! the same `Message`/`Tool`/`AssistantMessageEvent` types so the agent loop
//! is provider-agnostic.

pub mod error;
pub mod providers;
pub mod retry;
pub mod stream;
pub mod types;

pub use error::{Error, Result};

/// The `InvalidResponse` message for a reply stream that ended before the reply did (no stop reason, no message_stop):
/// a dropped connection, not a finished answer. pi-agent retries the step once on it.
pub const STREAM_ENDED_EARLY: &str = "the reply stream ended before the reply finished";
pub use providers::{
    anthropic::AnthropicProvider, google::GoogleProvider, openai::OpenAiProvider,
    openai_responses::OpenAiResponsesProvider, Provider,
};
pub use stream::AssistantMessageEventStream;
pub use types::{
    now_ms, AssistantMessage, AssistantMessageEvent, CacheRetention, Content, Context, Cost,
    Message, Model, ModelPricing, StopReason, StreamOptions, ThinkingLevel, Tool,
    ToolResultMessage, Usage,
};

/// Entry point that mirrors `streamSimple()` in pi-ai TS: pick the provider
/// implementation from `model.api` and return a stream of message events.
///
/// `openai-completions` covers OpenAI and every OpenAI-compatible passthrough
/// (OpenRouter, Together, Groq, Cerebras, DeepSeek, Fireworks, xAI, etc.) —
/// just set `Model::base_url` (or `StreamOptions::base_url`) accordingly.
pub async fn stream_simple(
    model: &Model,
    context: &Context,
    options: &StreamOptions,
) -> Result<AssistantMessageEventStream> {
    match model.api.as_str() {
        "anthropic-messages" => {
            AnthropicProvider::new()
                .stream(model, context, options)
                .await
        }
        "openai-completions" => OpenAiProvider::new().stream(model, context, options).await,
        "openai-responses" => {
            OpenAiResponsesProvider::new()
                .stream(model, context, options)
                .await
        }
        "google-generative-ai" => GoogleProvider::new().stream(model, context, options).await,
        other => Err(Error::UnsupportedProvider(other.into())),
    }
}
