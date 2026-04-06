/**
 * KernelProvider — React context exposing the Studio kernel to all components.
 *
 * Components use useKernel() to access the full kernel, or the focused hooks
 * (useSelection, useObjects, useNotifications) for specific slices.
 */

import { createContext, useContext, useSyncExternalStore, useCallback, useRef } from "react";
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

/**
 * Reactive object list from ObjectAtomStore.
 * Uses a version counter to avoid returning new arrays from getSnapshot.
 */
export function useObjects(filter?: {
  types?: string[];
  parentId?: ObjectId | null;
}): GraphObject[] {
  const kernel = useKernel();
  const cacheRef = useRef<{ value: GraphObject[]; version: number }>({ value: [], version: -1 });

  const version = useSyncExternalStore(
    (cb) => kernel.objectAtoms.subscribe(cb),
    () => {
      // Return a simple counter that increments on change
      const keys = Object.keys(kernel.objectAtoms.getState().objects);
      return keys.length; // Stable primitive
    },
  );

  // Compute the filtered list only when version changes
  if (cacheRef.current.version !== version) {
    const all = Object.values(kernel.objectAtoms.getState().objects);
    cacheRef.current = {
      value: all.filter((obj) => {
        if (obj.deletedAt) return false;
        if (filter?.types && !filter.types.includes(obj.type)) return false;
        if (filter?.parentId !== undefined && obj.parentId !== filter.parentId)
          return false;
        return true;
      }),
      version,
    };
  }

  return cacheRef.current.value;
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
  const cacheRef = useRef<{
    canUndo: boolean;
    canRedo: boolean;
    undoLabel: string | null;
    redoLabel: string | null;
  }>({ canUndo: false, canRedo: false, undoLabel: null, redoLabel: null });

  // Use a string key as the snapshot — stable primitive
  const stateKey = useSyncExternalStore(
    (cb) => kernel.undo.subscribe(cb),
    () => `${kernel.undo.canUndo}|${kernel.undo.canRedo}|${kernel.undo.undoLabel}|${kernel.undo.redoLabel}`,
  );

  // Update cache when key changes
  const parts = stateKey.split("|");
  cacheRef.current = {
    canUndo: parts[0] === "true",
    canRedo: parts[1] === "true",
    undoLabel: parts[2] === "null" ? null : (parts[2] ?? null),
    redoLabel: parts[3] === "null" ? null : (parts[3] ?? null),
  };

  return {
    ...cacheRef.current,
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
  const cacheRef = useRef<{ items: Notification[]; unreadCount: number; version: number }>({
    items: [],
    unreadCount: 0,
    version: -1,
  });

  const version = useSyncExternalStore(
    (cb) => kernel.notifications.subscribe(cb),
    () => kernel.notifications.getUnreadCount(),
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      items: kernel.notifications.getAll(kind ? { kind: [kind] } : undefined),
      unreadCount: kernel.notifications.getUnreadCount(),
      version,
    };
  }

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

  return { ...cacheRef.current, add, dismiss };
}
