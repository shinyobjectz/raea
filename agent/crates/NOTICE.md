# Third-party code

`pi-ai/` and `pi-agent/` are copied from the Rust port of pi, https://github.com/nktkt/pi at commit
0808c75 (1.2.0, 2026-05-12), itself a port of https://github.com/earendil-works/pi. MIT License,
see `LICENSE-pi-rust-port`. Changes in this copy: removed the `bash` and `web_fetch` tools; the
workspace's dependency features are trimmed for iOS.
- `pi-ai/src/providers/anthropic.rs`: also reads input and cache token counts from `message_delta`
  (some Anthropic-compatible APIs report them there).

- pi-agent: `AgentConfig::keep_images` — only the newest N tool-result images are sent to the model; older ones are replaced by a one-line placeholder (they stay in the saved conversation). Added for the host app.
- pi-agent: `AgentConfig::compact_at_chars` — past that size, tool results and long tool-call string arguments older than the last 6 are shortened in one pass, at most once per 50% growth (keeps the prompt-cache prefix stable between compactions). Added for the host app.
