/**
 * Behavior dispatcher — fires Luau behaviors attached to GraphObjects.
 *
 * A "behavior" is a first-class GraphObject of type `"behavior"` with
 * `{ targetObjectId, trigger, source, enabled }` in its data payload.
 * The dispatcher scans the store for behaviors matching a target + trigger
 * pair and runs each one through `executeLuau` with a small injected
 * global map (`ui`, `kernel`, `event`, `self`) so scripts can navigate,
 * mutate objects, or raise notifications.
 *
 * This same dispatcher is used in both edit-mode preview (Puck canvas,
 * the layout panel's "Try It" interactions) and in the published runtime
 * — one code path, one set of semantics.
 */

import { executeLuau } from "@prism/core/luau";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { CollectionStore } from "@prism/core/persistence";
import type { NotificationStore } from "@prism/core/notification";

/**
 * Minimal kernel surface the dispatcher needs. Keeps behavior-dispatcher
 * free of a circular `StudioKernel` import — the studio-kernel factory
 * wires real implementations as the kernel object is assembled.
 */
export interface BehaviorKernel {
  readonly store: CollectionStore;
  readonly notifications: NotificationStore;
  select(id: ObjectId | null): void;
  updateObject(id: ObjectId, patch: Partial<GraphObject>): GraphObject | undefined;
}

// ── Types ────────────────────────────────────────────────────────────────────

export type BehaviorTrigger =
  | "onClick"
  | "onMount"
  | "onChange"
  | "onRouteEnter"
  | "onRouteLeave";

export interface BehaviorRecord {
  id: ObjectId;
  targetObjectId: ObjectId;
  trigger: BehaviorTrigger;
  source: string;
  enabled: boolean;
}

/**
 * Result of a single behavior fire — one entry per matched behavior.
 * `success: false` carries the Luau runtime error message.
 */
export interface BehaviorFireResult {
  behaviorId: ObjectId;
  success: boolean;
  value?: unknown;
  error?: string;
}

export interface BehaviorDispatcher {
  /** Run every enabled behavior bound to (targetObjectId, trigger). */
  fire(
    targetObjectId: ObjectId,
    trigger: BehaviorTrigger,
    event?: Record<string, unknown>,
  ): Promise<BehaviorFireResult[]>;
  /** List behaviors bound to a target, optionally filtered by trigger. */
  list(targetObjectId: ObjectId, trigger?: BehaviorTrigger): BehaviorRecord[];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const BEHAVIOR_TYPE = "behavior";

function asBehavior(obj: GraphObject): BehaviorRecord | undefined {
  if (obj.type !== BEHAVIOR_TYPE || obj.deletedAt) return undefined;
  const data = obj.data as Record<string, unknown>;
  const targetObjectId = typeof data["targetObjectId"] === "string" ? (data["targetObjectId"] as ObjectId) : "";
  const trigger = (data["trigger"] as BehaviorTrigger | undefined) ?? "onClick";
  const source = typeof data["source"] === "string" ? (data["source"] as string) : "";
  const enabled = data["enabled"] !== false;
  if (!targetObjectId || !source) return undefined;
  return { id: obj.id, targetObjectId, trigger, source, enabled };
}

/**
 * Build the global map injected into every Luau behavior. Scripts call
 * `ui.navigate(...)`, `kernel.select(...)`, `kernel.notify(...)` without
 * having to import anything — these are just pre-bound functions.
 */
export function buildBehaviorGlobals(
  kernel: BehaviorKernel,
  target: ObjectId,
  event?: Record<string, unknown>,
): Record<string, unknown> {
  return {
    self: target,
    event: event ?? {},
    ui: {
      navigate: (path: string) => {
        kernel.notifications.add({
          title: `Navigate → ${path}`,
          kind: "info",
        });
      },
      notify: (title: string, body?: string) => {
        kernel.notifications.add({
          title,
          kind: "info",
          ...(body !== undefined ? { body } : {}),
        });
      },
      alert: (title: string) => {
        kernel.notifications.add({ title, kind: "warning" });
      },
    },
    kernel: {
      select: (id: string) => kernel.select(id as ObjectId),
      getObject: (id: string) => kernel.store.getObject(id as ObjectId),
      updateObject: (id: string, patch: Record<string, unknown>) =>
        kernel.updateObject(id as ObjectId, patch),
      notify: (title: string, body?: string) => {
        kernel.notifications.add({
          title,
          kind: "info",
          ...(body !== undefined ? { body } : {}),
        });
      },
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────────

export function createBehaviorDispatcher(kernel: BehaviorKernel): BehaviorDispatcher {
  function list(targetObjectId: ObjectId, trigger?: BehaviorTrigger): BehaviorRecord[] {
    const out: BehaviorRecord[] = [];
    for (const obj of kernel.store.allObjects()) {
      const b = asBehavior(obj);
      if (!b) continue;
      if (b.targetObjectId !== targetObjectId) continue;
      if (trigger && b.trigger !== trigger) continue;
      out.push(b);
    }
    return out;
  }

  async function fire(
    targetObjectId: ObjectId,
    trigger: BehaviorTrigger,
    event?: Record<string, unknown>,
  ): Promise<BehaviorFireResult[]> {
    const behaviors = list(targetObjectId, trigger).filter((b) => b.enabled);
    if (behaviors.length === 0) return [];
    const globals = buildBehaviorGlobals(kernel, targetObjectId, event);
    const results: BehaviorFireResult[] = [];
    for (const b of behaviors) {
      const res = await executeLuau(b.source, globals);
      if (res.success) {
        results.push({ behaviorId: b.id, success: true, value: res.value });
      } else {
        results.push({
          behaviorId: b.id,
          success: false,
          error: res.error ?? "Unknown Luau error",
        });
        kernel.notifications.add({
          title: `Behavior failed: ${b.id}`,
          kind: "error",
          body: res.error ?? "Unknown Luau error",
        });
      }
    }
    return results;
  }

  return { fire, list };
}
