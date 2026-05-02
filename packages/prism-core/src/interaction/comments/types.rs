//! Shared comment data shapes.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A single comment, either a root or a reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub object_id: String,
    /// `None` for root comments, `Some(root_comment_id)` for replies.
    pub parent_id: Option<String>,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
    /// ISO 8601 timestamp.
    pub created_at: String,
    pub updated_at: Option<String>,
    /// Soft-delete marker.
    pub deleted_at: Option<String>,
    /// Emoji → list of user IDs that reacted.
    pub reactions: IndexMap<String, Vec<String>>,
    pub resolved: bool,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<String>,
}

/// A root comment together with its direct replies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentThread {
    pub root_comment: Comment,
    pub replies: Vec<Comment>,
    pub total_replies: usize,
    pub is_resolved: bool,
    pub last_activity_at: String,
}

/// Input for creating a new comment. The store assigns `id` and
/// `created_at` automatically.
#[derive(Debug, Clone)]
pub struct NewComment {
    pub object_id: String,
    pub parent_id: Option<String>,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
}
