//! `automation::types` — trigger / condition / action data model.
//!
//! Port of `kernel/automation/automation-types.ts` at 8426588. Shape
//! mirrors the legacy TS discriminated unions via `#[serde(tag = "type")]`
//! enums so JSON produced by either tree round-trips.

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

// ── Triggers ────────────────────────────────────────────────────────────────

/// Variants of an object lifecycle event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ObjectEventType {
    #[default]
    #[serde(rename = "object:created")]
    Created,
    #[serde(rename = "object:updated")]
    Updated,
    #[serde(rename = "object:deleted")]
    Deleted,
}

impl ObjectEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "object:created",
            Self::Updated => "object:updated",
            Self::Deleted => "object:deleted",
        }
    }
}

/// Filter shared by the three object-lifecycle triggers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectTriggerFilter {
    #[serde(
        default,
        rename = "objectTypes",
        skip_serializing_if = "Option::is_none"
    )]
    pub object_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        rename = "fieldMatch",
        skip_serializing_if = "Option::is_none"
    )]
    pub field_match: Option<JsonMap<String, JsonValue>>,
}

/// Trigger enum tagged on `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AutomationTrigger {
    #[serde(rename = "object:created")]
    ObjectCreated {
        #[serde(
            default,
            rename = "objectTypes",
            skip_serializing_if = "Option::is_none"
        )]
        object_types: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(
            default,
            rename = "fieldMatch",
            skip_serializing_if = "Option::is_none"
        )]
        field_match: Option<JsonMap<String, JsonValue>>,
    },
    #[serde(rename = "object:updated")]
    ObjectUpdated {
        #[serde(
            default,
            rename = "objectTypes",
            skip_serializing_if = "Option::is_none"
        )]
        object_types: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(
            default,
            rename = "fieldMatch",
            skip_serializing_if = "Option::is_none"
        )]
        field_match: Option<JsonMap<String, JsonValue>>,
    },
    #[serde(rename = "object:deleted")]
    ObjectDeleted {
        #[serde(
            default,
            rename = "objectTypes",
            skip_serializing_if = "Option::is_none"
        )]
        object_types: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(
            default,
            rename = "fieldMatch",
            skip_serializing_if = "Option::is_none"
        )]
        field_match: Option<JsonMap<String, JsonValue>>,
    },
    #[serde(rename = "cron")]
    Cron {
        cron: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
    },
    #[serde(rename = "manual")]
    Manual,
}

impl AutomationTrigger {
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::ObjectCreated { .. } => "object:created",
            Self::ObjectUpdated { .. } => "object:updated",
            Self::ObjectDeleted { .. } => "object:deleted",
            Self::Cron { .. } => "cron",
            Self::Manual => "manual",
        }
    }

    pub fn object_event(&self) -> Option<ObjectEventType> {
        match self {
            Self::ObjectCreated { .. } => Some(ObjectEventType::Created),
            Self::ObjectUpdated { .. } => Some(ObjectEventType::Updated),
            Self::ObjectDeleted { .. } => Some(ObjectEventType::Deleted),
            _ => None,
        }
    }

    pub fn as_object_filter(&self) -> Option<ObjectTriggerFilter> {
        match self {
            Self::ObjectCreated {
                object_types,
                tags,
                field_match,
            }
            | Self::ObjectUpdated {
                object_types,
                tags,
                field_match,
            }
            | Self::ObjectDeleted {
                object_types,
                tags,
                field_match,
            } => Some(ObjectTriggerFilter {
                object_types: object_types.clone(),
                tags: tags.clone(),
                field_match: field_match.clone(),
            }),
            _ => None,
        }
    }
}

// ── Conditions ──────────────────────────────────────────────────────────────

/// Comparison operators supported by [`FieldCondition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "neq")]
    Neq,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "lte")]
    Lte,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "startsWith")]
    StartsWith,
    #[serde(rename = "endsWith")]
    EndsWith,
    #[serde(rename = "matches")]
    Matches,
}

/// Tag-match mode for [`TagCondition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagMode {
    All,
    Any,
}

/// Condition tree — tagged on `type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AutomationCondition {
    #[serde(rename = "field")]
    Field {
        path: String,
        operator: FieldOperator,
        value: JsonValue,
    },
    #[serde(rename = "type")]
    Type {
        #[serde(rename = "objectType")]
        object_type: String,
    },
    #[serde(rename = "tags")]
    Tags { tags: Vec<String>, mode: TagMode },
    #[serde(rename = "and")]
    And {
        conditions: Vec<AutomationCondition>,
    },
    #[serde(rename = "or")]
    Or {
        conditions: Vec<AutomationCondition>,
    },
    #[serde(rename = "not")]
    Not { condition: Box<AutomationCondition> },
}

// ── Actions ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AutomationAction {
    #[serde(rename = "object:create")]
    CreateObject {
        #[serde(rename = "objectType")]
        object_type: String,
        template: JsonMap<String, JsonValue>,
        #[serde(
            default,
            rename = "parentFromTrigger",
            skip_serializing_if = "Option::is_none"
        )]
        parent_from_trigger: Option<bool>,
    },
    #[serde(rename = "object:update")]
    UpdateObject {
        target: String,
        patch: JsonMap<String, JsonValue>,
    },
    #[serde(rename = "object:delete")]
    DeleteObject { target: String },
    #[serde(rename = "notification:send")]
    Notification {
        target: String,
        title: String,
        body: String,
    },
    #[serde(rename = "delay")]
    Delay { seconds: f64 },
    #[serde(rename = "automation:run")]
    RunAutomation {
        #[serde(rename = "automationId")]
        automation_id: String,
    },
    #[serde(rename = "email:send")]
    Email {
        to: String,
        subject: String,
        body: String,
        #[serde(
            default,
            rename = "templateId",
            skip_serializing_if = "Option::is_none"
        )]
        template_id: Option<String>,
    },
}

impl AutomationAction {
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::CreateObject { .. } => "object:create",
            Self::UpdateObject { .. } => "object:update",
            Self::DeleteObject { .. } => "object:delete",
            Self::Notification { .. } => "notification:send",
            Self::Delay { .. } => "delay",
            Self::RunAutomation { .. } => "automation:run",
            Self::Email { .. } => "email:send",
        }
    }
}

// ── Automation record ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger: AutomationTrigger,
    #[serde(default)]
    pub conditions: Vec<AutomationCondition>,
    #[serde(default)]
    pub actions: Vec<AutomationAction>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default, rename = "lastRunAt", skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    #[serde(default, rename = "runCount")]
    pub run_count: u32,
}

// ── Execution context ───────────────────────────────────────────────────────

/// Runtime context available during condition evaluation and action
/// template interpolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationContext {
    #[serde(rename = "automationId")]
    pub automation_id: String,
    #[serde(rename = "triggeredAt")]
    pub triggered_at: String,
    #[serde(rename = "triggerType")]
    pub trigger_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<JsonMap<String, JsonValue>>,
    #[serde(
        default,
        rename = "previousObject",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_object: Option<JsonMap<String, JsonValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<JsonMap<String, JsonValue>>,
}

// ── Run records ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Success,
    Failed,
    Skipped,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    #[serde(rename = "actionIndex")]
    pub action_index: usize,
    #[serde(rename = "actionType")]
    pub action_type: String,
    pub status: ActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, rename = "elapsedMs", skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: String,
    #[serde(rename = "automationId")]
    pub automation_id: String,
    pub status: RunStatus,
    #[serde(rename = "triggeredAt")]
    pub triggered_at: String,
    #[serde(
        default,
        rename = "completedAt",
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<String>,
    #[serde(rename = "conditionPassed")]
    pub condition_passed: bool,
    #[serde(default, rename = "actionResults")]
    pub action_results: Vec<ActionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Object event ────────────────────────────────────────────────────────────

/// Event dispatched to [`AutomationEngine::handle_object_event`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEvent {
    #[serde(rename = "type")]
    pub event: ObjectEventType,
    pub object: JsonMap<String, JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<JsonMap<String, JsonValue>>,
}

// ── Action handler ──────────────────────────────────────────────────────────

/// Synchronous action handler. Hosts that need async execution wrap the
/// engine in their own runtime. `delay` is handled by the engine itself
/// via a pluggable [`DelaySleeper`] (see `engine.rs`).
pub trait ActionHandler: Send + Sync {
    fn handle(&self, action: &AutomationAction, context: &AutomationContext) -> Result<(), String>;
}

/// Blanket impl for plain closures.
impl<F> ActionHandler for F
where
    F: Fn(&AutomationAction, &AutomationContext) -> Result<(), String> + Send + Sync,
{
    fn handle(&self, action: &AutomationAction, context: &AutomationContext) -> Result<(), String> {
        self(action, context)
    }
}
