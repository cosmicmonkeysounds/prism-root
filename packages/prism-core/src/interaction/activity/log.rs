//! Append-only per-object activity log.
//!
//! Port of `interaction/activity/activity-log.ts`. Each
//! `ActivityStore` keeps a size-bounded FIFO of events per object id
//! plus a set of per-object listeners. `record` / `hydrate` notify
//! subscribers with the newest-first slice that `get_events` would
//! return.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityVerb {
    Created,
    Updated,
    Deleted,
    Restored,
    Moved,
    Renamed,
    StatusChanged,
    Commented,
    Mentioned,
    Assigned,
    Unassigned,
    Attached,
    Detached,
    Linked,
    Unlinked,
    Completed,
    Reopened,
    Blocked,
    Unblocked,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub before: JsonValue,
    pub after: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: String,
    pub object_id: String,
    pub verb: ActivityVerb,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<FieldChange>,
    /// `Some(None)` means "explicitly root", `None` means "field
    /// not applicable to this verb". We use the outer `Option` for
    /// applicability and the inner `Option<String>` for root-vs-id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_parent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_parent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub meta: HashMap<String, JsonValue>,
    pub created_at: DateTime<Utc>,
}

/// Keyword-style input for `ActivityStore::record`. The store fills
/// in `id` and `created_at`.
#[derive(Debug, Clone)]
pub struct ActivityEventInput {
    pub object_id: String,
    pub verb: ActivityVerb,
    pub actor_id: Option<String>,
    pub actor_name: Option<String>,
    pub changes: Vec<FieldChange>,
    pub from_parent_id: Option<Option<String>>,
    pub to_parent_id: Option<Option<String>>,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub meta: HashMap<String, JsonValue>,
}

impl ActivityEventInput {
    pub fn new(object_id: impl Into<String>, verb: ActivityVerb) -> Self {
        Self {
            object_id: object_id.into(),
            verb,
            actor_id: None,
            actor_name: None,
            changes: Vec::new(),
            from_parent_id: None,
            to_parent_id: None,
            from_status: None,
            to_status: None,
            meta: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityDescription {
    pub text: String,
    pub html: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityGroup {
    pub label: &'static str,
    pub events: Vec<ActivityEvent>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GetEventsOptions {
    pub limit: Option<usize>,
    pub before: Option<DateTime<Utc>>,
}

pub type ActivityListener = Box<dyn FnMut(&[ActivityEvent])>;
pub type IdGenerator = Box<dyn FnMut() -> String>;

pub struct ActivityStoreOptions {
    pub max_per_object: usize,
    pub id_generator: Option<IdGenerator>,
}

impl Default for ActivityStoreOptions {
    fn default() -> Self {
        Self {
            max_per_object: 500,
            id_generator: None,
        }
    }
}

pub struct ActivitySubscription {
    object_id: String,
    id: u64,
}

impl ActivitySubscription {
    pub fn object_id(&self) -> &str {
        &self.object_id
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

pub struct ActivityStore {
    events: HashMap<String, Vec<ActivityEvent>>,
    listeners: HashMap<String, Vec<(u64, ActivityListener)>>,
    max_per_object: usize,
    id_generator: Option<IdGenerator>,
    next_listener_id: u64,
    fallback_counter: u64,
}

impl ActivityStore {
    pub fn new() -> Self {
        Self::with_options(ActivityStoreOptions::default())
    }

    pub fn with_options(options: ActivityStoreOptions) -> Self {
        Self {
            events: HashMap::new(),
            listeners: HashMap::new(),
            max_per_object: options.max_per_object,
            id_generator: options.id_generator,
            next_listener_id: 0,
            fallback_counter: 0,
        }
    }

    pub fn record(&mut self, input: ActivityEventInput) -> ActivityEvent {
        let id = self.generate_id();
        let event = ActivityEvent {
            id,
            object_id: input.object_id.clone(),
            verb: input.verb,
            actor_id: input.actor_id,
            actor_name: input.actor_name,
            changes: input.changes,
            from_parent_id: input.from_parent_id,
            to_parent_id: input.to_parent_id,
            from_status: input.from_status,
            to_status: input.to_status,
            meta: input.meta,
            created_at: Utc::now(),
        };

        let bucket = self.events.entry(input.object_id.clone()).or_default();
        bucket.push(event.clone());
        Self::trim(bucket, self.max_per_object);
        self.notify(&input.object_id);
        event
    }

    pub fn get_events(&self, object_id: &str, opts: GetEventsOptions) -> Vec<ActivityEvent> {
        let Some(bucket) = self.events.get(object_id) else {
            return Vec::new();
        };
        let mut results: Vec<ActivityEvent> = bucket.iter().rev().cloned().collect();
        if let Some(before) = opts.before {
            results.retain(|e| e.created_at < before);
        }
        if let Some(limit) = opts.limit {
            results.truncate(limit);
        }
        results
    }

    pub fn get_latest(&self, object_id: &str) -> Option<ActivityEvent> {
        self.events.get(object_id)?.last().cloned()
    }

    pub fn get_event_count(&self, object_id: &str) -> usize {
        self.events.get(object_id).map(|b| b.len()).unwrap_or(0)
    }

    pub fn hydrate(&mut self, object_id: &str, mut incoming: Vec<ActivityEvent>) {
        incoming.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Self::trim(&mut incoming, self.max_per_object);
        self.events.insert(object_id.to_string(), incoming);
        self.notify(object_id);
    }

    pub fn subscribe(
        &mut self,
        object_id: &str,
        listener: ActivityListener,
    ) -> ActivitySubscription {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners
            .entry(object_id.to_string())
            .or_default()
            .push((id, listener));
        ActivitySubscription {
            object_id: object_id.to_string(),
            id,
        }
    }

    pub fn unsubscribe(&mut self, subscription: ActivitySubscription) {
        if let Some(bucket) = self.listeners.get_mut(&subscription.object_id) {
            bucket.retain(|(id, _)| *id != subscription.id);
            if bucket.is_empty() {
                self.listeners.remove(&subscription.object_id);
            }
        }
    }

    pub fn to_map(&self) -> HashMap<String, Vec<ActivityEvent>> {
        self.events.clone()
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.listeners.clear();
    }

    fn trim(bucket: &mut Vec<ActivityEvent>, max: usize) {
        if bucket.len() > max {
            let drop = bucket.len() - max;
            bucket.drain(0..drop);
        }
    }

    fn notify(&mut self, object_id: &str) {
        let snapshot = self.get_events(object_id, GetEventsOptions::default());
        let Some(bucket) = self.listeners.get_mut(object_id) else {
            return;
        };
        for (_, listener) in bucket.iter_mut() {
            listener(&snapshot);
        }
    }

    fn generate_id(&mut self) -> String {
        if let Some(gen) = self.id_generator.as_mut() {
            return gen();
        }
        self.fallback_counter += 1;
        format!("act-{}", self.fallback_counter)
    }
}

impl Default for ActivityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn counter_gen() -> IdGenerator {
        let counter = Rc::new(RefCell::new(0u64));
        Box::new(move || {
            *counter.borrow_mut() += 1;
            format!("id-{}", counter.borrow())
        })
    }

    fn store_with_counter() -> ActivityStore {
        ActivityStore::with_options(ActivityStoreOptions {
            max_per_object: 500,
            id_generator: Some(counter_gen()),
        })
    }

    fn updated_input() -> ActivityEventInput {
        ActivityEventInput::new("obj-1", ActivityVerb::Updated)
    }

    #[test]
    fn record_stamps_id_and_timestamp() {
        let mut store = store_with_counter();
        let event = store.record(updated_input());
        assert_eq!(event.id, "id-1");
        assert_eq!(event.object_id, "obj-1");
        assert_eq!(event.verb, ActivityVerb::Updated);
    }

    #[test]
    fn record_preserves_optional_fields() {
        let mut store = store_with_counter();
        let mut input = updated_input();
        input.actor_id = Some("user-1".into());
        input.actor_name = Some("Alice".into());
        input.changes = vec![FieldChange {
            field: "name".into(),
            before: json!("Old"),
            after: json!("New"),
        }];
        input.meta.insert("reason".into(), json!("testing"));
        let event = store.record(input);
        assert_eq!(event.actor_id.as_deref(), Some("user-1"));
        assert_eq!(event.changes.len(), 1);
        assert_eq!(event.meta.get("reason"), Some(&json!("testing")));
    }

    #[test]
    fn record_preserves_move_fields() {
        let mut store = store_with_counter();
        let mut input = ActivityEventInput::new("obj-1", ActivityVerb::Moved);
        input.from_parent_id = Some(Some("p1".into()));
        input.to_parent_id = Some(Some("p2".into()));
        let event = store.record(input);
        assert_eq!(event.from_parent_id, Some(Some("p1".into())));
        assert_eq!(event.to_parent_id, Some(Some("p2".into())));
    }

    #[test]
    fn record_preserves_status_fields() {
        let mut store = store_with_counter();
        let mut input = ActivityEventInput::new("obj-1", ActivityVerb::StatusChanged);
        input.from_status = Some("todo".into());
        input.to_status = Some("done".into());
        let event = store.record(input);
        assert_eq!(event.from_status.as_deref(), Some("todo"));
        assert_eq!(event.to_status.as_deref(), Some("done"));
    }

    #[test]
    fn get_events_returns_newest_first() {
        let mut store = store_with_counter();
        for _ in 0..3 {
            store.record(updated_input());
        }
        let events = store.get_events("obj-1", GetEventsOptions::default());
        assert_eq!(events[0].id, "id-3");
        assert_eq!(events[2].id, "id-1");
    }

    #[test]
    fn get_events_respects_limit() {
        let mut store = store_with_counter();
        for _ in 0..5 {
            store.record(updated_input());
        }
        let events = store.get_events(
            "obj-1",
            GetEventsOptions {
                limit: Some(2),
                ..Default::default()
            },
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "id-5");
    }

    #[test]
    fn get_events_respects_before_cursor() {
        let mut store = store_with_counter();
        store.record(updated_input());
        std::thread::sleep(std::time::Duration::from_millis(10));
        let cursor = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.record(updated_input());
        let events = store.get_events(
            "obj-1",
            GetEventsOptions {
                before: Some(cursor),
                ..Default::default()
            },
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "id-1");
    }

    #[test]
    fn get_latest_returns_last_recorded() {
        let mut store = store_with_counter();
        store.record(updated_input());
        store.record(updated_input());
        assert_eq!(store.get_latest("obj-1").unwrap().id, "id-2");
        assert!(store.get_latest("missing").is_none());
    }

    #[test]
    fn get_event_count_reflects_records() {
        let mut store = store_with_counter();
        store.record(updated_input());
        store.record(updated_input());
        assert_eq!(store.get_event_count("obj-1"), 2);
        assert_eq!(store.get_event_count("missing"), 0);
    }

    #[test]
    fn trim_drops_oldest_when_over_capacity() {
        let mut store = ActivityStore::with_options(ActivityStoreOptions {
            max_per_object: 3,
            id_generator: Some(counter_gen()),
        });
        for _ in 0..5 {
            store.record(updated_input());
        }
        assert_eq!(store.get_event_count("obj-1"), 3);
        let events = store.get_events("obj-1", GetEventsOptions::default());
        assert_eq!(events[0].id, "id-5");
        assert_eq!(events[2].id, "id-3");
    }

    #[test]
    fn hydrate_replaces_and_notifies_subscribers() {
        let mut store = store_with_counter();
        let calls = Rc::new(RefCell::new(Vec::<usize>::new()));
        let cc = calls.clone();
        store.subscribe(
            "obj-1",
            Box::new(move |events| cc.borrow_mut().push(events.len())),
        );
        let incoming = vec![
            ActivityEvent {
                id: "ext-1".into(),
                object_id: "obj-1".into(),
                verb: ActivityVerb::Created,
                actor_id: None,
                actor_name: None,
                changes: vec![],
                from_parent_id: None,
                to_parent_id: None,
                from_status: None,
                to_status: None,
                meta: HashMap::new(),
                created_at: Utc::now() - Duration::seconds(10),
            },
            ActivityEvent {
                id: "ext-2".into(),
                object_id: "obj-1".into(),
                verb: ActivityVerb::Updated,
                actor_id: None,
                actor_name: None,
                changes: vec![],
                from_parent_id: None,
                to_parent_id: None,
                from_status: None,
                to_status: None,
                meta: HashMap::new(),
                created_at: Utc::now(),
            },
        ];
        store.hydrate("obj-1", incoming);
        assert_eq!(store.get_event_count("obj-1"), 2);
        assert_eq!(*calls.borrow(), vec![2]);
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let mut store = store_with_counter();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        let sub = store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.record(updated_input());
        store.unsubscribe(sub);
        store.record(updated_input());
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn subscribers_scoped_per_object() {
        let mut store = store_with_counter();
        let a = Rc::new(RefCell::new(0usize));
        let b = Rc::new(RefCell::new(0usize));
        let ac = a.clone();
        let bc = b.clone();
        store.subscribe("obj-1", Box::new(move |_| *ac.borrow_mut() += 1));
        store.subscribe("obj-2", Box::new(move |_| *bc.borrow_mut() += 1));
        store.record(ActivityEventInput::new("obj-1", ActivityVerb::Updated));
        store.record(ActivityEventInput::new("obj-1", ActivityVerb::Updated));
        store.record(ActivityEventInput::new("obj-2", ActivityVerb::Updated));
        assert_eq!(*a.borrow(), 2);
        assert_eq!(*b.borrow(), 1);
    }

    #[test]
    fn to_map_returns_full_state() {
        let mut store = store_with_counter();
        store.record(updated_input());
        store.record(ActivityEventInput::new("obj-2", ActivityVerb::Created));
        let map = store.to_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["obj-1"].len(), 1);
        assert_eq!(map["obj-2"].len(), 1);
    }

    #[test]
    fn clear_empties_store_and_listeners() {
        let mut store = store_with_counter();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe("obj-1", Box::new(move |_| *cc.borrow_mut() += 1));
        store.record(updated_input());
        store.clear();
        store.record(updated_input());
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(store.get_event_count("obj-1"), 1);
    }
}
