/**
 * Kernel-backed AdminDataSource.
 *
 * Projects a StudioKernel-shaped object into the normalised admin shape.
 * We don't import StudioKernel here (that would create a studio→admin-kit
 * cycle); instead we accept a structural subset — anything that exposes
 * `store`, `notifications`, `vfs adapter`, `relay`, `bus` etc.
 */

import type { PrismBus } from "@prism/core/atom";
import { PrismEvents } from "@prism/core/atom";
import type { CollectionStore } from "@prism/core/persistence";
import type { NotificationStore } from "@prism/core/notification";
import type { VfsAdapter } from "@prism/core/vfs";
import type { RelayManager } from "@prism/core/relay-manager";
import type { PresenceManager } from "@prism/core/presence";

import type {
  AdminDataSource,
  AdminSnapshot,
  ActivityItem,
  HealthLevel,
  HealthStatus,
  Metric,
  Service,
} from "../types.js";

/** Structural subset of a Prism kernel that the data source reads. */
export interface KernelAdminTarget {
  store: CollectionStore;
  notifications: NotificationStore;
  relay: RelayManager;
  presence: PresenceManager;
  bus: PrismBus;
  /**
   * VFS file list reader. The kernel exposes `listFiles(): BinaryRef[]`; we
   * only need the length + summed sizes so accept any shape that supports it.
   */
  listFiles?: () => Array<{ size: number }>;
  /** Optional raw adapter for `count()` / `totalSize()` when available. */
  vfs?: { adapter: VfsAdapter };
}

export interface KernelDataSourceOptions {
  id?: string;
  label?: string;
  /** Max activity items to retain in the ring buffer. Default 50. */
  maxActivity?: number;
  /** Clock override for tests. */
  now?: () => number;
}

const DEFAULT_MAX_ACTIVITY = 50;

interface BusEventBinding {
  event: string;
  kind: string;
  messageFor: (payload: unknown) => string;
}

const BUS_BINDINGS: BusEventBinding[] = [
  {
    event: PrismEvents.ObjectCreated,
    kind: "object.created",
    messageFor: (payload) => {
      const obj = (payload as { object?: { type?: string; name?: string } }).object;
      return obj ? `Created ${obj.type ?? "object"}: ${obj.name ?? ""}`.trim() : "Object created";
    },
  },
  {
    event: PrismEvents.ObjectUpdated,
    kind: "object.updated",
    messageFor: (payload) => {
      const obj = (payload as { object?: { type?: string; name?: string } }).object;
      return obj ? `Updated ${obj.type ?? "object"}: ${obj.name ?? ""}`.trim() : "Object updated";
    },
  },
  {
    event: PrismEvents.ObjectDeleted,
    kind: "object.deleted",
    messageFor: () => "Object deleted",
  },
  {
    event: PrismEvents.EdgeCreated,
    kind: "edge.created",
    messageFor: () => "Edge created",
  },
  {
    event: PrismEvents.EdgeDeleted,
    kind: "edge.deleted",
    messageFor: () => "Edge deleted",
  },
];

export function createKernelDataSource(
  kernel: KernelAdminTarget,
  options: KernelDataSourceOptions = {},
): AdminDataSource {
  const id = options.id ?? "kernel";
  const label = options.label ?? "Prism Kernel";
  const maxActivity = options.maxActivity ?? DEFAULT_MAX_ACTIVITY;
  const now = options.now ?? Date.now;
  const startedAt = now();

  const activity: ActivityItem[] = [];
  let activityCounter = 0;

  function pushActivity(kind: string, message: string, level: HealthLevel = "ok"): void {
    activity.unshift({
      id: `k-${++activityCounter}`,
      timestamp: new Date(now()).toISOString(),
      kind,
      message,
      level,
    });
    if (activity.length > maxActivity) activity.length = maxActivity;
  }

  // Seed from notifications if available (newest first).
  for (const n of kernel.notifications.getAll()) {
    pushActivity(`notification.${n.kind}`, n.title, mapNotificationLevel(n.kind));
    if (activity.length >= maxActivity) break;
  }

  // Wire bus listeners lazily in subscribe() so the ring buffer only spins
  // when something is actually watching.
  const disposers: Array<() => void> = [];

  function attachBus(): void {
    if (disposers.length > 0) return;
    for (const binding of BUS_BINDINGS) {
      disposers.push(
        kernel.bus.on(binding.event, (payload: unknown) => {
          pushActivity(binding.kind, binding.messageFor(payload));
          notifyListeners();
        }),
      );
    }
    disposers.push(
      kernel.notifications.subscribe((change) => {
        if (change.type === "add" && change.notification) {
          pushActivity(
            `notification.${change.notification.kind}`,
            change.notification.title,
            mapNotificationLevel(change.notification.kind),
          );
          notifyListeners();
        }
      }),
    );
    disposers.push(kernel.relay.subscribe(() => notifyListeners()));
  }

  function detachBus(): void {
    for (const off of disposers) off();
    disposers.length = 0;
  }

  const listeners = new Set<(snap: AdminSnapshot) => void>();

  function notifyListeners(): void {
    if (listeners.size === 0) return;
    const snap = buildSnapshot();
    for (const l of listeners) l(snap);
  }

  function buildSnapshot(): AdminSnapshot {
    const objects = kernel.store.allObjects();
    const edges = kernel.store.allEdges();
    const files = kernel.listFiles?.() ?? [];
    const totalBytes = files.reduce((sum, f) => sum + (f.size ?? 0), 0);
    const unread = kernel.notifications.getUnreadCount();
    const relays = kernel.relay.listRelays();
    const connectedRelays = relays.filter((r) => r.status === "connected").length;
    const peers = kernel.presence.peerCount;

    const services: Service[] = [
      {
        id: "object-store",
        name: "Object Store",
        kind: "crdt",
        health: "ok",
        status: `${objects.length} objects · ${edges.length} edges`,
      },
      {
        id: "vfs",
        name: "Virtual File System",
        kind: "storage",
        health: "ok",
        status: `${files.length} files · ${formatBytesShort(totalBytes)}`,
      },
      {
        id: "relay",
        name: "Relay Network",
        kind: "network",
        health: relays.length === 0 ? "unknown" : connectedRelays > 0 ? "ok" : "warn",
        status:
          relays.length === 0
            ? "no relays configured"
            : `${connectedRelays}/${relays.length} connected`,
      },
      {
        id: "presence",
        name: "Presence",
        kind: "network",
        health: "ok",
        status: `${peers} peer${peers === 1 ? "" : "s"} online`,
      },
      {
        id: "notifications",
        name: "Notifications",
        kind: "ui",
        health: unread === 0 ? "ok" : "warn",
        status: `${unread} unread · ${kernel.notifications.size()} total`,
      },
    ];

    const metrics: Metric[] = [
      { id: "objects", label: "Objects", value: objects.length },
      { id: "edges", label: "Edges", value: edges.length },
      { id: "files", label: "Files", value: files.length },
      { id: "bytes", label: "Storage", value: totalBytes, unit: " B" },
      { id: "relays", label: "Relays", value: `${connectedRelays}/${relays.length}` },
      { id: "peers", label: "Peers Online", value: peers },
      { id: "unread", label: "Unread Notifications", value: unread },
    ];

    const health: HealthStatus = computeHealth(services);
    const uptimeSeconds = Math.max(0, Math.floor((now() - startedAt) / 1000));

    return {
      sourceId: id,
      sourceLabel: label,
      capturedAt: new Date(now()).toISOString(),
      uptimeSeconds,
      health,
      metrics,
      services,
      activity: [...activity],
    };
  }

  return {
    id,
    label,
    async snapshot() {
      return buildSnapshot();
    },
    subscribe(listener) {
      if (listeners.size === 0) attachBus();
      listeners.add(listener);
      // Fire once with current state so the subscriber has an initial value.
      listener(buildSnapshot());
      return () => {
        listeners.delete(listener);
        if (listeners.size === 0) detachBus();
      };
    },
    dispose() {
      detachBus();
      listeners.clear();
    },
  };
}

// ── Helpers ───────────────────────────────────────────────────────────────

function mapNotificationLevel(kind: string): HealthLevel {
  if (kind === "error") return "error";
  if (kind === "warning") return "warn";
  return "ok";
}

function computeHealth(services: Service[]): HealthStatus {
  const hasError = services.some((s) => s.health === "error");
  if (hasError) return { level: "error", label: "Degraded" };
  const hasWarn = services.some((s) => s.health === "warn");
  if (hasWarn) return { level: "warn", label: "Attention needed" };
  return { level: "ok", label: "Healthy" };
}

function formatBytesShort(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
