/**
 * Focus depth system for KBar command routing.
 *
 * KBar interrogates focus depth to surface contextual commands:
 *   Global → App → Plugin → Cursor
 *
 * Each depth level contributes actions to the palette. Deeper levels
 * override or augment shallower ones.
 */

import type { Action } from "kbar";

/** The four focus depth levels in Prism. */
export type FocusDepth = "global" | "app" | "plugin" | "cursor";

/** A priority-ordered list from shallowest to deepest. */
const DEPTH_ORDER: FocusDepth[] = ["global", "app", "plugin", "cursor"];

/** An action with an associated focus depth. */
export type PrismAction = Action & {
  depth: FocusDepth;
};

/** Registry of actions organized by focus depth. */
export type ActionRegistry = {
  /** Register actions at a specific depth. Returns unregister function. */
  register: (depth: FocusDepth, actions: Action[]) => () => void;
  /** Get all actions for the current depth and above. */
  getActions: (currentDepth: FocusDepth) => Action[];
  /** Subscribe to action changes. */
  subscribe: (callback: () => void) => () => void;
};

/**
 * Create an action registry that manages KBar actions by focus depth.
 *
 * Actions registered at deeper depths are only visible when that depth
 * is active. Global actions are always visible.
 */
export function createActionRegistry(): ActionRegistry {
  const actionsByDepth = new Map<FocusDepth, Map<string, Action>>();
  const listeners = new Set<() => void>();

  for (const depth of DEPTH_ORDER) {
    actionsByDepth.set(depth, new Map());
  }

  function notify() {
    for (const listener of listeners) {
      listener();
    }
  }

  return {
    register(depth: FocusDepth, actions: Action[]): () => void {
      const depthMap = actionsByDepth.get(depth);
      if (!depthMap) return () => {};

      for (const action of actions) {
        depthMap.set(action.id, action);
      }
      notify();

      return () => {
        for (const action of actions) {
          depthMap.delete(action.id);
        }
        notify();
      };
    },

    getActions(currentDepth: FocusDepth): Action[] {
      const depthIndex = DEPTH_ORDER.indexOf(currentDepth);
      const result: Action[] = [];

      // Include all actions from current depth and above (shallower)
      for (let i = 0; i <= depthIndex; i++) {
        const depth = DEPTH_ORDER[i];
        if (depth === undefined) continue;
        const depthMap = actionsByDepth.get(depth);
        if (depthMap) {
          result.push(...depthMap.values());
        }
      }

      return result;
    },

    subscribe(callback: () => void): () => void {
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },
  };
}
