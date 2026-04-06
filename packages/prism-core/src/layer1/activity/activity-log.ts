/**
 * Activity Log — Types + ActivityStore
 *
 * Append-only audit trail of GraphObject mutations.
 * ActivityStore is an in-memory per-object ring buffer with subscriptions,
 * hydration, and serialisation. No external dependencies.
 */

// ── Verb ───────────────────────────────────────────────────────────────────────

export type ActivityVerb =
  | "created"
  | "updated"
  | "deleted"
  | "restored"
  | "moved"
  | "renamed"
  | "status-changed"
  | "commented"
  | "mentioned"
  | "assigned"
  | "unassigned"
  | "attached"
  | "detached"
  | "linked"
  | "unlinked"
  | "completed"
  | "reopened"
  | "blocked"
  | "unblocked"
  | "custom";

// ── Field change ───────────────────────────────────────────────────────────────

/** Records the before/after value of a single field on an 'updated' event. */
export interface FieldChange {
  field: string;
  before: unknown;
  after: unknown;
}

// ── Event ──────────────────────────────────────────────────────────────────────

/**
 * One immutable audit record for a GraphObject mutation.
 *
 * Only the fields relevant to the verb need be populated:
 *   - 'updated'        → changes[]
 *   - 'moved'          → fromParentId / toParentId
 *   - 'status-changed' → fromStatus / toStatus
 *   - 'renamed'        → changes[] with field='name'
 *   - all verbs        → actorId / actorName / meta are always optional
 */
export interface ActivityEvent {
  /** Unique ID — assigned by ActivityStore.record() */
  id: string;

  /** The object this event belongs to. */
  objectId: string;

  verb: ActivityVerb;

  /** Identifier of the user / system that caused the event. */
  actorId?: string;

  /** Display name of the actor (denormalised for rendering without a lookup). */
  actorName?: string;

  /** For 'updated' and 'renamed': which fields changed and their before/after values. */
  changes?: FieldChange[];

  /** For 'moved': the parent the object came from (null = was root). */
  fromParentId?: string | null;

  /** For 'moved': the parent the object moved to (null = now root). */
  toParentId?: string | null;

  /** For 'status-changed': the previous status string. */
  fromStatus?: string;

  /** For 'status-changed': the new status string. */
  toStatus?: string;

  /** Arbitrary caller-supplied context (edge IDs, comment text, etc.). */
  meta?: Record<string, unknown>;

  /** ISO 8601 datetime — assigned by ActivityStore.record(). */
  createdAt: string;
}

// ── Input (what callers pass to record()) ──────────────────────────────────────

export type ActivityEventInput = Omit<ActivityEvent, "id" | "createdAt">;

// ── Description ────────────────────────────────────────────────────────────────

/** Human-readable rendering of an ActivityEvent. */
export interface ActivityDescription {
  /** Plain-text summary, e.g. "John changed status from Todo to Done" */
  text: string;
  /** HTML version with entity names wrapped in <b>. */
  html?: string;
}

// ── Date grouping ──────────────────────────────────────────────────────────────

/** A labelled bucket of ActivityEvents for timeline rendering. */
export interface ActivityGroup {
  /** Human-readable bucket label: "Today", "Yesterday", "This week", "Earlier" */
  label: string;
  events: ActivityEvent[];
}

// ── Store options ──────────────────────────────────────────────────────────────

export interface ActivityStoreOptions {
  /**
   * Maximum number of events stored per object.
   * When exceeded the oldest events are dropped.
   * @default 500
   */
  maxPerObject?: number;

  /** Override ID generation for deterministic testing. */
  generateId?: () => string;
}

// ── Listener ───────────────────────────────────────────────────────────────────

export type ActivityListener = (events: ActivityEvent[]) => void;

// ── Store interface ────────────────────────────────────────────────────────────

export interface ActivityStore {
  /** Record a new event. Stamps id and createdAt automatically. Returns the completed event. */
  record(input: ActivityEventInput): ActivityEvent;

  /** Returns events for an object, newest-first. */
  getEvents(
    objectId: string,
    opts?: { limit?: number; before?: string },
  ): ActivityEvent[];

  /** Returns the most recent event for the object, or null. */
  getLatest(objectId: string): ActivityEvent | null;

  /** Returns the total number of events stored for the object. */
  getEventCount(objectId: string): number;

  /** Replaces the local event list for an object (for hydration from persistence). */
  hydrate(objectId: string, events: ActivityEvent[]): void;

  /** Subscribe to changes for a specific object. Returns unsubscribe function. */
  subscribe(objectId: string, listener: ActivityListener): () => void;

  /** Returns the full store as a plain JSON-serialisable object. */
  toJSON(): Record<string, ActivityEvent[]>;

  /** Removes all events and listeners. */
  clear(): void;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

let _seq = 0;

function defaultGenerateId(): string {
  const ts = Date.now().toString(36);
  const seq = (++_seq).toString(36);
  const rnd = Math.random().toString(36).slice(2, 6);
  return `${ts}-${seq}-${rnd}`;
}

// ── Factory ────────────────────────────────────────────────────────────────────

export function createActivityStore(
  options?: ActivityStoreOptions,
): ActivityStore {
  const maxPerObject = options?.maxPerObject ?? 500;
  const generateId = options?.generateId ?? defaultGenerateId;

  /** objectId → ordered array of events (oldest first) */
  const events = new Map<string, ActivityEvent[]>();

  /** objectId → Set<Listener> */
  const listeners = new Map<string, Set<ActivityListener>>();

  // ── Private helpers ────────────────────────────────────────────────────────

  function getOrCreate(objectId: string): ActivityEvent[] {
    let bucket = events.get(objectId);
    if (!bucket) {
      bucket = [];
      events.set(objectId, bucket);
    }
    return bucket;
  }

  function notify(objectId: string): void {
    const subs = listeners.get(objectId);
    if (!subs || subs.size === 0) return;
    const snapshot = getEvents(objectId);
    for (const fn of subs) {
      fn(snapshot);
    }
  }

  function trim(bucket: ActivityEvent[]): void {
    if (bucket.length > maxPerObject) {
      bucket.splice(0, bucket.length - maxPerObject);
    }
  }

  // ── Public API ─────────────────────────────────────────────────────────────

  function record(input: ActivityEventInput): ActivityEvent {
    const completed: ActivityEvent = {
      ...input,
      id: generateId(),
      createdAt: new Date().toISOString(),
    };

    const bucket = getOrCreate(completed.objectId);
    bucket.push(completed);
    trim(bucket);
    notify(completed.objectId);

    return completed;
  }

  function getEvents(
    objectId: string,
    opts?: { limit?: number; before?: string },
  ): ActivityEvent[] {
    const bucket = events.get(objectId) ?? [];
    let results = bucket.slice().reverse();

    if (opts?.before) {
      const before = opts.before;
      results = results.filter((e) => e.createdAt < before);
    }

    if (opts?.limit !== undefined && opts.limit >= 0) {
      results = results.slice(0, opts.limit);
    }

    return results;
  }

  function getLatest(objectId: string): ActivityEvent | null {
    const bucket = events.get(objectId);
    if (!bucket || bucket.length === 0) return null;
    return bucket[bucket.length - 1] ?? null;
  }

  function getEventCount(objectId: string): number {
    return events.get(objectId)?.length ?? 0;
  }

  function hydrate(objectId: string, incoming: ActivityEvent[]): void {
    const sorted = incoming
      .slice()
      .sort((a, b) => (a.createdAt < b.createdAt ? -1 : 1));
    events.set(objectId, sorted);
    trim(sorted);
    notify(objectId);
  }

  function subscribe(
    objectId: string,
    listener: ActivityListener,
  ): () => void {
    let set = listeners.get(objectId);
    if (!set) {
      set = new Set();
      listeners.set(objectId, set);
    }
    set.add(listener);

    return () => {
      listeners.get(objectId)?.delete(listener);
    };
  }

  function toJSON(): Record<string, ActivityEvent[]> {
    const out: Record<string, ActivityEvent[]> = {};
    for (const [id, bucket] of events) {
      out[id] = bucket.slice();
    }
    return out;
  }

  function clear(): void {
    events.clear();
    listeners.clear();
  }

  return {
    record,
    getEvents,
    getLatest,
    getEventCount,
    hydrate,
    subscribe,
    toJSON,
    clear,
  };
}
