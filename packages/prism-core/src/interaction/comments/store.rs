//! Synchronous in-memory comment store with per-object subscription.
//!
//! Port of `@core/comments`. One-level threading (root + replies),
//! soft-delete, emoji reactions, resolve/unresolve, and a per-object
//! listener bus.

use std::collections::HashMap;

use chrono::Utc;
use indexmap::IndexMap;
use uuid::Uuid;

use super::types::{Comment, CommentThread, NewComment};

pub type CommentListener = Box<dyn Fn(&[Comment])>;

pub struct CommentStore {
    /// `object_id` → flat list of comments (including soft-deleted).
    comments: IndexMap<String, Vec<Comment>>,
    /// `object_id` → list of (subscriber_id, callback).
    subscribers: IndexMap<String, Vec<(usize, CommentListener)>>,
    next_sub_id: usize,
}

impl CommentStore {
    pub fn new() -> Self {
        Self {
            comments: IndexMap::new(),
            subscribers: IndexMap::new(),
            next_sub_id: 0,
        }
    }

    // ── Read ────────────────────────────────────────────────────

    /// Non-deleted comments for `object_id`, flat.
    pub fn get_comments(&self, object_id: &str) -> Vec<&Comment> {
        self.comments
            .get(object_id)
            .map(|cs| cs.iter().filter(|c| c.deleted_at.is_none()).collect())
            .unwrap_or_default()
    }

    /// Build a single thread for `root_comment_id` under `object_id`.
    pub fn get_thread(&self, object_id: &str, root_comment_id: &str) -> Option<CommentThread> {
        let all = self.comments.get(object_id)?;
        let root = all
            .iter()
            .find(|c| c.id == root_comment_id && c.parent_id.is_none())?;
        let replies: Vec<Comment> = all
            .iter()
            .filter(|c| c.parent_id.as_deref() == Some(root_comment_id) && c.deleted_at.is_none())
            .cloned()
            .collect();
        let total_replies = replies.len();
        let last_activity_at = thread_last_activity(root, &replies);
        Some(CommentThread {
            root_comment: root.clone(),
            replies,
            total_replies,
            is_resolved: root.resolved,
            last_activity_at,
        })
    }

    /// All threads for `object_id`, ordered by root `created_at` ascending.
    pub fn get_threads(&self, object_id: &str) -> Vec<CommentThread> {
        let all = match self.comments.get(object_id) {
            Some(cs) => cs,
            None => return Vec::new(),
        };
        let non_deleted: Vec<&Comment> = all.iter().filter(|c| c.deleted_at.is_none()).collect();
        build_threads_from_refs(&non_deleted)
    }

    /// Non-deleted direct replies to `comment_id` (searches all objects).
    pub fn get_replies(&self, comment_id: &str) -> Vec<&Comment> {
        for cs in self.comments.values() {
            // Check if this object has the parent comment.
            if cs.iter().any(|c| c.id == comment_id) {
                return cs
                    .iter()
                    .filter(|c| {
                        c.parent_id.as_deref() == Some(comment_id) && c.deleted_at.is_none()
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    /// Count of unresolved root comments for `object_id`.
    pub fn get_unresolved_count(&self, object_id: &str) -> usize {
        self.comments
            .get(object_id)
            .map(|cs| {
                cs.iter()
                    .filter(|c| c.parent_id.is_none() && !c.resolved && c.deleted_at.is_none())
                    .count()
            })
            .unwrap_or(0)
    }

    // ── Write ───────────────────────────────────────────────────

    /// Add a new comment, assigning `id` and `created_at`.
    pub fn add(&mut self, input: NewComment) -> Comment {
        let now = Utc::now().to_rfc3339();
        let comment = Comment {
            id: Uuid::new_v4().to_string(),
            object_id: input.object_id.clone(),
            parent_id: input.parent_id,
            author_id: input.author_id,
            author_name: input.author_name,
            body: input.body,
            created_at: now,
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        self.comments
            .entry(input.object_id.clone())
            .or_default()
            .push(comment.clone());
        self.notify(&input.object_id);
        comment
    }

    /// Edit the body of a comment. Returns `None` if unknown or deleted.
    pub fn edit(&mut self, id: &str, body: &str) -> Option<&Comment> {
        let (object_id, idx) = self.find_comment_mut(id)?;
        let object_id = object_id.clone();
        let cs = self.comments.get_mut(&object_id)?;
        let c = &mut cs[idx];
        if c.deleted_at.is_some() {
            return None;
        }
        c.body = body.to_string();
        c.updated_at = Some(Utc::now().to_rfc3339());
        self.notify(&object_id);
        let cs = self.comments.get(&object_id)?;
        Some(&cs[idx])
    }

    /// Soft-delete a comment. Idempotent.
    pub fn delete(&mut self, id: &str) {
        if let Some((object_id, idx)) = self.find_comment_mut(id) {
            let object_id = object_id.clone();
            if let Some(cs) = self.comments.get_mut(&object_id) {
                if cs[idx].deleted_at.is_none() {
                    cs[idx].deleted_at = Some(Utc::now().to_rfc3339());
                    self.notify(&object_id);
                }
            }
        }
    }

    /// Resolve a root comment.
    pub fn resolve(&mut self, id: &str, by_user_id: &str) {
        if let Some((object_id, idx)) = self.find_comment_mut(id) {
            let object_id = object_id.clone();
            if let Some(cs) = self.comments.get_mut(&object_id) {
                let c = &mut cs[idx];
                c.resolved = true;
                c.resolved_by = Some(by_user_id.to_string());
                c.resolved_at = Some(Utc::now().to_rfc3339());
                self.notify(&object_id);
            }
        }
    }

    /// Unresolve a root comment.
    pub fn unresolve(&mut self, id: &str) {
        if let Some((object_id, idx)) = self.find_comment_mut(id) {
            let object_id = object_id.clone();
            if let Some(cs) = self.comments.get_mut(&object_id) {
                let c = &mut cs[idx];
                c.resolved = false;
                c.resolved_by = None;
                c.resolved_at = None;
                self.notify(&object_id);
            }
        }
    }

    /// Add a reaction. Idempotent per (comment, emoji, user).
    pub fn add_reaction(&mut self, comment_id: &str, emoji: &str, user_id: &str) {
        if let Some((object_id, idx)) = self.find_comment_mut(comment_id) {
            let object_id = object_id.clone();
            if let Some(cs) = self.comments.get_mut(&object_id) {
                let c = &mut cs[idx];
                if c.deleted_at.is_some() {
                    return;
                }
                let users = c.reactions.entry(emoji.to_string()).or_default();
                if !users.contains(&user_id.to_string()) {
                    users.push(user_id.to_string());
                    self.notify(&object_id);
                }
            }
        }
    }

    /// Remove a reaction.
    pub fn remove_reaction(&mut self, comment_id: &str, emoji: &str, user_id: &str) {
        if let Some((object_id, idx)) = self.find_comment_mut(comment_id) {
            let object_id = object_id.clone();
            if let Some(cs) = self.comments.get_mut(&object_id) {
                let c = &mut cs[idx];
                if c.deleted_at.is_some() {
                    return;
                }
                let mut changed = false;
                let mut remove_key = false;
                if let Some(users) = c.reactions.get_mut(emoji) {
                    let before = users.len();
                    users.retain(|u| u != user_id);
                    changed = users.len() != before;
                    remove_key = users.is_empty();
                }
                if remove_key {
                    c.reactions.shift_remove(emoji);
                }
                if changed {
                    self.notify(&object_id);
                }
            }
        }
    }

    /// Bulk-load comments for an object. Replaces any existing
    /// comments for that `object_id`. Does **not** fire subscribers.
    pub fn hydrate(&mut self, object_id: &str, comments: Vec<Comment>) {
        self.comments.insert(object_id.to_string(), comments);
    }

    // ── Observer ────────────────────────────────────────────────

    /// Subscribe to changes on a specific `object_id`. The callback
    /// receives all non-deleted comments for that object on every
    /// mutation.
    pub fn subscribe(&mut self, object_id: &str, listener: CommentListener) -> usize {
        let id = self.next_sub_id;
        self.next_sub_id += 1;
        self.subscribers
            .entry(object_id.to_string())
            .or_default()
            .push((id, listener));
        id
    }

    /// Remove a subscriber by ID.
    pub fn unsubscribe(&mut self, id: usize) {
        for subs in self.subscribers.values_mut() {
            subs.retain(|(sid, _)| *sid != id);
        }
    }

    // ── Serialization ───────────────────────────────────────────

    /// Snapshot the full store (including soft-deleted comments).
    pub fn to_json(&self) -> IndexMap<String, Vec<Comment>> {
        self.comments.clone()
    }

    // ── Internal ────────────────────────────────────────────────

    /// Find the (object_id, index) of a comment by its `id`.
    fn find_comment_mut(&self, id: &str) -> Option<(String, usize)> {
        for (object_id, cs) in &self.comments {
            if let Some(idx) = cs.iter().position(|c| c.id == id) {
                return Some((object_id.clone(), idx));
            }
        }
        None
    }

    fn notify(&self, object_id: &str) {
        if let Some(subs) = self.subscribers.get(object_id) {
            let visible: Vec<Comment> = self
                .comments
                .get(object_id)
                .map(|cs| {
                    cs.iter()
                        .filter(|c| c.deleted_at.is_none())
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            for (_, listener) in subs {
                listener(&visible);
            }
        }
    }
}

impl Default for CommentStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Pure utility functions ──────────────────────────────────────

/// Build one-level threads from a flat comment list.
///
/// - Roots: comments with `parent_id = None` and `deleted_at = None`.
/// - Replies: comments whose `parent_id` matches a root's `id`.
/// - Orphans (replies whose parent is missing) are promoted to roots.
/// - Roots sorted by `created_at` ascending.
/// - Replies sorted by `created_at` ascending.
pub fn build_threads(comments: &[Comment]) -> Vec<CommentThread> {
    let non_deleted: Vec<&Comment> = comments.iter().filter(|c| c.deleted_at.is_none()).collect();
    build_threads_from_refs(&non_deleted)
}

fn build_threads_from_refs(comments: &[&Comment]) -> Vec<CommentThread> {
    let root_ids: HashMap<&str, &Comment> = comments
        .iter()
        .filter(|c| c.parent_id.is_none())
        .map(|c| (c.id.as_str(), *c))
        .collect();

    let mut reply_map: IndexMap<String, Vec<Comment>> = IndexMap::new();
    let mut orphans: Vec<&Comment> = Vec::new();

    for c in comments.iter().filter(|c| c.parent_id.is_some()) {
        let pid = c.parent_id.as_deref().unwrap();
        if root_ids.contains_key(pid) {
            reply_map
                .entry(pid.to_string())
                .or_default()
                .push((*c).clone());
        } else {
            orphans.push(c);
        }
    }

    // Sort replies by created_at ascending.
    for replies in reply_map.values_mut() {
        replies.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    }

    let mut threads: Vec<CommentThread> = Vec::new();

    // Real roots.
    let mut roots: Vec<&Comment> = root_ids.values().copied().collect();
    roots.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    for root in roots {
        let replies = reply_map.shift_remove(root.id.as_str()).unwrap_or_default();
        let total_replies = replies.len();
        let last_activity_at = thread_last_activity(root, &replies);
        threads.push(CommentThread {
            root_comment: root.clone(),
            replies,
            total_replies,
            is_resolved: root.resolved,
            last_activity_at,
        });
    }

    // Orphan promotion: treat each orphan as a root with no replies.
    let mut orphans_sorted = orphans;
    orphans_sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    for orphan in orphans_sorted {
        let last_activity_at = effective_timestamp(orphan);
        threads.push(CommentThread {
            root_comment: orphan.clone(),
            replies: Vec::new(),
            total_replies: 0,
            is_resolved: orphan.resolved,
            last_activity_at,
        });
    }

    threads
}

/// Count non-deleted comments.
pub fn count_comments(comments: &[Comment]) -> usize {
    comments.iter().filter(|c| c.deleted_at.is_none()).count()
}

/// Latest timestamp across all comments (including deleted).
/// Prefers `updated_at` over `created_at`.
pub fn last_activity(comments: &[Comment]) -> Option<String> {
    comments.iter().map(effective_timestamp).max()
}

/// Truncate `body` to `max_length` characters. Normalises internal
/// whitespace to single spaces. Appends `'…'` when truncated.
pub fn truncate_body(body: &str, max_length: usize) -> String {
    let normalised: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalised.len() <= max_length {
        normalised
    } else {
        let mut truncated: String = normalised.chars().take(max_length).collect();
        truncated.push('\u{2026}');
        truncated
    }
}

/// Returns `true` if `user_id` is allowed to edit the comment.
///
/// - Authors can always edit their own comments.
/// - Users with `admin`, `super-admin`, or `moderator` roles can edit.
/// - Deleted comments cannot be edited by anyone.
pub fn can_edit(comment: &Comment, user_id: &str, roles: &[&str]) -> bool {
    if comment.deleted_at.is_some() {
        return false;
    }
    if comment.author_id == user_id {
        return true;
    }
    const PRIVILEGED: &[&str] = &["admin", "super-admin", "moderator"];
    roles.iter().any(|r| PRIVILEGED.contains(r))
}

// ── Helpers ─────────────────────────────────────────────────────

fn effective_timestamp(c: &Comment) -> String {
    c.updated_at.as_deref().unwrap_or(&c.created_at).to_string()
}

fn thread_last_activity(root: &Comment, replies: &[Comment]) -> String {
    let root_ts = effective_timestamp(root);
    let reply_max = replies.iter().map(effective_timestamp).max();
    match reply_max {
        Some(ts) if ts > root_ts => ts,
        _ => root_ts,
    }
}

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "comment-thread".into(),
            label: "Comment Thread".into(),
            description: "Displays a comment thread for an object".into(),
            category: WidgetCategory::Communication,
            default_size: WidgetSize::new(2, 2),
            data_query: Some(DataQuery {
                object_type: Some("comment".into()),
                ..Default::default()
            }),
            data_key: Some("threads".into()),
            config_fields: vec![
                FieldSpec::text("object_id", "Object ID"),
                FieldSpec::boolean("show_resolved", "Show Resolved"),
                FieldSpec::select(
                    "sort_order",
                    "Sort Order",
                    vec![
                        SelectOption::new("newest", "Newest First"),
                        SelectOption::new("oldest", "Oldest First"),
                    ],
                ),
            ],
            signals: vec![
                SignalSpec::new("comment-added", "A new comment was added")
                    .with_payload(vec![FieldSpec::text("body", "Body")]),
                SignalSpec::new("comment-resolved", "A comment was resolved")
                    .with_payload(vec![FieldSpec::text("comment_id", "Comment ID")]),
                SignalSpec::new("reply-added", "A reply was added").with_payload(vec![
                    FieldSpec::text("parent_id", "Parent ID"),
                    FieldSpec::text("body", "Body"),
                ]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("new-comment", "New Comment", "plus"),
                ToolbarAction::signal("resolve-all", "Resolve All", "check"),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Comments"}),
                        },
                        TemplateNode::Conditional {
                            field: "has_comments".into(),
                            child: Box::new(TemplateNode::Repeater {
                                source: "threads".into(),
                                item_template: Box::new(TemplateNode::DataBinding {
                                    field: "body".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                }),
                                empty_label: Some("No comments yet".into()),
                            }),
                            fallback: None,
                        },
                        TemplateNode::DataBinding {
                            field: "compose".into(),
                            component_id: "input".into(),
                            prop_key: "value".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "comment-count".into(),
            label: "Comment Count".into(),
            description: "Badge showing comment count for an object".into(),
            category: WidgetCategory::Communication,
            default_size: WidgetSize::new(1, 1),
            data_query: Some(DataQuery {
                object_type: Some("comment".into()),
                ..Default::default()
            }),
            data_key: Some("comments".into()),
            config_fields: vec![FieldSpec::text("object_id", "Object ID")],
            signals: vec![SignalSpec::new("clicked", "Badge clicked")],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "count".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "unresolved_count".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn new_comment(object_id: &str, body: &str) -> NewComment {
        NewComment {
            object_id: object_id.to_string(),
            parent_id: None,
            author_id: "user-1".to_string(),
            author_name: "Alice".to_string(),
            body: body.to_string(),
        }
    }

    fn reply_comment(object_id: &str, parent_id: &str, body: &str) -> NewComment {
        NewComment {
            object_id: object_id.to_string(),
            parent_id: Some(parent_id.to_string()),
            author_id: "user-2".to_string(),
            author_name: "Bob".to_string(),
            body: body.to_string(),
        }
    }

    // ── add ─────────────────────────────────────────────────────

    #[test]
    fn add_assigns_uuid_and_created_at() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "hello"));
        assert!(!c.id.is_empty());
        assert!(!c.created_at.is_empty());
        assert_eq!(c.body, "hello");
        assert_eq!(c.object_id, "obj-1");
        assert!(c.deleted_at.is_none());
        assert!(c.updated_at.is_none());
        assert!(!c.resolved);
    }

    #[test]
    fn add_visible_in_get_comments() {
        let mut store = CommentStore::new();
        store.add(new_comment("obj-1", "a"));
        store.add(new_comment("obj-1", "b"));
        assert_eq!(store.get_comments("obj-1").len(), 2);
    }

    #[test]
    fn get_comments_empty_for_unknown_object() {
        let store = CommentStore::new();
        assert!(store.get_comments("nope").is_empty());
    }

    // ── edit ────────────────────────────────────────────────────

    #[test]
    fn edit_updates_body_and_updated_at() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "original"));
        let edited = store.edit(&c.id, "revised").unwrap();
        assert_eq!(edited.body, "revised");
        assert!(edited.updated_at.is_some());
    }

    #[test]
    fn edit_returns_none_for_unknown() {
        let mut store = CommentStore::new();
        assert!(store.edit("nonexistent", "x").is_none());
    }

    #[test]
    fn edit_returns_none_for_deleted() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "doomed"));
        store.delete(&c.id);
        assert!(store.edit(&c.id, "x").is_none());
    }

    // ── delete ──────────────────────────────────────────────────

    #[test]
    fn delete_filters_from_get_comments() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "gone"));
        store.delete(&c.id);
        assert!(store.get_comments("obj-1").is_empty());
    }

    #[test]
    fn delete_preserved_in_to_json() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "gone"));
        store.delete(&c.id);
        let snap = store.to_json();
        let all = snap.get("obj-1").unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].deleted_at.is_some());
    }

    #[test]
    fn delete_is_idempotent() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "gone"));
        store.delete(&c.id);
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.delete(&c.id); // second delete — no notification
        assert_eq!(*calls.borrow(), 0);
    }

    // ── resolve / unresolve ─────────────────────────────────────

    #[test]
    fn resolve_sets_fields() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "todo"));
        store.resolve(&c.id, "resolver-user");
        let cs = store.get_comments("obj-1");
        assert!(cs[0].resolved);
        assert_eq!(cs[0].resolved_by.as_deref(), Some("resolver-user"));
        assert!(cs[0].resolved_at.is_some());
    }

    #[test]
    fn unresolve_clears_fields() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "todo"));
        store.resolve(&c.id, "resolver-user");
        store.unresolve(&c.id);
        let cs = store.get_comments("obj-1");
        assert!(!cs[0].resolved);
        assert!(cs[0].resolved_by.is_none());
        assert!(cs[0].resolved_at.is_none());
    }

    #[test]
    fn unresolved_count() {
        let mut store = CommentStore::new();
        let a = store.add(new_comment("obj-1", "a"));
        store.add(new_comment("obj-1", "b"));
        assert_eq!(store.get_unresolved_count("obj-1"), 2);
        store.resolve(&a.id, "u");
        assert_eq!(store.get_unresolved_count("obj-1"), 1);
    }

    // ── reactions ───────────────────────────────────────────────

    #[test]
    fn add_reaction_works() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "nice"));
        store.add_reaction(&c.id, "👍", "user-1");
        let cs = store.get_comments("obj-1");
        assert_eq!(cs[0].reactions.get("👍").unwrap(), &vec!["user-1"]);
    }

    #[test]
    fn add_reaction_is_idempotent() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "nice"));
        store.add_reaction(&c.id, "👍", "user-1");
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.add_reaction(&c.id, "👍", "user-1"); // duplicate
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn remove_reaction_works() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "nice"));
        store.add_reaction(&c.id, "👍", "user-1");
        store.remove_reaction(&c.id, "👍", "user-1");
        let cs = store.get_comments("obj-1");
        assert!(cs[0].reactions.get("👍").is_none());
    }

    #[test]
    fn reaction_noop_on_deleted() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "dead"));
        store.delete(&c.id);
        store.add_reaction(&c.id, "👍", "user-1");
        let snap = store.to_json();
        let all = snap.get("obj-1").unwrap();
        assert!(all[0].reactions.is_empty());
    }

    // ── subscribe / unsubscribe ─────────────────────────────────

    #[test]
    fn subscribe_fires_on_mutation() {
        let mut store = CommentStore::new();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.add(new_comment("obj-1", "a"));
        assert_eq!(*calls.borrow(), 1);
        store.add(new_comment("obj-1", "b"));
        assert_eq!(*calls.borrow(), 2);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let mut store = CommentStore::new();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        let sub = store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.add(new_comment("obj-1", "a"));
        assert_eq!(*calls.borrow(), 1);
        store.unsubscribe(sub);
        store.add(new_comment("obj-1", "b"));
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn subscriber_receives_non_deleted_comments() {
        let mut store = CommentStore::new();
        let received = Rc::new(RefCell::new(Vec::<usize>::new()));
        let r = received.clone();
        store.subscribe("obj-1", Box::new(move |cs| r.borrow_mut().push(cs.len())));
        store.add(new_comment("obj-1", "a"));
        let c = store.add(new_comment("obj-1", "b"));
        store.delete(&c.id);
        // After add(a) => [1], add(b) => [1,2], delete(b) => [1,2,1]
        assert_eq!(*received.borrow(), vec![1, 2, 1]);
    }

    // ── hydrate / to_json ───────────────────────────────────────

    #[test]
    fn hydrate_replaces_existing() {
        let mut store = CommentStore::new();
        store.add(new_comment("obj-1", "stale"));
        let fresh = vec![Comment {
            id: "c-1".to_string(),
            object_id: "obj-1".to_string(),
            parent_id: None,
            author_id: "u".to_string(),
            author_name: "U".to_string(),
            body: "fresh".to_string(),
            created_at: "2026-01-01T00:00:00+00:00".to_string(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        }];
        store.hydrate("obj-1", fresh);
        assert_eq!(store.get_comments("obj-1").len(), 1);
        assert_eq!(store.get_comments("obj-1")[0].body, "fresh");
    }

    #[test]
    fn hydrate_does_not_notify() {
        let mut store = CommentStore::new();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.hydrate("obj-1", Vec::new());
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn to_json_includes_soft_deleted() {
        let mut store = CommentStore::new();
        let c = store.add(new_comment("obj-1", "deleted"));
        store.add(new_comment("obj-1", "alive"));
        store.delete(&c.id);
        let snap = store.to_json();
        assert_eq!(snap.get("obj-1").unwrap().len(), 2);
    }

    // ── get_replies / get_thread ────────────────────────────────

    #[test]
    fn get_replies_returns_direct_replies() {
        let mut store = CommentStore::new();
        let root = store.add(new_comment("obj-1", "root"));
        store.add(reply_comment("obj-1", &root.id, "reply-1"));
        store.add(reply_comment("obj-1", &root.id, "reply-2"));
        let replies = store.get_replies(&root.id);
        assert_eq!(replies.len(), 2);
    }

    #[test]
    fn get_thread_returns_full_thread() {
        let mut store = CommentStore::new();
        let root = store.add(new_comment("obj-1", "root"));
        store.add(reply_comment("obj-1", &root.id, "reply"));
        let thread = store.get_thread("obj-1", &root.id).unwrap();
        assert_eq!(thread.root_comment.body, "root");
        assert_eq!(thread.replies.len(), 1);
        assert_eq!(thread.total_replies, 1);
    }

    #[test]
    fn get_thread_none_for_unknown() {
        let store = CommentStore::new();
        assert!(store.get_thread("obj-1", "nope").is_none());
    }

    #[test]
    fn get_thread_excludes_deleted_replies() {
        let mut store = CommentStore::new();
        let root = store.add(new_comment("obj-1", "root"));
        let r = store.add(reply_comment("obj-1", &root.id, "reply"));
        store.delete(&r.id);
        let thread = store.get_thread("obj-1", &root.id).unwrap();
        assert_eq!(thread.replies.len(), 0);
    }

    // ── build_threads ───────────────────────────────────────────

    #[test]
    fn build_threads_empty() {
        assert!(build_threads(&[]).is_empty());
    }

    #[test]
    fn build_threads_root_reply_separation() {
        let root = Comment {
            id: "r1".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "root".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let reply = Comment {
            parent_id: Some("r1".into()),
            id: "c1".into(),
            body: "reply".into(),
            created_at: "2026-01-01T00:01:00+00:00".into(),
            ..root.clone()
        };
        let threads = build_threads(&[root, reply]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].replies.len(), 1);
        assert_eq!(threads[0].total_replies, 1);
    }

    #[test]
    fn build_threads_root_ordering() {
        let mk = |id: &str, ts: &str| Comment {
            id: id.into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: id.into(),
            created_at: ts.into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let threads = build_threads(&[
            mk("b", "2026-01-02T00:00:00+00:00"),
            mk("a", "2026-01-01T00:00:00+00:00"),
        ]);
        assert_eq!(threads[0].root_comment.id, "a");
        assert_eq!(threads[1].root_comment.id, "b");
    }

    #[test]
    fn build_threads_reply_ordering() {
        let root = Comment {
            id: "r".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "root".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let r2 = Comment {
            id: "c2".into(),
            parent_id: Some("r".into()),
            body: "second".into(),
            created_at: "2026-01-01T00:02:00+00:00".into(),
            ..root.clone()
        };
        let r1 = Comment {
            id: "c1".into(),
            parent_id: Some("r".into()),
            body: "first".into(),
            created_at: "2026-01-01T00:01:00+00:00".into(),
            ..root.clone()
        };
        let threads = build_threads(&[root, r2, r1]);
        assert_eq!(threads[0].replies[0].body, "first");
        assert_eq!(threads[0].replies[1].body, "second");
    }

    #[test]
    fn build_threads_orphan_promotion() {
        let orphan = Comment {
            id: "orphan".into(),
            object_id: "o".into(),
            parent_id: Some("missing-parent".into()),
            author_id: "u".into(),
            author_name: "U".into(),
            body: "orphan".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let threads = build_threads(&[orphan]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].root_comment.id, "orphan");
        assert_eq!(threads[0].replies.len(), 0);
    }

    #[test]
    fn build_threads_is_resolved() {
        let root = Comment {
            id: "r".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "root".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: true,
            resolved_by: Some("u".into()),
            resolved_at: Some("2026-01-02T00:00:00+00:00".into()),
        };
        let threads = build_threads(&[root]);
        assert!(threads[0].is_resolved);
    }

    #[test]
    fn build_threads_last_activity_at() {
        let root = Comment {
            id: "r".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "root".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let reply = Comment {
            id: "c1".into(),
            parent_id: Some("r".into()),
            body: "reply".into(),
            created_at: "2026-01-01T00:05:00+00:00".into(),
            updated_at: Some("2026-01-01T00:10:00+00:00".into()),
            ..root.clone()
        };
        let threads = build_threads(&[root, reply]);
        assert_eq!(threads[0].last_activity_at, "2026-01-01T00:10:00+00:00");
    }

    #[test]
    fn build_threads_filters_deleted() {
        let root = Comment {
            id: "r".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "root".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: Some("2026-01-02T00:00:00+00:00".into()),
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert!(build_threads(&[root]).is_empty());
    }

    // ── count_comments ──────────────────────────────────────────

    #[test]
    fn count_comments_empty() {
        assert_eq!(count_comments(&[]), 0);
    }

    #[test]
    fn count_comments_filters_deleted() {
        let alive = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "alive".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        let dead = Comment {
            deleted_at: Some("2026-01-02T00:00:00+00:00".into()),
            id: "b".into(),
            body: "dead".into(),
            ..alive.clone()
        };
        assert_eq!(count_comments(&[alive, dead]), 1);
    }

    // ── last_activity ───────────────────────────────────────────

    #[test]
    fn last_activity_none_for_empty() {
        assert!(last_activity(&[]).is_none());
    }

    #[test]
    fn last_activity_created_at_fallback() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert_eq!(last_activity(&[c]).unwrap(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn last_activity_prefers_updated_at() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "u".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: Some("2026-06-01T00:00:00+00:00".into()),
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert_eq!(last_activity(&[c]).unwrap(), "2026-06-01T00:00:00+00:00");
    }

    // ── truncate_body ───────────────────────────────────────────

    #[test]
    fn truncate_body_short_unchanged() {
        assert_eq!(truncate_body("hello", 140), "hello");
    }

    #[test]
    fn truncate_body_long_truncated() {
        let long = "a".repeat(200);
        let result = truncate_body(&long, 140);
        assert_eq!(result.chars().count(), 141); // 140 + ellipsis
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_body_newline_normalisation() {
        assert_eq!(truncate_body("hello\n  world\tfoo", 140), "hello world foo");
    }

    // ── can_edit ────────────────────────────────────────────────

    #[test]
    fn can_edit_author_match() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "user-1".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert!(can_edit(&c, "user-1", &[]));
    }

    #[test]
    fn can_edit_non_author_rejected() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "user-1".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert!(!can_edit(&c, "user-2", &[]));
    }

    #[test]
    fn can_edit_admin_roles() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "user-1".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: None,
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert!(can_edit(&c, "user-2", &["admin"]));
        assert!(can_edit(&c, "user-2", &["super-admin"]));
        assert!(can_edit(&c, "user-2", &["moderator"]));
        assert!(!can_edit(&c, "user-2", &["viewer"]));
    }

    #[test]
    fn can_edit_deleted_rejected() {
        let c = Comment {
            id: "a".into(),
            object_id: "o".into(),
            parent_id: None,
            author_id: "user-1".into(),
            author_name: "U".into(),
            body: "x".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: None,
            deleted_at: Some("2026-01-02T00:00:00+00:00".into()),
            reactions: IndexMap::new(),
            resolved: false,
            resolved_by: None,
            resolved_at: None,
        };
        assert!(!can_edit(&c, "user-1", &["admin"]));
    }

    #[test]
    fn widget_contributions_has_2_entries() {
        let contributions = widget_contributions();
        assert_eq!(contributions.len(), 2);
        let ids: Vec<&str> = contributions.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"comment-thread"));
        assert!(ids.contains(&"comment-count"));
    }

    #[test]
    fn widget_contributions_roundtrip_through_json() {
        for c in widget_contributions() {
            let json = serde_json::to_string(&c).unwrap();
            let back: crate::widget::WidgetContribution = serde_json::from_str(&json).unwrap();
            assert_eq!(back.id, c.id);
        }
    }
}
