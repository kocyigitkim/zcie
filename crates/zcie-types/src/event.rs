//! Event envelope `zcie.event.v1` (`docs/05-data-model.md` section 3).

use serde::{Deserialize, Serialize};

use crate::ids::EventId;
use crate::scope::Scope;

/// The schema name of an event envelope.
pub const EVENT_SCHEMA: &str = "zcie.event.v1";

/// The metadata of one immutable event. The payload is in the object store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Value schema version.
    pub v: u32,
    /// Always `zcie.event.v1`.
    pub schema: String,
    /// Set by the daemon.
    pub event_id: EventId,
    /// Set by the daemon. Monotonic per tenant.
    pub seq: u64,
    /// Set by the client. Two events with the same key are one event.
    pub idempotency_key: String,
    /// RFC 3339 time.
    pub timestamp_utc: String,
    /// The scope of the event.
    pub scope: Scope,
    /// The client source of the event.
    pub source: EventSource,
    /// The event type.
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// Content hash `b3:...` of the payload, or `None`.
    pub payload_ref: Option<String>,
    /// Payload size in bytes.
    pub payload_size: u64,
    /// Git state of the client, if known.
    #[serde(default)]
    pub git: Option<GitContext>,
    /// Privacy class of the event.
    pub classification: Classification,
    /// Number of secrets that redaction replaced.
    #[serde(default)]
    pub redactions: u32,
    /// True after `forget`. The payload reference is removed.
    #[serde(default)]
    pub forgotten: bool,
}

/// The client source of an event.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EventSource {
    /// Client application name, for example `claude-code`.
    pub app: String,
    /// Client session ID.
    pub session_id: String,
    /// Client turn ID, if the client has turns.
    #[serde(default)]
    pub turn_id: Option<String>,
}

/// Git state of a client at event time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitContext {
    /// Worktree ID.
    pub worktree_id: String,
    /// HEAD commit SHA.
    pub head: String,
}

/// Privacy class of a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Public content.
    Public,
    /// Content private to the project. The default.
    #[default]
    ProjectPrivate,
    /// Personal content.
    Personal,
    /// Secret content. Only admin principals can read it.
    Secret,
}

/// The type of an event (`docs/05-data-model.md` section 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// A user message.
    #[serde(rename = "user.message")]
    UserMessage,
    /// An assistant message.
    #[serde(rename = "assistant.message")]
    AssistantMessage,
    /// A tool call.
    #[serde(rename = "tool.call")]
    ToolCall,
    /// A tool result.
    #[serde(rename = "tool.result")]
    ToolResult,
    /// A command run with its output.
    #[serde(rename = "command.run")]
    CommandRun,
    /// A test result.
    #[serde(rename = "test.result")]
    TestResult,
    /// A changed file.
    #[serde(rename = "file.changed")]
    FileChanged,
    /// A deleted file.
    #[serde(rename = "file.deleted")]
    FileDeleted,
    /// A Git commit.
    #[serde(rename = "git.commit")]
    GitCommit,
    /// Session start.
    #[serde(rename = "session.start")]
    SessionStart,
    /// Session end.
    #[serde(rename = "session.end")]
    SessionEnd,
    /// A user confirmation of a link or a record.
    #[serde(rename = "user.confirmation")]
    UserConfirmation,
}

impl EventType {
    /// Returns the serialized name, for example `user.message`.
    pub fn name(&self) -> &'static str {
        match self {
            EventType::UserMessage => "user.message",
            EventType::AssistantMessage => "assistant.message",
            EventType::ToolCall => "tool.call",
            EventType::ToolResult => "tool.result",
            EventType::CommandRun => "command.run",
            EventType::TestResult => "test.result",
            EventType::FileChanged => "file.changed",
            EventType::FileDeleted => "file.deleted",
            EventType::GitCommit => "git.commit",
            EventType::SessionStart => "session.start",
            EventType::SessionEnd => "session.end",
            EventType::UserConfirmation => "user.confirmation",
        }
    }

    /// Returns true if the event type needs an `ExtractMemory` job.
    pub fn needs_job(&self) -> bool {
        !matches!(self, EventType::SessionStart | EventType::SessionEnd | EventType::ToolCall)
    }

    /// Returns true if the payload is message text for the conversations index.
    pub fn is_conversation(&self) -> bool {
        matches!(
            self,
            EventType::UserMessage
                | EventType::AssistantMessage
                | EventType::TestResult
                | EventType::CommandRun
                | EventType::ToolResult
        )
    }
}

/// Payload of a `test.result` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResultPayload {
    /// Test name.
    pub name: String,
    /// `passed` or `failed`.
    pub status: String,
    /// Test output.
    #[serde(default)]
    pub output: Option<String>,
    /// Refs that the test covers.
    #[serde(default)]
    pub refs: Vec<String>,
}

/// Payload of a `file.changed` or `file.deleted` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangePayload {
    /// Path relative to the worktree root, or an absolute path inside it.
    pub path: String,
    /// Old content hash, if known.
    #[serde(default)]
    pub old_hash: Option<String>,
    /// New content hash, if known.
    #[serde(default)]
    pub new_hash: Option<String>,
}

/// Payload of a `git.commit` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitPayload {
    /// Commit SHA.
    pub sha: String,
}

/// Payload of a `user.confirmation` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationPayload {
    /// Source ref.
    pub from: String,
    /// Relation name, for example `IMPLEMENTS`.
    pub relation: String,
    /// Target ref.
    pub to: String,
}

/// Payload of a `command.run` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRunPayload {
    /// The command line.
    pub command: String,
    /// Exit code of the command.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Output of the command.
    #[serde(default)]
    pub output: Option<String>,
}
