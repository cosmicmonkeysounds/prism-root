import { describe, it, expect, beforeEach } from "vitest";
import type { NotificationStore } from "./notification-store.js";
import { createNotificationStore } from "./notification-store.js";
import type { NotificationQueue, TimerProvider } from "./notification-queue.js";
import { createNotificationQueue } from "./notification-queue.js";

// ── Fake timers ──────────────────────────────────────────────────────────────

function createFakeTimers(): TimerProvider & {
  advance(ms: number): void;
  pendingCount(): number;
} {
  let currentTime = 0;
  let nextId = 1;
  const pending = new Map<number, { fn: () => void; fireAt: number }>();

  function advance(ms: number): void {
    const targetTime = currentTime + ms;
    // Fire timers in order
    while (true) {
      let earliest: { id: number; fn: () => void; fireAt: number } | null = null;
      for (const [id, entry] of pending) {
        if (entry.fireAt <= targetTime && (!earliest || entry.fireAt < earliest.fireAt)) {
          earliest = { id, ...entry };
        }
      }
      if (!earliest) break;
      currentTime = earliest.fireAt;
      pending.delete(earliest.id);
      earliest.fn();
    }
    currentTime = targetTime;
  }

  return {
    setTimeout(fn: () => void, ms: number): number {
      const id = nextId++;
      pending.set(id, { fn, fireAt: currentTime + ms });
      return id;
    },
    clearTimeout(id: number): void {
      pending.delete(id);
    },
    now(): number {
      return currentTime;
    },
    advance,
    pendingCount(): number {
      return pending.size;
    },
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("NotificationQueue", () => {
  let store: NotificationStore;
  let timers: ReturnType<typeof createFakeTimers>;
  let queue: NotificationQueue;
  let idCounter: number;

  beforeEach(() => {
    idCounter = 0;
    store = createNotificationStore({ generateId: () => `notif-${++idCounter}` });
    timers = createFakeTimers();
    queue = createNotificationQueue(store, {
      debounceMs: 300,
      dedupWindowMs: 5000,
      timers,
    });
  });

  // ── Basic enqueue/flush ────────────────────────────────────────────────────

  describe("enqueue and flush", () => {
    it("queues a notification", () => {
      queue.enqueue({ kind: "info", title: "Hello" });
      expect(queue.pending()).toBe(1);
      expect(store.size()).toBe(0); // not delivered yet
    });

    it("flushes manually", () => {
      queue.enqueue({ kind: "info", title: "Hello" });
      const delivered = queue.flush();
      expect(delivered).toHaveLength(1);
      expect(delivered[0]?.title).toBe("Hello");
      expect(store.size()).toBe(1);
      expect(queue.pending()).toBe(0);
    });

    it("auto-flushes after debounce", () => {
      queue.enqueue({ kind: "info", title: "Auto" });
      expect(store.size()).toBe(0);

      timers.advance(300);
      expect(store.size()).toBe(1);
    });

    it("resets debounce on subsequent enqueue", () => {
      queue.enqueue({ kind: "info", title: "First" });
      timers.advance(200);
      queue.enqueue({ kind: "warning", title: "Second" });
      timers.advance(200);
      // 400ms total since first enqueue, but only 200ms since last
      expect(store.size()).toBe(0);

      timers.advance(100);
      // Now 300ms since last enqueue
      expect(store.size()).toBe(2);
    });
  });

  // ── Deduplication ──────────────────────────────────────────────────────────

  describe("deduplication", () => {
    it("deduplicates by (objectId, kind) within queue", () => {
      queue.enqueue({ kind: "activity", title: "Edit 1", objectId: "obj-1" });
      queue.enqueue({ kind: "activity", title: "Edit 2", objectId: "obj-1" });
      queue.enqueue({ kind: "activity", title: "Edit 3", objectId: "obj-1" });

      expect(queue.pending()).toBe(1);

      const delivered = queue.flush();
      expect(delivered).toHaveLength(1);
      expect(delivered[0]?.title).toBe("Edit 3"); // last one wins
    });

    it("does not dedup different objectIds", () => {
      queue.enqueue({ kind: "activity", title: "A", objectId: "obj-1" });
      queue.enqueue({ kind: "activity", title: "B", objectId: "obj-2" });

      expect(queue.pending()).toBe(2);
    });

    it("does not dedup different kinds for same objectId", () => {
      queue.enqueue({ kind: "activity", title: "A", objectId: "obj-1" });
      queue.enqueue({ kind: "mention", title: "B", objectId: "obj-1" });

      expect(queue.pending()).toBe(2);
    });

    it("does not dedup items without objectId", () => {
      queue.enqueue({ kind: "info", title: "A" });
      queue.enqueue({ kind: "info", title: "B" });

      expect(queue.pending()).toBe(2);
    });

    it("deduplicates across flush within dedup window", () => {
      queue.enqueue({ kind: "activity", title: "First", objectId: "obj-1" });
      queue.flush();
      expect(store.size()).toBe(1);

      // Within 5000ms window — should be deduped
      timers.advance(1000);
      queue.enqueue({ kind: "activity", title: "Second", objectId: "obj-1" });
      expect(queue.pending()).toBe(0); // skipped entirely
    });

    it("allows after dedup window expires", () => {
      queue.enqueue({ kind: "activity", title: "First", objectId: "obj-1" });
      queue.flush();

      // Advance past dedup window
      timers.advance(6000);
      queue.enqueue({ kind: "activity", title: "Second", objectId: "obj-1" });
      expect(queue.pending()).toBe(1);
      queue.flush();
      expect(store.size()).toBe(2);
    });
  });

  // ── Pending ────────────────────────────────────────────────────────────────

  describe("pending", () => {
    it("tracks queued count", () => {
      expect(queue.pending()).toBe(0);
      queue.enqueue({ kind: "info", title: "A" });
      queue.enqueue({ kind: "info", title: "B" });
      expect(queue.pending()).toBe(2);
      queue.flush();
      expect(queue.pending()).toBe(0);
    });
  });

  // ── Dispose ────────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("clears pending and timers", () => {
      queue.enqueue({ kind: "info", title: "A" });
      queue.dispose();

      expect(queue.pending()).toBe(0);
      timers.advance(1000);
      expect(store.size()).toBe(0); // timer was cancelled
    });
  });
});
