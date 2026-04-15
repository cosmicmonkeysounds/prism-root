//! Shared notification data shapes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    System,
    Mention,
    Activity,
    Reminder,
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub kind: NotificationKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    pub read: bool,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, JsonValue>,
}

/// Keyword-style arguments for `NotificationStore::add`. Missing
/// fields are filled by the store (`id`, `read`, `pinned`,
/// `created_at`).
#[derive(Debug, Clone, Default)]
pub struct NotificationInput {
    pub id: Option<String>,
    pub kind: NotificationKind,
    pub title: String,
    pub body: Option<String>,
    pub object_id: Option<String>,
    pub object_type: Option<String>,
    pub actor_id: Option<String>,
    pub read: Option<bool>,
    pub pinned: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub data: BTreeMap<String, JsonValue>,
}

impl Default for NotificationKind {
    fn default() -> Self {
        NotificationKind::Info
    }
}

#[derive(Debug, Clone, Default)]
pub struct NotificationFilter {
    pub kind: Option<Vec<NotificationKind>>,
    pub read: Option<bool>,
    pub pinned: Option<bool>,
    pub object_id: Option<String>,
    /// Only notifications created strictly after this timestamp.
    pub since: Option<DateTime<Utc>>,
}

impl NotificationFilter {
    pub fn matches(&self, n: &Notification) -> bool {
        if let Some(kinds) = &self.kind {
            if !kinds.is_empty() && !kinds.contains(&n.kind) {
                return false;
            }
        }
        if let Some(read) = self.read {
            if n.read != read {
                return false;
            }
        }
        if let Some(pinned) = self.pinned {
            if n.pinned != pinned {
                return false;
            }
        }
        if let Some(object_id) = &self.object_id {
            if n.object_id.as_deref() != Some(object_id.as_str()) {
                return false;
            }
        }
        if let Some(since) = self.since {
            if n.created_at <= since {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationChangeType {
    Add,
    Update,
    Dismiss,
    Clear,
}

#[derive(Debug, Clone)]
pub struct NotificationChange {
    pub kind: NotificationChangeType,
    pub notification: Option<Notification>,
}
