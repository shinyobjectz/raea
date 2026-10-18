# agent

The agent runtime void's agent is built on, and the pieces around it. It is a Cargo workspace of three crates, plus
the tracing crate beside it.

- `crates/pi-ai`: one streaming API over model providers (Anthropic Messages, OpenAI, Gemini, any OpenAI-compatible
  endpoint).
- `crates/pi-agent`: the agent loop: streaming turns, tool calls, a permission policy, context compaction.
- `crates/raea-spec`: a Gherkin parser (features, rules, backgrounds, scenarios, tags, tables, docstrings) for eval
  scenarios and prompt rules.
- `otel/` (`raea-otel`): traces of each turn as OpenTelemetry spans, written as JSONL beside the run and sent over
  OTLP/HTTP to a collector such as Arize Phoenix.

`pi-ai` and `pi-agent` are a modified copy of the Rust port of pi (MIT); `crates/NOTICE.md` lists the changes. void
depends on `raea-otel` by path.

Status: in use by void.
