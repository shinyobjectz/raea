# pi-agent

Agent runtime with tool calling. Part of the
[`pi`](https://github.com/nktkt/pi) agent harness — a Rust port of
[`@earendil-works/pi-agent-core`](https://github.com/earendil-works/pi).

Builds on [`pi-ai`](https://crates.io/crates/pi-ai) and adds:

- `run_agent` / `run_agent_with_history` — streaming agent loop with sequential
  tool execution.
- `AgentTool` trait — implement once, plug in via `AgentConfig::with_tools`.
- `PermissionPolicy` trait with `Allow` / `AllowSession` / `Deny` —
  tools that flag `requires_permission()` (e.g. `bash`, `write`, `edit`) are
  gated.
- Streaming `AgentEvent::TextDelta` / `ThinkingDelta` /
  `ToolExecutionStart` / `ToolExecutionEnd` events through an
  `mpsc::UnboundedSender`.
- Built-in tools under `pi_agent::tools::*`: `read`, `write`, `edit`,
  `bash`, `ls`, `grep`, `glob_tool`, `web_fetch`, `todo`. Pre-bundled as
  `pi_agent::tools::default_tools()`.

## Quick start

```rust
use pi_agent::{run_agent, tools::default_tools, AgentConfig};
use pi_ai::{Message, Model};

# tokio_test::block_on(async {
let cfg = AgentConfig::new(
    Model::anthropic_claude_sonnet_4_6(),
    "You are a helpful coding assistant.",
)
.with_tools(default_tools())
.with_max_turns(8);

let user = Message::user_text("List the .rs files in the current directory.");
let run = run_agent(&cfg, user, None).await.unwrap();
println!("{} messages", run.messages.len());
# });
```

## License

MIT
