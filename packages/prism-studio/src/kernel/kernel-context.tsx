/**
 * KernelProvider — React context exposing the Studio kernel to all components.
 *
 * Components use useKernel() to access the full kernel, or the focused hooks
 * (useSelection, useObjects, useNotifications) for specific slices.
 */

import { createContext, useContext, useSyncExternalStore, useCallback } from "react";
import type { ReactNode } from "react";
import type { StudioKernel } from "./studio-kernel.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { Notification, NotificationKind } from "@prism/core/notification";

// ── Context ─────────────────────────────────────────────────────────────────

const KernelContext = createContext<StudioKernel | null>(null);

export function KernelProvider({
  kernel,
  children,
}: {
  kernel: StudioKernel;
  children: ReactNode;
}) {
  return (
    <KernelContext.Provider value={kernel}>{children}</KernelContext.Provider>
  );
}

/** Access the full Studio kernel. Throws if used outside KernelProvider. */
export function useKernel(): StudioKernel {
  const ctx = useContext(KernelContext);
  if (!ctx) throw new Error("useKernel must be used within KernelProvider");
  return ctx;
}

// ── Focused hooks ───────────────────────────────────────────────────────────

/** Reactive selected object ID from AtomStore. */
export function useSelection(): {
  selectedId: ObjectId | null;
  select: (id: ObjectId | null) => void;
} {
  const kernel = useKernel();
  const selectedId = useSyncExternalStore(
    (cb) => kernel.atoms.subscribe(cb),
    () => kernel.atoms.getState().selectedId,
  );
  return { selectedId, select: kernel.select };
}

/** Reactive object list from ObjectAtomStore. */
export function useObjects(filter?: {
  types?: string[];
  parentId?: ObjectId | null;
}): GraphObject[] {
  const kernel = useKernel();
  return useSyncExternalStore(
    (cb) => kernel.objectAtoms.subscribe(cb),
    () => {
      const all = Object.values(kernel.objectAtoms.getState().objects);
      return all.filter((obj) => {
        if (obj.deletedAt) return false;
        if (filter?.types && !filter.types.includes(obj.type)) return false;
        if (filter?.parentId !== undefined && obj.parentId !== filter.parentId)
          return false;
        return true;
      });
    },
  );
}

/** Reactive single object by ID. */
export function useObject(id: ObjectId | null): GraphObject | undefined {
  const kernel = useKernel();
  return useSyncExternalStore(
    (cb) => kernel.objectAtoms.subscribe(cb),
    () => (id ? kernel.objectAtoms.getState().objects[id] : undefined),
  );
}

/** Reactive undo/redo state. */
export function useUndo(): {
  canUndo: boolean;
  canRedo: boolean;
  undoLabel: string | null;
  redoLabel: string | null;
  undo: () => void;
  redo: () => void;
} {
  const kernel = useKernel();
  const state = useSyncExternalStore(
    (cb) => kernel.undo.subscribe(cb),
    () => ({
      canUndo: kernel.undo.canUndo,
      canRedo: kernel.undo.canRedo,
      undoLabel: kernel.undo.undoLabel,
      redoLabel: kernel.undo.redoLabel,
    }),
  );
  return {
    ...state,
    undo: () => kernel.undo.undo(),
    redo: () => kernel.undo.redo(),
  };
}

/** Reactive notification list. */
export function useNotifications(kind?: NotificationKind): {
  items: Notification[];
  unreadCount: number;
  add: (title: string, kind: NotificationKind, body?: string) => void;
  dismiss: (id: string) => void;
} {
  const kernel = useKernel();
  const state = useSyncExternalStore(
    (cb) => kernel.notifications.subscribe(cb),
    () => ({
      items: kernel.notifications.getAll(kind ? { kind: [kind] } : undefined),
      unreadCount: kernel.notifications.getUnreadCount(),
    }),
  );

  const add = useCallback(
    (title: string, k: NotificationKind, body?: string) => {
      kernel.notifications.add({ title, kind: k, body });
    },
    [kernel],
  );

  const dismiss = useCallback(
    (id: string) => {
      kernel.notifications.dismiss(id);
    },
    [kernel],
  );

  return { ...state, add, dismiss };
}
