/**
 * PrismBus — lightweight typed event bus.
 *
 * Bridges the push world (actions, mutations) into the pull world (atoms, UI).
 * Handlers subscribe to event types and receive typed payloads.
 *
 * Design notes:
 *   - Synchronous dispatch (no async queue). Keep handlers fast.
 *   - No wildcard subscriptions. Subscribe to specific event types.
 *   - Each subscribe() returns a cleanup function.
 *   - createPrismBus() factory for testability — no singletons.
 */

export type EventHandler<T = unknown> = (payload: T) => void;

export interface PrismBus {
  emit<T>(eventType: string, payload: T): void;
  on<T>(eventType: string, handler: EventHandler<T>): () => void;
  once<T>(eventType: string, handler: EventHandler<T>): () => void;
  off(eventType?: string): void;
  listenerCount(eventType?: string): number;
}

export function createPrismBus(): PrismBus {
  const handlers = new Map<string, Set<EventHandler>>();

  function getOrCreate(eventType: string): Set<EventHandler> {
    let set = handlers.get(eventType);
    if (!set) {
      set = new Set();
      handlers.set(eventType, set);
    }
    return set;
  }

  function on<T>(eventType: string, handler: EventHandler<T>): () => void {
    const set = getOrCreate(eventType);
    set.add(handler as EventHandler);
    return () => {
      set.delete(handler as EventHandler);
      if (set.size === 0) handlers.delete(eventType);
    };
  }

  const bus: PrismBus = {
    emit<T>(eventType: string, payload: T): void {
      const set = handlers.get(eventType);
      if (!set) return;
      for (const handler of [...set]) {
        (handler as EventHandler<T>)(payload);
      }
    },

    on,

    once<T>(eventType: string, handler: EventHandler<T>): () => void {
      const wrapper: EventHandler<T> = (payload) => {
        off();
        handler(payload);
      };
      const off = on(eventType, wrapper);
      return off;
    },

    off(eventType?: string): void {
      if (eventType) {
        handlers.delete(eventType);
      } else {
        handlers.clear();
      }
    },

    listenerCount(eventType?: string): number {
      if (eventType) {
        return handlers.get(eventType)?.size ?? 0;
      }
      let total = 0;
      for (const set of handlers.values()) total += set.size;
      return total;
    },
  };

  return bus;
}

/** Well-known Prism event type constants. */
export const PrismEvents = {
  ObjectCreated: "objects:created",
  ObjectUpdated: "objects:updated",
  ObjectDeleted: "objects:deleted",
  ObjectMoved: "objects:moved",

  EdgeCreated: "edges:created",
  EdgeDeleted: "edges:deleted",

  NavigationNavigate: "navigation:navigate",
  NavigationPanelToggled: "navigation:panel-toggled",

  SelectionChanged: "ui:selection-changed",
  EditModeChanged: "ui:edit-mode-changed",

  SearchCommit: "search:commit",
  SearchClear: "search:clear",
} as const;
