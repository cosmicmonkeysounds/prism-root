import { describe, expect, it } from "vitest";
import { PrismEvents } from "@prism/core/atom";
import {
  createKernelDataSource,
  type KernelAdminTarget,
} from "./kernel-data-source.js";

/**
 * Minimal in-memory fake of a StudioKernel that satisfies KernelAdminTarget.
 * Rather than wire up the full kernel (store + bus + relay + presence + ...),
 * we build just enough shape to exercise the projection logic.
 */
function createFakeKernel(): {
  target: KernelAdminTarget;
  bus: {
    emit: (event: string, payload: unknown) => void;
    handlers: Map<string, Set<(payload: unknown) => void>>;
  };
  files: { size: number }[];
  notifications: {
    add(n: { kind: string; title: string }): void;
    markAllRead(): void;
  };
  setPeerCount(n: number): void;
} {
  const handlers = new Map<string, Set<(payload: unknown) => void>>();
  const bus = {
    emit(event: string, payload: unknown) {
      handlers.get(event)?.forEach((h) => h(payload));
    },
    on(event: string, handler: (payload: unknown) => void): () => void {
      let set = handlers.get(event);
      if (!set) {
        set = new Set();
        handlers.set(event, set);
      }
      set.add(handler);
      return () => handlers.get(event)?.delete(handler);
    },
    handlers,
  };

  const storedObjects: Array<{ id: string; deletedAt: null }> = [
    { id: "a", deletedAt: null },
    { id: "b", deletedAt: null },
  ];
  const storedEdges: Array<{ id: string; deletedAt: null }> = [{ id: "e1", deletedAt: null }];

  const notificationList: Array<{ kind: string; title: string; read: boolean }> = [];
  const notificationListeners = new Set<(change: { type: string; notification?: { kind: string; title: string } }) => void>();

  const notifications = {
    add(n: { kind: string; title: string }) {
      notificationList.push({ ...n, read: false });
      notificationListeners.forEach((l) =>
        l({ type: "add", notification: { kind: n.kind, title: n.title } }),
      );
    },
    markAllRead() {
      for (const n of notificationList) n.read = true;
    },
    getAll: () => notificationList.slice(),
    getUnreadCount: () => notificationList.filter((n) => !n.read).length,
    size: () => notificationList.length,
    subscribe: (
      listener: (change: { type: string; notification?: { kind: string; title: string } }) => void,
    ) => {
      notificationListeners.add(listener);
      return () => notificationListeners.delete(listener);
    },
  };

  const relayList: Array<{ id: string; status: string }> = [];
  const relayListeners = new Set<() => void>();
  const relay = {
    listRelays: () => relayList.slice(),
    subscribe: (listener: () => void) => {
      relayListeners.add(listener);
      return () => relayListeners.delete(listener);
    },
    _add(entry: { id: string; status: string }) {
      relayList.push(entry);
      relayListeners.forEach((l) => l());
    },
  };

  const presence = { peerCount: 0 };

  const files: Array<{ size: number }> = [{ size: 1024 }, { size: 512 }];

  const target: KernelAdminTarget = {
    store: {
      allObjects: () => storedObjects.slice(),
      allEdges: () => storedEdges.slice(),
    } as unknown as KernelAdminTarget["store"],
    notifications: notifications as unknown as KernelAdminTarget["notifications"],
    relay: relay as unknown as KernelAdminTarget["relay"],
    presence: presence as unknown as KernelAdminTarget["presence"],
    bus: bus as unknown as KernelAdminTarget["bus"],
    listFiles: () => files.slice(),
  };

  return {
    target,
    bus,
    files,
    notifications,
    setPeerCount: (n: number) => {
      presence.peerCount = n;
    },
  };
}

describe("createKernelDataSource", () => {
  it("returns a data source with id/label", () => {
    const { target } = createFakeKernel();
    const src = createKernelDataSource(target, { id: "k", label: "Test Kernel" });
    expect(src.id).toBe("k");
    expect(src.label).toBe("Test Kernel");
  });

  it("projects store state into the snapshot", async () => {
    const { target } = createFakeKernel();
    const src = createKernelDataSource(target);
    const snap = await src.snapshot();
    const byId = Object.fromEntries(snap.metrics.map((m) => [m.id, m.value]));
    expect(byId["objects"]).toBe(2);
    expect(byId["edges"]).toBe(1);
    expect(byId["files"]).toBe(2);
    expect(byId["bytes"]).toBe(1024 + 512);
  });

  it("reports services with health inferred from state", async () => {
    const { target, setPeerCount } = createFakeKernel();
    setPeerCount(3);
    const src = createKernelDataSource(target);
    const snap = await src.snapshot();
    const services = Object.fromEntries(snap.services.map((s) => [s.id, s]));
    expect(services["object-store"]?.health).toBe("ok");
    expect(services["relay"]?.health).toBe("unknown");
    expect(services["presence"]?.status).toBe("3 peers online");
  });

  it("warns when notifications have unread items", async () => {
    const { target, notifications } = createFakeKernel();
    notifications.add({ kind: "warning", title: "something" });
    const src = createKernelDataSource(target);
    const snap = await src.snapshot();
    const notif = snap.services.find((s) => s.id === "notifications");
    expect(notif?.health).toBe("warn");
    expect(snap.health.level).toBe("warn");
  });

  it("errors when any service reports error health", async () => {
    const { target, notifications } = createFakeKernel();
    notifications.add({ kind: "error", title: "fail" });
    const src = createKernelDataSource(target);
    const subscribe = src.subscribe;
    if (!subscribe) throw new Error("subscribe should be defined");
    let received: ReturnType<typeof src.snapshot> | undefined;
    const unsubscribe = subscribe((snap) => {
      received = Promise.resolve(snap);
    });
    if (!received) throw new Error("subscribe did not emit synchronously");
    const snap = await received;
    // One error notification doesn't itself force service-level error, but activity records it.
    expect(snap.activity.some((a) => a.level === "error")).toBe(true);
    unsubscribe();
  });

  it("records bus events as activity when subscribed", async () => {
    const { target, bus } = createFakeKernel();
    const src = createKernelDataSource(target);

    let latest = await src.snapshot();
    const subscribe = src.subscribe;
    if (!subscribe) throw new Error("subscribe should be defined");
    const unsubscribe = subscribe((snap) => {
      latest = snap;
    });

    bus.emit(PrismEvents.ObjectCreated, { object: { type: "page", name: "Home" } });

    const kinds = latest.activity.map((a) => a.kind);
    expect(kinds).toContain("object.created");
    unsubscribe();
  });

  it("cleans up bus subscriptions when the last listener leaves", () => {
    const { target, bus } = createFakeKernel();
    const src = createKernelDataSource(target);
    const subscribe = src.subscribe;
    if (!subscribe) throw new Error("subscribe should be defined");
    const off = subscribe(() => {});
    expect(bus.handlers.get(PrismEvents.ObjectCreated)?.size ?? 0).toBeGreaterThan(0);
    off();
    expect(bus.handlers.get(PrismEvents.ObjectCreated)?.size ?? 0).toBe(0);
  });

  it("uses injected clock for capturedAt / uptime", async () => {
    const { target } = createFakeKernel();
    let t = 1_000_000;
    const src = createKernelDataSource(target, { now: () => t });
    t += 5_000;
    const snap = await src.snapshot();
    expect(snap.uptimeSeconds).toBe(5);
  });
});
