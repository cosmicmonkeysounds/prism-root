# activity

Append-only audit trail of GraphObject mutations. The `ActivityStore` is an in-memory per-object ring buffer with subscriptions and hydration; the `ActivityTracker` watches any `TrackableStore` and auto-derives events (`created`, `updated`, `renamed`, `moved`, `status-changed`, `deleted`, `restored`, ...) from structural diffs.

## Import

```ts
import {
  createActivityStore,
  createActivityTracker,
  formatActivity,
} from "@prism/core/activity";
```

## Key exports

- `createActivityStore(options?)` — factory for `ActivityStore` with `record`/`getEvents`/`getLatest`/`getEventCount`/`hydrate`/`subscribe`/`toJSON`/`clear`.
- `createActivityTracker({ activityStore, ignoredFields?, actorId?, actorName? })` — returns `{ track, untrackAll, trackedIds }`.
- `formatActivity(event, opts?)` — renders an `ActivityEvent` to `{ text, html? }`.
- `formatFieldName(field)` / `formatFieldValue(value, field?)` — human-readable field and value renderers.
- `groupActivityByDate(events)` — buckets events into `Today` / `Yesterday` / `This week` / `Earlier`.
- Types: `ActivityEvent`, `ActivityVerb` (20 verbs), `FieldChange`, `ActivityDescription`, `ActivityGroup`, `ActivityStoreOptions`, `ActivityListener`, `TrackableStore`.

## Usage

```ts
import { createActivityStore, createActivityTracker } from "@prism/core/activity";

const activity = createActivityStore({ maxPerObject: 500 });
const tracker = createActivityTracker({
  activityStore: activity,
  actorId: "did:key:...",
});

// Watch a GraphObject — diffs are recorded automatically on every change.
const untrack = tracker.track("obj-123", collectionStore);

// Read newest-first
const recent = activity.getEvents("obj-123", { limit: 20 });
```
