/**
 * NotificationQueue — batching and deduplication wrapper for NotificationStore.
 *
 * Debounces rapid notification bursts and deduplicates by (objectId, kind)
 * within a configurable time window. Useful for preventing notification spam
 * from rapid-fire events (e.g., many edits to the same object).
 *
 * Ported from legacy @core/logic/notifications queue.
 */

import type { NotificationInput, NotificationStore, Notification } from "./notification-store.js";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface NotificationQueueOptions {
  /** Debounce window in ms. Queued items are flushed after this delay. Default: 300. */
  debounceMs?: number | undefined;
  /** Dedup window in ms. Notifications with same (objectId, kind) within this window are merged. Default: 5000. */
  dedupWindowMs?: number | undefined;
  /** Timer implementation for testing. Default: globalThis setTimeout/clearTimeout. */
  timers?: TimerProvider | undefined;
}

export interface TimerProvider {
  setTimeout(fn: () => void, ms: number): number;
  clearTimeout(id: number): void;
  now(): number;
}

// ── Default timers ───────────────────────────────────────────────────────────

function defaultTimers(): TimerProvider {
  return {
    setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms) as unknown as number,
    clearTimeout: (id) => globalThis.clearTimeout(id),
    now: () => Date.now(),
  };
}

// ── Dedup key ────────────────────────────────────────────────────────────────

function dedupKey(input: NotificationInput): string | null {
  if (!input.objectId) return null;
  return `${input.objectId}:${input.kind}`;
}

// ── NotificationQueue ────────────────────────────────────────────────────────

export interface NotificationQueue {
  /** Enqueue a notification. May be deduped or debounced. */
  enqueue(input: NotificationInput): void;

  /** Force immediate delivery of all queued notifications. */
  flush(): Notification[];

  /** Number of notifications currently in the queue. */
  pending(): number;

  /** Dispose the queue (clear timers). */
  dispose(): void;
}

export function createNotificationQueue(
  store: NotificationStore,
  options?: NotificationQueueOptions,
): NotificationQueue {
  const debounceMs = options?.debounceMs ?? 300;
  const dedupWindowMs = options?.dedupWindowMs ?? 5000;
  const timers = options?.timers ?? defaultTimers();

  /** Pending items keyed by dedup key (or unique fallback). */
  const queue = new Map<string, { input: NotificationInput; enqueuedAt: number }>();
  let fallbackCounter = 0;
  let debounceTimer: number | null = null;

  /** Track recent deliveries for dedup: dedupKey → timestamp. */
  const recentDeliveries = new Map<string, number>();

  function scheduleFlush(): void {
    if (debounceTimer !== null) {
      timers.clearTimeout(debounceTimer);
    }
    debounceTimer = timers.setTimeout(() => {
      debounceTimer = null;
      flush();
    }, debounceMs);
  }

  function cleanRecentDeliveries(): void {
    const cutoff = timers.now() - dedupWindowMs;
    for (const [key, ts] of recentDeliveries) {
      if (ts < cutoff) recentDeliveries.delete(key);
    }
  }

  function enqueue(input: NotificationInput): void {
    const key = dedupKey(input);

    if (key) {
      // Check if recently delivered — skip if within dedup window
      const lastDelivered = recentDeliveries.get(key);
      if (lastDelivered !== undefined && timers.now() - lastDelivered < dedupWindowMs) {
        // Merge: update the pending entry (or skip if already flushed)
        const existing = queue.get(key);
        if (existing) {
          // Replace with newer content but keep original enqueue time
          existing.input = input;
          return;
        }
        // Already delivered within window — skip entirely
        return;
      }

      // Dedup within the queue itself
      const existing = queue.get(key);
      if (existing) {
        existing.input = input;
        scheduleFlush();
        return;
      }

      queue.set(key, { input, enqueuedAt: timers.now() });
    } else {
      // No dedup key — use unique fallback
      queue.set(`__unique_${++fallbackCounter}`, { input, enqueuedAt: timers.now() });
    }

    scheduleFlush();
  }

  function flush(): Notification[] {
    if (debounceTimer !== null) {
      timers.clearTimeout(debounceTimer);
      debounceTimer = null;
    }

    const delivered: Notification[] = [];
    const now = timers.now();

    for (const [key, entry] of queue) {
      const notification = store.add(entry.input);
      delivered.push(notification);
      // Track delivery for dedup
      if (!key.startsWith("__unique_")) {
        recentDeliveries.set(key, now);
      }
    }

    queue.clear();
    cleanRecentDeliveries();
    return delivered;
  }

  function dispose(): void {
    if (debounceTimer !== null) {
      timers.clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    queue.clear();
    recentDeliveries.clear();
  }

  return {
    enqueue,
    flush,
    pending: () => queue.size,
    dispose,
  };
}
