//! `actor::types` — shared enums, capability scope, task record, and
//! the `ActorRuntime` trait.
//!
//! Port of `kernel/actor/actor-types.ts` at 8426588. Async runtime
//! execution in the legacy TS version used `Promise`s; we keep the port
//! synchronous because `prism-core` is a pure-logic leaf and the host
//! that actually drives WASM / sidecar execution (prism-daemon) owns
//! its own async runtime. Runtimes that need to bridge to async can
//! block on their host executor or schedule the real work outside the
//! `execute` call.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Where an actor task runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionTarget {
    /// Zero-latency, offline, private — the host's own runtime.
    #[default]
    Local,
    /// Trusted federated delegate (e.g. home-server GPU) over E2EE.
    Federated,
    /// Third-party provider gated by a capability token.
    External,
}

/// Fine-grained permission scope for actor sandboxing. Actors get zero
/// capabilities by default; each must be explicitly granted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityScope {
    pub network: bool,
    pub fs_read: bool,
    pub fs_write: bool,
    pub crdt_read: bool,
    pub crdt_write: bool,
    pub spawn: bool,
    /// Allowed API endpoints (for external providers). Empty = none.
    pub allowed_endpoints: Vec<String>,
    /// Maximum execution time in milliseconds. 0 = no limit.
    pub max_duration_ms: u64,
    /// Maximum memory in bytes. 0 = no limit.
    pub max_memory_bytes: u64,
}

/// Default zero-trust sandbox: CRDT read only, no fs / network / spawn,
/// 30 s duration cap, no memory cap.
pub fn default_capability_scope() -> CapabilityScope {
    CapabilityScope {
        network: false,
        fs_read: false,
        fs_write: false,
        crdt_read: true,
        crdt_write: false,
        spawn: false,
        allowed_endpoints: Vec::new(),
        max_duration_ms: 30_000,
        max_memory_bytes: 0,
    }
}

impl Default for CapabilityScope {
    fn default() -> Self {
        default_capability_scope()
    }
}

/// Partial scope — `Some` fields override the default; `None` leaves
/// the default. Mirrors `Partial<CapabilityScope>` from TS.
#[derive(Debug, Clone, Default)]
pub struct CapabilityScopeOverride {
    pub network: Option<bool>,
    pub fs_read: Option<bool>,
    pub fs_write: Option<bool>,
    pub crdt_read: Option<bool>,
    pub crdt_write: Option<bool>,
    pub spawn: Option<bool>,
    pub allowed_endpoints: Option<Vec<String>>,
    pub max_duration_ms: Option<u64>,
    pub max_memory_bytes: Option<u64>,
}

impl CapabilityScopeOverride {
    pub fn apply(self, base: CapabilityScope) -> CapabilityScope {
        CapabilityScope {
            network: self.network.unwrap_or(base.network),
            fs_read: self.fs_read.unwrap_or(base.fs_read),
            fs_write: self.fs_write.unwrap_or(base.fs_write),
            crdt_read: self.crdt_read.unwrap_or(base.crdt_read),
            crdt_write: self.crdt_write.unwrap_or(base.crdt_write),
            spawn: self.spawn.unwrap_or(base.spawn),
            allowed_endpoints: self.allowed_endpoints.unwrap_or(base.allowed_endpoints),
            max_duration_ms: self.max_duration_ms.unwrap_or(base.max_duration_ms),
            max_memory_bytes: self.max_memory_bytes.unwrap_or(base.max_memory_bytes),
        }
    }
}

/// Lifecycle status of a [`ProcessTask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// An enqueued unit of work. Payload and result are `serde_json::Value`
/// to match the `unknown` TS type — callers serialize their own shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessTask {
    pub id: String,
    pub name: String,
    pub runtime: String,
    pub target: ExecutionTarget,
    pub payload: JsonValue,
    pub scope: CapabilityScope,
    /// Lower = higher priority. Default 10.
    pub priority: i32,
    pub status: TaskStatus,
    #[serde(default)]
    pub result: Option<JsonValue>,
    #[serde(default)]
    pub error: Option<String>,
    pub enqueued_at: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

/// Parameters for [`ProcessQueue::enqueue`](super::ProcessQueue::enqueue).
#[derive(Debug, Clone)]
pub struct EnqueueParams {
    pub name: String,
    pub runtime: String,
    pub payload: JsonValue,
    pub target: Option<ExecutionTarget>,
    pub scope: Option<CapabilityScopeOverride>,
    pub priority: Option<i32>,
}

impl EnqueueParams {
    pub fn new(name: impl Into<String>, runtime: impl Into<String>, payload: JsonValue) -> Self {
        Self {
            name: name.into(),
            runtime: runtime.into(),
            payload,
            target: None,
            scope: None,
            priority: None,
        }
    }
}

/// Result returned from a runtime's `execute` call.
#[derive(Debug, Clone)]
pub struct RuntimeResult {
    pub success: bool,
    pub value: Option<JsonValue>,
    pub error: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

impl RuntimeResult {
    pub fn ok(value: JsonValue, duration_ms: u64) -> Self {
        Self {
            success: true,
            value: Some(value),
            error: None,
            duration_ms,
        }
    }

    pub fn err(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: false,
            value: None,
            error: Some(error.into()),
            duration_ms,
        }
    }
}

/// Pluggable language runtime.
///
/// Implementations live downstream: `luau` (mlua-backed, in
/// `prism-daemon`), `typescript` (Deno sidecar), `python` (Python
/// sidecar). A simple in-memory test runtime ships here behind
/// [`TestRuntime`].
pub trait ActorRuntime: Send + Sync {
    /// Runtime identifier (e.g. `"luau"`).
    fn name(&self) -> &str;
    /// Whether this runtime is usable in the current environment.
    fn is_available(&self) -> bool;
    /// Execute `payload` under `scope` and return a result. Implementations
    /// must return synchronously — async runtimes block on their host
    /// executor. The legacy TS version was async; see module-level docs
    /// for why we keep this sync.
    fn execute(&self, payload: &JsonValue, scope: &CapabilityScope) -> RuntimeResult;
    /// Dispose of runtime resources. Default no-op.
    fn dispose(&self) {}
}

/// Types of events broadcast on the [`ProcessQueue`](super::ProcessQueue)
/// event bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueEventType {
    Enqueued,
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// A single event on the queue bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEvent {
    #[serde(rename = "type")]
    pub event_type: QueueEventType,
    pub task_id: String,
    pub task: ProcessTask,
}

/// Options handed to [`super::ProcessQueue::new`].
#[derive(Debug, Clone, Copy)]
pub struct ProcessQueueOptions {
    /// Maximum number of tasks treated as "running" at once. Default 1.
    pub concurrency: usize,
    /// Auto-start processing on construction. Default false.
    pub auto_start: bool,
}

impl Default for ProcessQueueOptions {
    fn default() -> Self {
        Self {
            concurrency: 1,
            auto_start: false,
        }
    }
}
