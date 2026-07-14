// State + opener for the global context menu. The `<ContextMenuHost>`
// component reads from here and renders at most one menu at a time.
// Split out of the component file so react-refresh stays happy.

import { create } from 'zustand'

export type ContextMenuItem = {
  label: string
  onSelect(): void
  kind?: 'danger' | 'default'
  disabled?: boolean
  /** Right-aligned keybinding hint (e.g. `F12`). */
  hint?: string
  /** Optional e2e hook rendered as `data-testid` on the menu button. */
  testid?: string
}

/** A menu is items with optional divider rows between groups. */
export type ContextMenuEntry = ContextMenuItem | { separator: true }

export function isSeparator(e: ContextMenuEntry): e is { separator: true } {
  return 'separator' in e
}

type State = {
  items: ContextMenuEntry[] | null
  anchor: { x: number; y: number } | null
  open(items: ContextMenuEntry[], anchor: { x: number; y: number }): void
  close(): void
}

export const useContextMenu = create<State>((set) => ({
  items: null,
  anchor: null,
  open: (items, anchor) => set({ items, anchor }),
  close: () => set({ items: null, anchor: null }),
}))

/** Convenience: imperatively summon the menu from non-React code. */
export function openContextMenu(
  items: ContextMenuEntry[],
  anchor: { x: number; y: number },
): void {
  useContextMenu.getState().open(items, anchor)
}
