//! Debouncing, deduplicating queue wrapper over [`NotificationStore`].
//!
//! Port of `interaction/notification/notification-queue.ts`. The
//! queue never owns a real async timer itself — callers drive time
//! forward via a [`TimerProvider`]. The default provider is the
//! monotonic wall clock + manual flush (Rust has no built-in timer
//! subsystem tied to a runtime the way the browser has).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use super::store::NotificationStore;
use super::types::{Notification, NotificationInput};

pub trait TimerProvider {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemTimer;

impl TimerProvider for SystemTimer {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct NotificationQueueOptions {
    pub debounce: Duration,
    pub dedup_window: Duration,
    pub timer: Box<dyn TimerProvider>,
}

impl Default for NotificationQueueOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::milliseconds(300),
            dedup_window: Duration::milliseconds(5000),
            timer: Box::new(SystemTimer),
        }
    }
}

struct PendingEntry {
    input: NotificationInput,
}

pub struct NotificationQueue {
    debounce: Duration,
    dedup_window: Duration,
    timer: Box<dyn TimerProvider>,
    queue: Vec<(String, PendingEntry)>,
    queue_index: HashMap<String, usize>,
    fallback_counter: u64,
    recent_deliveries: HashMap<String, DateTime<Utc>>,
    last_enqueue_at: Option<DateTime<Utc>>,
}

impl NotificationQueue {
    pub fn new() -> Self {
        Self::with_options(NotificationQueueOptions::default())
    }

    pub fn with_options(options: NotificationQueueOptions) -> Self {
        Self {
            debounce: options.debounce,
            dedup_window: options.dedup_window,
            timer: options.timer,
            queue: Vec::new(),
            queue_index: HashMap::new(),
            fallback_counter: 0,
            recent_deliveries: HashMap::new(),
            last_enqueue_at: None,
        }
    }

    pub fn enqueue(&mut self, input: NotificationInput) {
        let now = self.timer.now();
        let key = dedup_key(&input);

        if let Some(key) = key.clone() {
            if let Some(last_delivered) = self.recent_deliveries.get(&key).copied() {
                if now - last_delivered < self.dedup_window {
                    // Recently delivered: update any pending entry, otherwise drop.
                    if let Some(&idx) = self.queue_index.get(&key) {
                        self.queue[idx].1.input = input;
                    }
                    return;
                }
            }

            if let Some(&idx) = self.queue_index.get(&key) {
                self.queue[idx].1.input = input;
                self.last_enqueue_at = Some(now);
                return;
            }

            let idx = self.queue.len();
            self.queue.push((key.clone(), PendingEntry { input }));
            self.queue_index.insert(key, idx);
        } else {
            self.fallback_counter += 1;
            let fallback_key = format!("__unique_{}", self.fallback_counter);
            let idx = self.queue.len();
            self.queue
                .push((fallback_key.clone(), PendingEntry { input }));
            self.queue_index.insert(fallback_key, idx);
        }

        self.last_enqueue_at = Some(now);
    }

    /// True when the debounce window has elapsed since the last
    /// `enqueue` call and the queue has pending items.
    pub fn should_flush(&self) -> bool {
        match (self.last_enqueue_at, self.queue.is_empty()) {
            (None, _) | (_, true) => false,
            (Some(last), false) => self.timer.now() - last >= self.debounce,
        }
    }

    /// Deliver every queued notification to `store`, bypassing the
    /// debounce window. Returns the notifications actually created.
    pub fn flush(&mut self, store: &mut NotificationStore) -> Vec<Notification> {
        let now = self.timer.now();
        let mut delivered = Vec::with_capacity(self.queue.len());
        let drained: Vec<(String, PendingEntry)> = self.queue.drain(..).collect();
        self.queue_index.clear();
        self.last_enqueue_at = None;

        for (key, entry) in drained {
            let notification = store.add(entry.input);
            delivered.push(notification);
            if !key.starts_with("__unique_") {
                self.recent_deliveries.insert(key, now);
            }
        }

        self.clean_recent_deliveries(now);
        delivered
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub fn dispose(&mut self) {
        self.queue.clear();
        self.queue_index.clear();
        self.recent_deliveries.clear();
        self.last_enqueue_at = None;
    }

    fn clean_recent_deliveries(&mut self, now: DateTime<Utc>) {
        let cutoff = now - self.dedup_window;
        self.recent_deliveries.retain(|_, ts| *ts >= cutoff);
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn dedup_key(input: &NotificationInput) -> Option<String> {
    input
        .object_id
        .as_ref()
        .map(|id| format!("{}:{:?}", id, input.kind))
}

#[cfg(test)]
mod tests {
    use super::super::types::NotificationKind;
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct FakeTimer {
        now: Rc<Cell<DateTime<Utc>>>,
    }

    impl TimerProvider for FakeTimer {
        fn now(&self) -> DateTime<Utc> {
            self.now.get()
        }
    }

    fn fake_timer() -> (Rc<Cell<DateTime<Utc>>>, Box<dyn TimerProvider>) {
        let now = Rc::new(Cell::new(Utc::now()));
        let t = FakeTimer { now: now.clone() };
        (now, Box::new(t))
    }

    fn linked_input(title: &str, object_id: &str) -> NotificationInput {
        NotificationInput {
            title: title.into(),
            object_id: Some(object_id.into()),
            ..Default::default()
        }
    }

    #[test]
    fn single_enqueue_plus_flush_delivers_once() {
        let (_, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(NotificationInput {
            title: "hi".into(),
            ..Default::default()
        });
        let delivered = queue.flush(&mut store);
        assert_eq!(delivered.len(), 1);
        assert_eq!(queue.pending(), 0);
    }

    #[test]
    fn dedupes_within_queue_same_object_and_kind() {
        let (_, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(linked_input("first", "obj-1"));
        queue.enqueue(linked_input("second", "obj-1"));
        assert_eq!(queue.pending(), 1);
        let delivered = queue.flush(&mut store);
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].title, "second");
    }

    #[test]
    fn dedupes_after_delivery_within_window() {
        let (now, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(linked_input("first", "obj-1"));
        queue.flush(&mut store);
        // 2s later, within the 5s dedup window.
        now.set(now.get() + Duration::seconds(2));
        queue.enqueue(linked_input("second", "obj-1"));
        let delivered = queue.flush(&mut store);
        assert_eq!(delivered.len(), 0);
    }

    #[test]
    fn allows_redelivery_after_window() {
        let (now, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(linked_input("first", "obj-1"));
        queue.flush(&mut store);
        now.set(now.get() + Duration::seconds(10));
        queue.enqueue(linked_input("second", "obj-1"));
        let delivered = queue.flush(&mut store);
        assert_eq!(delivered.len(), 1);
    }

    #[test]
    fn keys_split_by_kind() {
        let (_, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(NotificationInput {
            title: "a".into(),
            kind: NotificationKind::Info,
            object_id: Some("obj-1".into()),
            ..Default::default()
        });
        queue.enqueue(NotificationInput {
            title: "b".into(),
            kind: NotificationKind::Warning,
            object_id: Some("obj-1".into()),
            ..Default::default()
        });
        assert_eq!(queue.pending(), 2);
        assert_eq!(queue.flush(&mut store).len(), 2);
    }

    #[test]
    fn notifications_without_object_id_are_never_deduped() {
        let (_, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        let mut store = NotificationStore::new();
        queue.enqueue(NotificationInput {
            title: "a".into(),
            ..Default::default()
        });
        queue.enqueue(NotificationInput {
            title: "b".into(),
            ..Default::default()
        });
        assert_eq!(queue.pending(), 2);
        assert_eq!(queue.flush(&mut store).len(), 2);
    }

    #[test]
    fn should_flush_waits_for_debounce() {
        let (now, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            debounce: Duration::milliseconds(300),
            ..Default::default()
        });
        queue.enqueue(NotificationInput {
            title: "a".into(),
            ..Default::default()
        });
        assert!(!queue.should_flush());
        now.set(now.get() + Duration::milliseconds(299));
        assert!(!queue.should_flush());
        now.set(now.get() + Duration::milliseconds(2));
        assert!(queue.should_flush());
    }

    #[test]
    fn dispose_clears_state() {
        let (_, timer) = fake_timer();
        let mut queue = NotificationQueue::with_options(NotificationQueueOptions {
            timer,
            ..Default::default()
        });
        queue.enqueue(NotificationInput {
            title: "a".into(),
            ..Default::default()
        });
        queue.dispose();
        assert_eq!(queue.pending(), 0);
        assert!(!queue.should_flush());
    }
}
