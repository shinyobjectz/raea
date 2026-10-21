# pi-ai

Unified multi-provider LLM API. Part of the [`pi`](https://github.com/nktkt/pi)
agent runtime — a Rust port of
[`@earendil-works/pi-ai`](https://github.com/earendil-works/pi).

## Providers

| API string | Implemented by | Endpoints |
|------------|---------------|-----------|
| `anthropic-messages` | `AnthropicProvider` | Anthropic Messages SSE |
| `openai-completions` | `OpenAiProvider` | OpenAI Chat Completions + any OpenAI-compatible base URL (OpenRouter, Together, Groq, Cerebras, DeepSeek, Fireworks, xAI, etc.) |
| `google-generative-ai` | `GoogleProvider` | Gemini `streamGenerateContent?alt=sse` |

All three stream via SSE, emit per-block `text_delta` / `thinking_delta` /
`toolcall_delta` events, share a retry helper with `Retry-After` parsing,
and honor a `CancellationToken` for mid-stream abort.

## Quick start

```rust
use pi_ai::{stream_simple, Context, Message, Model, StreamOptions};
use futures::StreamExt;

# tokio_test::block_on(async {
let model = Model::anthropic_claude_sonnet_4_6();
let ctx = Context {
    system_prompt: Some("You are a helpful assistant.".into()),
    system_suffix: None,
    messages: vec![Message::user_text("Say hi in one word.")],
    trailing: None,
    tools: vec![],
};
let mut events = stream_simple(&model, &ctx, &StreamOptions::default()).await.unwrap();
while let Some(ev) = events.next().await {
    println!("{:?}", ev.unwrap());
}
# });
```

`ANTHROPIC_API_KEY` (or `OPENAI_API_KEY` / `GOOGLE_API_KEY` /
`GEMINI_API_KEY`) is read from the environment unless
`StreamOptions::api_key` is set.

## OpenAI-compatible passthrough

```rust
use pi_ai::Model;
let m = Model::openai_compat(
    "openrouter",
    "anthropic/claude-3.5-sonnet",
    "https://openrouter.ai/api/v1",
    200_000, 8_192,
);
```

Set `OPENAI_API_KEY` to the provider's key, or pass `api_key` directly.

## License

MIT
