//! `notification` — in-memory notification store + debouncing queue.
//!
//! Port of `interaction/notification/*`:
//! - [`store::NotificationStore`] is the synchronous, framework-agnostic
//!   registry with eviction and subscription.
//! - [`queue::NotificationQueue`] is the batching/dedup wrapper that
//!   drains into a store on a pluggable timer.
//!
//! Persistence lives one layer up (`foundation::persistence`) — this
//! module deliberately knows nothing about Loro, daemon IPC, or disks.

pub mod queue;
pub mod store;
pub mod types;

pub use queue::{NotificationQueue, NotificationQueueOptions, TimerProvider};
pub use store::{NotificationListener, NotificationStore, NotificationStoreOptions};
pub use types::{
    Notification, NotificationChange, NotificationChangeType, NotificationFilter,
    NotificationInput, NotificationKind,
};
