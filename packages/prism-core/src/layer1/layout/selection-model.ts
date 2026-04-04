import type { SelectionEvent, SelectionListener } from "./layout-types.js";

export class SelectionModel {
  private _selected = new Set<string>();
  private _primary: string | null = null;
  private listeners = new Set<SelectionListener>();

  select(id: string): void {
    this._selected = new Set([id]);
    this._primary = id;
    this.emit();
  }

  toggle(id: string): void {
    if (this._selected.has(id)) {
      this._selected.delete(id);
      this._primary = this._selected.size > 0 ? [...this._selected].at(-1)! : null;
    } else {
      this._selected.add(id);
      this._primary = id;
    }
    this.emit();
  }

  selectRange(orderedIds: string[], fromId: string, toId: string): void {
    const a = orderedIds.indexOf(fromId);
    const b = orderedIds.indexOf(toId);
    if (a < 0 || b < 0) return;
    const [lo, hi] = a <= b ? [a, b] : [b, a];
    for (let i = lo; i <= hi; i++) this._selected.add(orderedIds[i]!);
    this._primary = toId;
    this.emit();
  }

  selectAll(ids: string[]): void {
    this._selected = new Set(ids);
    this._primary = ids.at(-1) ?? null;
    this.emit();
  }

  clear(): void {
    this._selected = new Set();
    this._primary = null;
    this.emit();
  }

  isSelected(id: string): boolean {
    return this._selected.has(id);
  }

  get selected(): ReadonlySet<string> {
    return this._selected;
  }

  get selectedIds(): string[] {
    return [...this._selected];
  }

  get primary(): string | null {
    return this._primary;
  }

  get size(): number {
    return this._selected.size;
  }

  get isEmpty(): boolean {
    return this._selected.size === 0;
  }

  get hasMultiple(): boolean {
    return this._selected.size > 1;
  }

  on(listener: SelectionListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit(): void {
    const event: SelectionEvent = { selected: this._selected, primary: this._primary };
    for (const l of this.listeners) l(event);
  }
}
