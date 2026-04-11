/**
 * NotificationStore — in-memory notification registry with eviction policy.
 *
 * Ported from legacy @core/logic/notifications. Adapted for Prism:
 *   - Synchronous (no async, no localStorage — Tauri IPC or Loro for persistence)
 *   - Framework-agnostic (pure TypeScript, no React)
 *   - Eviction policy: dismissed unpinned → read unpinned → oldest first
 *
 * Responsibilities:
 *   - Add/dismiss/markRead/pin notifications
 *   - Filter by kind, read, pinned, objectId, since
 *   - Subscribe to changes
 *   - Automatic eviction when max capacity reached
 *   - Hydrate from external source
 */

// ── Types ─────────────────────────────────────────────────────────────────────

export type NotificationKind =
  | "system"
  | "mention"
  | "activity"
  | "reminder"
  | "info"
  | "success"
  | "warning"
  | "error";

export interface Notification {
  id: string;
  kind: NotificationKind;
  title: string;
  body?: string | undefined;
  /** Link to a specific object. */
  objectId?: string | undefined;
  objectType?: string | undefined;
  /** Who triggered this notification. */
  actorId?: string | undefined;
  read: boolean;
  pinned: boolean;
  createdAt: string;
  readAt?: string | undefined;
  dismissedAt?: string | undefined;
  /** Auto-expire after this timestamp. */
  expiresAt?: string | undefined;
  /** Arbitrary payload. */
  data?: Record<string, unknown> | undefined;
}

export interface NotificationFilter {
  kind?: NotificationKind[] | undefined;
  read?: boolean | undefined;
  pinned?: boolean | undefined;
  objectId?: string | undefined;
  /** Only notifications created after this ISO timestamp (exclusive). */
  since?: string | undefined;
}

export type NotificationInput = Omit<
  Notification,
  "id" | "read" | "pinned" | "createdAt" | "readAt" | "dismissedAt"
> & {
  id?: string | undefined;
  read?: boolean | undefined;
  pinned?: boolean | undefined;
  createdAt?: string | undefined;
};

export type NotificationChangeType = "add" | "update" | "dismiss" | "clear";

export interface NotificationChange {
  type: NotificationChangeType;
  notification?: Notification | undefined;
}

export type NotificationListener = (change: NotificationChange) => void;

export interface NotificationStoreOptions {
  /** Maximum notifications to retain. Default: 200. */
  maxItems?: number | undefined;
  /** ID generator. Default: crypto.randomUUID fallback to counter. */
  generateId?: (() => string) | undefined;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

let idCounter = 0;

function defaultGenerateId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `notif-${++idCounter}`;
}

function matchesFilter(n: Notification, filter: NotificationFilter): boolean {
  if (filter.kind && filter.kind.length > 0 && !filter.kind.includes(n.kind)) {
    return false;
  }
  if (filter.read !== undefined && n.read !== filter.read) return false;
  if (filter.pinned !== undefined && n.pinned !== filter.pinned) return false;
  if (filter.objectId !== undefined && n.objectId !== filter.objectId) return false;
  if (filter.since !== undefined && n.createdAt <= filter.since) return false;
  return true;
}

// ── NotificationStore ────────────────────────────────────────────────────────

export interface NotificationStore {
  /** Add a notification. Returns the created notification. */
  add(input: NotificationInput): Notification;

  /** Mark a notification as read. */
  markRead(id: string): Notification | undefined;

  /** Mark all matching notifications as read. */
  markAllRead(filter?: NotificationFilter): number;

  /** Dismiss (soft-delete) a notification. */
  dismiss(id: string): Notification | undefined;

  /** Dismiss all matching notifications. */
  dismissAll(filter?: NotificationFilter): number;

  /** Pin a notification (immune to eviction). */
  pin(id: string): Notification | undefined;

  /** Unpin a notification. */
  unpin(id: string): Notification | undefined;

  /** Get a notification by ID. */
  get(id: string): Notification | undefined;

  /**
   * Get all non-dismissed notifications, newest first.
   * Expired notifications are excluded.
   */
  getAll(filter?: NotificationFilter): Notification[];

  /** Count of unread non-dismissed notifications. */
  getUnreadCount(filter?: NotificationFilter): number;

  /** Total non-dismissed notifications. */
  size(): number;

  /** Subscribe to changes. Returns unsubscribe function. */
  subscribe(listener: NotificationListener): () => void;

  /** Bulk load from external source. Sorts newest-first and evicts. */
  hydrate(items: Notification[]): void;

  /** Remove all dismissed unpinned notifications from memory. */
  clear(): number;
}

export function createNotificationStore(
  options?: NotificationStoreOptions,
): NotificationStore {
  const maxItems = options?.maxItems ?? 200;
  const generateId = options?.generateId ?? defaultGenerateId;

  /** id → Notification */
  const store = new Map<string, Notification>();
  const listeners = new Set<NotificationListener>();

  function emit(change: NotificationChange): void {
    for (const listener of listeners) {
      listener(change);
    }
  }

  function isExpired(n: Notification): boolean {
    if (!n.expiresAt) return false;
    return new Date(n.expiresAt).getTime() <= Date.now();
  }

  /**
   * Evict oldest notifications when over capacity.
   * Priority: dismissed unpinned (oldest) → read unpinned (oldest).
   * Unread and pinned items are never evicted.
   */
  function evict(): void {
    if (store.size <= maxItems) return;

    const all = [...store.values()];

    // Build eviction candidates in priority order
    const dismissedUnpinned = all
      .filter((n) => n.dismissedAt && !n.pinned)
      .sort((a, b) => a.createdAt.localeCompare(b.createdAt));

    const readUnpinned = all
      .filter((n) => !n.dismissedAt && n.read && !n.pinned)
      .sort((a, b) => a.createdAt.localeCompare(b.createdAt));

    const candidates = [...dismissedUnpinned, ...readUnpinned];

    let toRemove = store.size - maxItems;
    for (const candidate of candidates) {
      if (toRemove <= 0) break;
      store.delete(candidate.id);
      toRemove--;
    }
  }

  function add(input: NotificationInput): Notification {
    const now = new Date().toISOString();
    const notification: Notification = {
      id: input.id ?? generateId(),
      kind: input.kind,
      title: input.title,
      read: input.read ?? false,
      pinned: input.pinned ?? false,
      createdAt: input.createdAt ?? now,
    };
    if (input.body !== undefined) notification.body = input.body;
    if (input.objectId !== undefined) notification.objectId = input.objectId;
    if (input.objectType !== undefined) notification.objectType = input.objectType;
    if (input.actorId !== undefined) notification.actorId = input.actorId;
    if (input.expiresAt !== undefined) notification.expiresAt = input.expiresAt;
    if (input.data !== undefined) notification.data = input.data;

    store.set(notification.id, notification);
    evict();
    emit({ type: "add", notification });
    return notification;
  }

  function markRead(id: string): Notification | undefined {
    const n = store.get(id);
    if (!n || n.read) return n;
    const updated = { ...n, read: true, readAt: new Date().toISOString() };
    store.set(id, updated);
    emit({ type: "update", notification: updated });
    return updated;
  }

  function markAllRead(filter?: NotificationFilter): number {
    let count = 0;
    for (const n of store.values()) {
      if (n.read || n.dismissedAt) continue;
      if (filter && !matchesFilter(n, filter)) continue;
      const updated = { ...n, read: true, readAt: new Date().toISOString() };
      store.set(n.id, updated);
      count++;
    }
    if (count > 0) emit({ type: "update" });
    return count;
  }

  function dismiss(id: string): Notification | undefined {
    const n = store.get(id);
    if (!n || n.dismissedAt) return n;
    const updated = { ...n, dismissedAt: new Date().toISOString() };
    store.set(id, updated);
    emit({ type: "dismiss", notification: updated });
    return updated;
  }

  function dismissAll(filter?: NotificationFilter): number {
    let count = 0;
    for (const n of store.values()) {
      if (n.dismissedAt) continue;
      if (filter && !matchesFilter(n, filter)) continue;
      const updated = { ...n, dismissedAt: new Date().toISOString() };
      store.set(n.id, updated);
      count++;
    }
    if (count > 0) emit({ type: "dismiss" });
    return count;
  }

  function setPin(id: string, pinned: boolean): Notification | undefined {
    const n = store.get(id);
    if (!n) return undefined;
    const updated = { ...n, pinned };
    store.set(id, updated);
    emit({ type: "update", notification: updated });
    return updated;
  }

  function getAll(filter?: NotificationFilter): Notification[] {
    const now = Date.now();
    const result: Notification[] = [];
    for (const n of store.values()) {
      if (n.dismissedAt) continue;
      if (n.expiresAt && new Date(n.expiresAt).getTime() <= now) continue;
      if (filter && !matchesFilter(n, filter)) continue;
      result.push(n);
    }
    // Newest first
    result.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    return result;
  }

  function getUnreadCount(filter?: NotificationFilter): number {
    let count = 0;
    const now = Date.now();
    for (const n of store.values()) {
      if (n.dismissedAt || n.read) continue;
      if (n.expiresAt && new Date(n.expiresAt).getTime() <= now) continue;
      if (filter && !matchesFilter(n, filter)) continue;
      count++;
    }
    return count;
  }

  function hydrate(items: Notification[]): void {
    store.clear();
    // Sort newest first before inserting
    const sorted = [...items].sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    for (const item of sorted) {
      store.set(item.id, item);
    }
    evict();
    emit({ type: "clear" });
  }

  function clear(): number {
    let count = 0;
    for (const [id, n] of store) {
      if (n.dismissedAt && !n.pinned) {
        store.delete(id);
        count++;
      }
    }
    if (count > 0) emit({ type: "clear" });
    return count;
  }

  return {
    add,
    markRead,
    markAllRead,
    dismiss,
    dismissAll,
    pin: (id) => setPin(id, true),
    unpin: (id) => setPin(id, false),
    get: (id) => store.get(id),
    getAll,
    getUnreadCount,
    size: () => {
      let count = 0;
      for (const n of store.values()) {
        if (!n.dismissedAt && !isExpired(n)) count++;
      }
      return count;
    },
    subscribe(listener: NotificationListener): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    hydrate,
    clear,
  };
}
