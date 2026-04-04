import type { WorkspaceManagerEvent, WorkspaceManagerListener } from "./layout-types.js";
import type { PageRegistry } from "./page-registry.js";
import type { PageModel } from "./page-model.js";
import { WorkspaceSlot } from "./workspace-slot.js";

export class WorkspaceManager<TTarget extends { kind: string }> {
  private slots = new Map<string, WorkspaceSlot<TTarget>>();
  private _activeId: string | null = null;
  private listeners = new Set<WorkspaceManagerListener<TTarget>>();

  open(
    id: string,
    registry: PageRegistry<TTarget>,
    initialTarget: TTarget,
    options: { label?: string; cacheSize?: number } = {},
  ): WorkspaceSlot<TTarget> {
    if (this.slots.has(id)) return this.slots.get(id)!;
    const slot = new WorkspaceSlot<TTarget>({ id, registry, initialTarget, ...options });
    this.slots.set(id, slot);
    this.emit({ kind: "slot-opened", slotId: id });
    this.focus(id);
    return slot;
  }

  close(id: string): void {
    const slot = this.slots.get(id);
    if (!slot) return;
    slot.dispose();
    this.slots.delete(id);
    if (this._activeId === id) {
      this._activeId = this.slots.size > 0 ? [...this.slots.keys()].at(-1)! : null;
    }
    this.emit({ kind: "slot-closed", slotId: id });
  }

  focus(id: string): void {
    const slot = this.slots.get(id);
    if (!slot || this._activeId === id) return;
    this._activeId = id;
    this.emit({ kind: "slot-focused", slotId: id });
  }

  get activeSlot(): WorkspaceSlot<TTarget> | null {
    return this._activeId ? (this.slots.get(this._activeId) ?? null) : null;
  }

  get activePage(): PageModel<TTarget> | null {
    return this.activeSlot?.activePage ?? null;
  }

  getSlot(id: string): WorkspaceSlot<TTarget> | undefined {
    return this.slots.get(id);
  }

  get allSlots(): WorkspaceSlot<TTarget>[] {
    return [...this.slots.values()];
  }

  get slotCount(): number {
    return this.slots.size;
  }

  on(listener: WorkspaceManagerListener<TTarget>): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  dispose(): void {
    for (const slot of this.slots.values()) slot.dispose();
    this.slots.clear();
    this._activeId = null;
    this.listeners.clear();
  }

  private emit(event: WorkspaceManagerEvent<TTarget>): void {
    for (const l of this.listeners) l(event);
  }
}
