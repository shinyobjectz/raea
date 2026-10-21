use std::sync::Arc;

use async_trait::async_trait;
use pi_ai::{Content, Message, Model, StreamOptions, ThinkingLevel, Tool};
use serde_json::Value;

/// A live result returned from a tool execution.
#[derive(Debug, Clone, Default)]
pub struct AgentToolResult {
    pub content: Vec<Content>,
    pub details: Value,
    pub terminate: bool,
}

impl AgentToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(s)],
            details: Value::Null,
            terminate: false,
        }
    }
}

/// Permission outcome for a tool call. Returned by a [`PermissionPolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    /// Allow this call and remember the choice for the rest of the run.
    AllowSession,
    /// Deny this call; emit an error tool result with `reason`.
    Deny {
        reason: String,
    },
}

/// User-supplied permission policy. Implementations may prompt interactively or
/// consult a static allow-list.
#[async_trait]
pub trait PermissionPolicy: Send + Sync {
    async fn check(&self, tool_name: &str, args: &Value) -> PermissionDecision;
}

/// Always-allow policy — useful for tests and non-interactive runs.
pub struct AllowAllPolicy;

#[async_trait]
impl PermissionPolicy for AllowAllPolicy {
    async fn check(&self, _tool_name: &str, _args: &Value) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

/// Tool execution trait — analog of `AgentTool.execute` in TS.
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn label(&self) -> &str {
        self.name()
    }
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    /// Whether the tool requires user permission by default. Read-only tools
    /// (`read`, `ls`, `grep`, `glob`) return `false`; mutating or side-effecting
    /// tools (`bash`, `write`, `edit`) return `true`.
    fn requires_permission(&self) -> bool {
        false
    }
    async fn execute(&self, tool_call_id: &str, args: Value) -> Result<AgentToolResult, String>;
}

pub fn tool_def(t: &dyn AgentTool) -> Tool {
    Tool {
        name: t.name().to_string(),
        description: t.description().to_string(),
        parameters: t.parameters(),
    }
}

/// What the loop asks before a reply without tool calls ends the run: given the conversation so far, the text of one
/// more user message (what's still unfinished), or None to let the reply stand. the host app addition (T-141).
pub type BeforeReply = Arc<dyn Fn(&[Message]) -> futures::future::BoxFuture<'static, Option<String>> + Send + Sync>;

/// Agent configuration controlling the loop.
/// A reply stream for one run's first step (T-200). Taken once; behind a lock so `AgentConfig` stays `Clone`.
#[derive(Clone)]
pub struct SeededStream(Arc<std::sync::Mutex<Option<pi_ai::AssistantMessageEventStream>>>);

impl SeededStream {
    pub fn new(stream: pi_ai::AssistantMessageEventStream) -> Self { Self(Arc::new(std::sync::Mutex::new(Some(stream)))) }
    pub fn take(&self) -> Option<pi_ai::AssistantMessageEventStream> { self.0.lock().unwrap().take() }
}

#[derive(Clone)]
pub struct AgentConfig {
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub stream_options: StreamOptions,
    pub max_turns: u32,
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub system_prompt: String,
    pub permission: Arc<dyn PermissionPolicy>,
    /// Only the newest this-many images in tool results are sent to the model; older ones become a short
    /// placeholder (they stay in the conversation). the host app addition.
    pub keep_images: usize,
    /// When the conversation passes this many characters, older tool results and long tool-call arguments are
    /// shortened in one go (one prompt-cache miss, then a smaller stable prefix). the host app addition.
    pub compact_at_chars: usize,
    /// This turn's own instructions, sent after `system_prompt` and outside its cache breakpoint (pi-ai
    /// Context::system_suffix). the host app addition.
    pub system_suffix: String,
    /// This turn's moving lines, after the conversation and outside the cache (pi-ai Context::trailing, T-251).
    pub trailing: String,
    /// The first step's reply stream, already started elsewhere (a speculative voice turn, T-200): the first step reads
    /// it instead of calling the model (what it buffered comes out at once, the rest live), then the run goes on as usual
    /// (running its tool calls). the host app addition.
    pub seeded_stream: Option<SeededStream>,
    /// With this many steps left before `max_turns`, the text is added to that step's last tool result, once, so a long
    /// job ends with something delivered instead of stopping mid-way at the limit. the host app addition.
    pub wrap_up: Option<(u32, String)>,
    /// Asked once per run, when the model replies without calling a tool and a step is left: Some(text) adds the text
    /// as a user message and the run goes on; None, or any later reply, ends the run as before. the host app addition (T-141).
    pub before_reply: Option<BeforeReply>,
}

impl AgentConfig {
    pub fn new(model: Model, system_prompt: impl Into<String>) -> Self {
        Self {
            model,
            thinking_level: ThinkingLevel::Off,
            stream_options: StreamOptions::default(),
            max_turns: 32,
            tools: Vec::new(),
            system_prompt: system_prompt.into(),
            permission: Arc::new(AllowAllPolicy),
            keep_images: usize::MAX,
            compact_at_chars: usize::MAX,
            system_suffix: String::new(),
            trailing: String::new(),
            seeded_stream: None,
            wrap_up: None,
            before_reply: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<Arc<dyn AgentTool>>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_max_turns(mut self, n: u32) -> Self {
        self.max_turns = n;
        self
    }

    pub fn with_permission(mut self, p: Arc<dyn PermissionPolicy>) -> Self {
        self.permission = p;
        self
    }

    pub fn with_thinking(mut self, level: ThinkingLevel) -> Self {
        self.thinking_level = level;
        self
    }
}

/// Events emitted by the agent loop, mirroring `AgentEvent` in TS.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<Message>,
    },
    TurnStart,
    TurnEnd,
    AssistantMessage {
        message: Message,
    },
    UserMessage {
        message: Message,
    },
    /// Streaming text chunk while the assistant types.
    TextDelta {
        delta: String,
    },
    /// Streaming thinking chunk.
    ThinkingDelta {
        delta: String,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
        content: Vec<Content>,
    },
    /// Permission denied for a tool call (the loop appended an error tool result).
    PermissionDenied {
        tool_name: String,
        reason: String,
    },
}
