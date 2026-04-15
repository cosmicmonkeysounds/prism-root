//! Synchronous in-memory notification registry.
//!
//! Port of `interaction/notification/notification-store.ts`. The
//! only departure from the TS version: `created_at`/`read_at`/
//! `dismissed_at`/`expires_at` are strongly-typed `DateTime<Utc>`
//! values so we can compare them without stringly-typed ISO
//! roundtrips, and `object_id`/`kind` dedup keys are computed from
//! the typed fields.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;

use super::types::{
    Notification, NotificationChange, NotificationChangeType, NotificationFilter,
    NotificationInput,
};

pub type NotificationListener = Box<dyn FnMut(&NotificationChange)>;
pub type IdGenerator = Box<dyn FnMut() -> String>;

pub struct NotificationStoreOptions {
    pub max_items: usize,
    pub id_generator: Option<IdGenerator>,
}

impl Default for NotificationStoreOptions {
    fn default() -> Self {
        Self {
            max_items: 200,
            id_generator: None,
        }
    }
}

pub struct NotificationStore {
    store: IndexMap<String, Notification>,
    listeners: HashMap<u64, NotificationListener>,
    next_listener_id: u64,
    max_items: usize,
    id_counter: u64,
    id_generator: Option<IdGenerator>,
}

pub struct NotificationSubscription {
    id: u64,
}

impl NotificationSubscription {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl NotificationStore {
    pub fn new() -> Self {
        Self::with_options(NotificationStoreOptions::default())
    }

    pub fn with_options(options: NotificationStoreOptions) -> Self {
        Self {
            store: IndexMap::new(),
            listeners: HashMap::new(),
            next_listener_id: 0,
            max_items: options.max_items,
            id_counter: 0,
            id_generator: options.id_generator,
        }
    }

    pub fn add(&mut self, input: NotificationInput) -> Notification {
        let now = Utc::now();
        let id = input.id.unwrap_or_else(|| self.generate_id());
        let notification = Notification {
            id: id.clone(),
            kind: input.kind,
            title: input.title,
            body: input.body,
            object_id: input.object_id,
            object_type: input.object_type,
            actor_id: input.actor_id,
            read: input.read.unwrap_or(false),
            pinned: input.pinned.unwrap_or(false),
            created_at: input.created_at.unwrap_or(now),
            read_at: None,
            dismissed_at: None,
            expires_at: input.expires_at,
            data: input.data,
        };

        self.store.insert(id, notification.clone());
        self.evict();
        self.emit(NotificationChange {
            kind: NotificationChangeType::Add,
            notification: Some(notification.clone()),
        });
        notification
    }

    pub fn mark_read(&mut self, id: &str) -> Option<Notification> {
        let entry = self.store.get_mut(id)?;
        if entry.read {
            return Some(entry.clone());
        }
        entry.read = true;
        entry.read_at = Some(Utc::now());
        let updated = entry.clone();
        self.emit(NotificationChange {
            kind: NotificationChangeType::Update,
            notification: Some(updated.clone()),
        });
        Some(updated)
    }

    pub fn mark_all_read(&mut self, filter: Option<&NotificationFilter>) -> usize {
        let now = Utc::now();
        let mut count = 0;
        for n in self.store.values_mut() {
            if n.read || n.dismissed_at.is_some() {
                continue;
            }
            if let Some(f) = filter {
                if !f.matches(n) {
                    continue;
                }
            }
            n.read = true;
            n.read_at = Some(now);
            count += 1;
        }
        if count > 0 {
            self.emit(NotificationChange {
                kind: NotificationChangeType::Update,
                notification: None,
            });
        }
        count
    }

    pub fn dismiss(&mut self, id: &str) -> Option<Notification> {
        let entry = self.store.get_mut(id)?;
        if entry.dismissed_at.is_some() {
            return Some(entry.clone());
        }
        entry.dismissed_at = Some(Utc::now());
        let updated = entry.clone();
        self.emit(NotificationChange {
            kind: NotificationChangeType::Dismiss,
            notification: Some(updated.clone()),
        });
        Some(updated)
    }

    pub fn dismiss_all(&mut self, filter: Option<&NotificationFilter>) -> usize {
        let now = Utc::now();
        let mut count = 0;
        for n in self.store.values_mut() {
            if n.dismissed_at.is_some() {
                continue;
            }
            if let Some(f) = filter {
                if !f.matches(n) {
                    continue;
                }
            }
            n.dismissed_at = Some(now);
            count += 1;
        }
        if count > 0 {
            self.emit(NotificationChange {
                kind: NotificationChangeType::Dismiss,
                notification: None,
            });
        }
        count
    }

    pub fn pin(&mut self, id: &str) -> Option<Notification> {
        self.set_pin(id, true)
    }

    pub fn unpin(&mut self, id: &str) -> Option<Notification> {
        self.set_pin(id, false)
    }

    fn set_pin(&mut self, id: &str, pinned: bool) -> Option<Notification> {
        let entry = self.store.get_mut(id)?;
        entry.pinned = pinned;
        let updated = entry.clone();
        self.emit(NotificationChange {
            kind: NotificationChangeType::Update,
            notification: Some(updated.clone()),
        });
        Some(updated)
    }

    pub fn get(&self, id: &str) -> Option<&Notification> {
        self.store.get(id)
    }

    pub fn get_all(&self, filter: Option<&NotificationFilter>) -> Vec<Notification> {
        let now = Utc::now();
        let mut result: Vec<Notification> = self
            .store
            .values()
            .filter(|n| n.dismissed_at.is_none())
            .filter(|n| !is_expired(n, now))
            .filter(|n| filter.is_none_or(|f| f.matches(n)))
            .cloned()
            .collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result
    }

    pub fn get_unread_count(&self, filter: Option<&NotificationFilter>) -> usize {
        let now = Utc::now();
        self.store
            .values()
            .filter(|n| n.dismissed_at.is_none() && !n.read)
            .filter(|n| !is_expired(n, now))
            .filter(|n| filter.is_none_or(|f| f.matches(n)))
            .count()
    }

    /// Active (non-dismissed, non-expired) notification count.
    pub fn size(&self) -> usize {
        let now = Utc::now();
        self.store
            .values()
            .filter(|n| n.dismissed_at.is_none() && !is_expired(n, now))
            .count()
    }

    pub fn subscribe(&mut self, listener: NotificationListener) -> NotificationSubscription {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.insert(id, listener);
        NotificationSubscription { id }
    }

    pub fn unsubscribe(&mut self, subscription: NotificationSubscription) {
        self.listeners.remove(&subscription.id);
    }

    pub fn hydrate(&mut self, mut items: Vec<Notification>) {
        self.store.clear();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        for item in items {
            self.store.insert(item.id.clone(), item);
        }
        self.evict();
        self.emit(NotificationChange {
            kind: NotificationChangeType::Clear,
            notification: None,
        });
    }

    /// Remove all dismissed unpinned notifications from memory.
    pub fn clear_dismissed(&mut self) -> usize {
        let removed: Vec<String> = self
            .store
            .iter()
            .filter(|(_, n)| n.dismissed_at.is_some() && !n.pinned)
            .map(|(id, _)| id.clone())
            .collect();
        let count = removed.len();
        for id in removed {
            self.store.shift_remove(&id);
        }
        if count > 0 {
            self.emit(NotificationChange {
                kind: NotificationChangeType::Clear,
                notification: None,
            });
        }
        count
    }

    fn evict(&mut self) {
        if self.store.len() <= self.max_items {
            return;
        }
        // Priority: dismissed unpinned (oldest first), then read unpinned (oldest first).
        let mut dismissed: Vec<(String, DateTime<Utc>)> = self
            .store
            .iter()
            .filter(|(_, n)| n.dismissed_at.is_some() && !n.pinned)
            .map(|(id, n)| (id.clone(), n.created_at))
            .collect();
        dismissed.sort_by(|a, b| a.1.cmp(&b.1));

        let mut read: Vec<(String, DateTime<Utc>)> = self
            .store
            .iter()
            .filter(|(_, n)| n.dismissed_at.is_none() && n.read && !n.pinned)
            .map(|(id, n)| (id.clone(), n.created_at))
            .collect();
        read.sort_by(|a, b| a.1.cmp(&b.1));

        let mut to_remove = self.store.len().saturating_sub(self.max_items);
        for (id, _) in dismissed.into_iter().chain(read.into_iter()) {
            if to_remove == 0 {
                break;
            }
            self.store.shift_remove(&id);
            to_remove -= 1;
        }
    }

    fn emit(&mut self, change: NotificationChange) {
        for listener in self.listeners.values_mut() {
            listener(&change);
        }
    }

    fn generate_id(&mut self) -> String {
        if let Some(gen) = self.id_generator.as_mut() {
            return gen();
        }
        self.id_counter += 1;
        format!("notif-{}", self.id_counter)
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

fn is_expired(n: &Notification, now: DateTime<Utc>) -> bool {
    match n.expires_at {
        Some(exp) => exp <= now,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::NotificationKind;
    use super::*;
    use chrono::Duration;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn input(title: &str, kind: NotificationKind) -> NotificationInput {
        NotificationInput {
            title: title.into(),
            kind,
            ..Default::default()
        }
    }

    #[test]
    fn add_assigns_id_and_returns_notification() {
        let mut store = NotificationStore::new();
        let n = store.add(input("hi", NotificationKind::Info));
        assert_eq!(n.title, "hi");
        assert!(!n.read);
        assert!(!n.pinned);
        assert_eq!(store.size(), 1);
        assert_eq!(n.id, "notif-1");
    }

    #[test]
    fn add_respects_supplied_id() {
        let mut store = NotificationStore::new();
        let n = store.add(NotificationInput {
            id: Some("custom".into()),
            ..input("hi", NotificationKind::Info)
        });
        assert_eq!(n.id, "custom");
    }

    #[test]
    fn mark_read_sets_read_and_read_at() {
        let mut store = NotificationStore::new();
        let n = store.add(input("hi", NotificationKind::Mention));
        let updated = store.mark_read(&n.id).unwrap();
        assert!(updated.read);
        assert!(updated.read_at.is_some());
    }

    #[test]
    fn mark_read_is_idempotent() {
        let mut store = NotificationStore::new();
        let n = store.add(input("hi", NotificationKind::Info));
        store.mark_read(&n.id);
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        store.subscribe(Box::new(move |_| *cc.borrow_mut() += 1));
        store.mark_read(&n.id);
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn mark_all_read_with_filter_applies() {
        let mut store = NotificationStore::new();
        store.add(input("a", NotificationKind::Info));
        store.add(input("b", NotificationKind::Warning));
        let count = store.mark_all_read(Some(&NotificationFilter {
            kind: Some(vec![NotificationKind::Info]),
            ..Default::default()
        }));
        assert_eq!(count, 1);
        assert_eq!(store.get_unread_count(None), 1);
    }

    #[test]
    fn dismiss_marks_dismissed_and_excludes_from_get_all() {
        let mut store = NotificationStore::new();
        let n = store.add(input("hi", NotificationKind::Info));
        store.dismiss(&n.id);
        assert_eq!(store.get_all(None).len(), 0);
        assert_eq!(store.size(), 0);
    }

    #[test]
    fn pin_and_unpin_persist() {
        let mut store = NotificationStore::new();
        let n = store.add(input("hi", NotificationKind::Info));
        store.pin(&n.id);
        assert!(store.get(&n.id).unwrap().pinned);
        store.unpin(&n.id);
        assert!(!store.get(&n.id).unwrap().pinned);
    }

    #[test]
    fn get_all_returns_newest_first() {
        let mut store = NotificationStore::new();
        let base = Utc::now();
        for (i, t) in ["a", "b", "c"].iter().enumerate() {
            store.add(NotificationInput {
                title: (*t).into(),
                created_at: Some(base + Duration::seconds(i as i64)),
                ..Default::default()
            });
        }
        let all = store.get_all(None);
        assert_eq!(all.iter().map(|n| n.title.clone()).collect::<Vec<_>>(), vec!["c", "b", "a"]);
    }

    #[test]
    fn get_all_excludes_expired() {
        let mut store = NotificationStore::new();
        store.add(NotificationInput {
            title: "past".into(),
            expires_at: Some(Utc::now() - Duration::seconds(60)),
            ..Default::default()
        });
        store.add(input("live", NotificationKind::Info));
        assert_eq!(store.get_all(None).len(), 1);
    }

    #[test]
    fn filter_matches_since_strictly() {
        let mut store = NotificationStore::new();
        let t = Utc::now();
        store.add(NotificationInput {
            title: "same".into(),
            created_at: Some(t),
            ..Default::default()
        });
        store.add(NotificationInput {
            title: "later".into(),
            created_at: Some(t + Duration::seconds(1)),
            ..Default::default()
        });
        let hits = store.get_all(Some(&NotificationFilter {
            since: Some(t),
            ..Default::default()
        }));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "later");
    }

    #[test]
    fn evict_drops_dismissed_before_read() {
        let mut store = NotificationStore::with_options(NotificationStoreOptions {
            max_items: 2,
            id_generator: None,
        });
        let a = store.add(input("a", NotificationKind::Info));
        store.dismiss(&a.id);
        let b = store.add(input("b", NotificationKind::Info));
        store.mark_read(&b.id);
        store.add(input("c", NotificationKind::Info));
        assert!(store.get(&a.id).is_none());
        assert!(store.get(&b.id).is_some());
    }

    #[test]
    fn evict_drops_read_when_no_dismissed_unpinned() {
        let mut store = NotificationStore::with_options(NotificationStoreOptions {
            max_items: 2,
            id_generator: None,
        });
        let a = store.add(input("a", NotificationKind::Info));
        store.mark_read(&a.id);
        let b = store.add(input("b", NotificationKind::Info));
        store.mark_read(&b.id);
        store.add(input("c", NotificationKind::Info));
        assert!(store.get(&a.id).is_none());
        assert!(store.get(&b.id).is_some());
    }

    #[test]
    fn evict_preserves_pinned_and_unread() {
        let mut store = NotificationStore::with_options(NotificationStoreOptions {
            max_items: 2,
            id_generator: None,
        });
        let a = store.add(input("pinned", NotificationKind::Info));
        store.pin(&a.id);
        let b = store.add(input("unread", NotificationKind::Info));
        store.add(input("c", NotificationKind::Info));
        // Nothing is dismissed/read/unpinned, so no eviction even though over-cap.
        assert!(store.get(&a.id).is_some());
        assert!(store.get(&b.id).is_some());
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let mut store = NotificationStore::new();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        let sub = store.subscribe(Box::new(move |_| *cc.borrow_mut() += 1));
        store.add(input("a", NotificationKind::Info));
        assert_eq!(*calls.borrow(), 1);
        store.unsubscribe(sub);
        store.add(input("b", NotificationKind::Info));
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn hydrate_replaces_and_sorts() {
        let mut store = NotificationStore::new();
        store.add(input("stale", NotificationKind::Info));
        let t = Utc::now();
        let items = vec![
            Notification {
                id: "1".into(),
                kind: NotificationKind::Info,
                title: "older".into(),
                body: None,
                object_id: None,
                object_type: None,
                actor_id: None,
                read: false,
                pinned: false,
                created_at: t,
                read_at: None,
                dismissed_at: None,
                expires_at: None,
                data: Default::default(),
            },
            Notification {
                id: "2".into(),
                kind: NotificationKind::Info,
                title: "newer".into(),
                body: None,
                object_id: None,
                object_type: None,
                actor_id: None,
                read: false,
                pinned: false,
                created_at: t + Duration::seconds(5),
                read_at: None,
                dismissed_at: None,
                expires_at: None,
                data: Default::default(),
            },
        ];
        store.hydrate(items);
        let all = store.get_all(None);
        assert_eq!(all[0].title, "newer");
        assert_eq!(all[1].title, "older");
    }

    #[test]
    fn clear_dismissed_drops_dismissed_unpinned() {
        let mut store = NotificationStore::new();
        let a = store.add(input("a", NotificationKind::Info));
        let b = store.add(input("b", NotificationKind::Info));
        store.dismiss(&a.id);
        store.pin(&b.id);
        store.dismiss(&b.id);
        let count = store.clear_dismissed();
        assert_eq!(count, 1);
        assert!(store.get(&a.id).is_none());
        assert!(store.get(&b.id).is_some());
    }

    #[test]
    fn object_id_filter_matches() {
        let mut store = NotificationStore::new();
        store.add(NotificationInput {
            title: "linked".into(),
            object_id: Some("obj-1".into()),
            ..Default::default()
        });
        store.add(input("bare", NotificationKind::Info));
        let hits = store.get_all(Some(&NotificationFilter {
            object_id: Some("obj-1".into()),
            ..Default::default()
        }));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "linked");
    }
}
