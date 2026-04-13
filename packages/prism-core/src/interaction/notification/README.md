# notification

In-memory notification registry with eviction, filtering, and debounced batching. `NotificationStore` manages 8 kinds (`system`/`mention`/`activity`/`reminder`/`info`/`success`/`warning`/`error`) with add/mark-read/dismiss/pin, filter queries, and an eviction policy (dismissed unpinned → read unpinned → oldest first) once capacity is reached. `NotificationQueue` wraps a store with debouncing and dedup by `(objectId, kind)`.

## Import

```ts
import {
  createNotificationStore,
  createNotificationQueue,
} from "@prism/core/notification";
```

## Key exports

- `createNotificationStore({ maxItems?, generateId? })` — returns `NotificationStore` with `add`/`markRead`/`markAllRead`/`dismiss`/`pin`/`unpin`/`query`/`subscribe`/`clear`/`hydrate`.
- `createNotificationQueue(store, { debounceMs?, dedupWindowMs?, timers? })` — returns `NotificationQueue` with `enqueue`/`flush`/`pending`/`dispose`.
- Types: `Notification`, `NotificationKind`, `NotificationInput`, `NotificationFilter`, `NotificationChange`, `NotificationChangeType`, `NotificationListener`, `NotificationStoreOptions`, `NotificationQueueOptions`, `TimerProvider`.

## Usage

```ts
import {
  createNotificationStore,
  createNotificationQueue,
} from "@prism/core/notification";

const notes = createNotificationStore({ maxItems: 200 });
const queue = createNotificationQueue(notes, { debounceMs: 300, dedupWindowMs: 5000 });

queue.enqueue({
  kind: "mention",
  title: "You were mentioned",
  objectId: "obj-123",
});

// Later: direct add
notes.add({ kind: "success", title: "Saved" });
```
