import type { KeyEventLike, InputRouterEvent, InputRouterListener } from "./input-types.js";
import type { InputScope } from "./input-scope.js";

export class InputRouter {
  private stack: InputScope[] = [];
  private listeners = new Set<InputRouterListener>();

  push(scope: InputScope): void {
    if (this.stack.some((s) => s.id === scope.id)) return;
    this.stack.push(scope);
    this.emit({ kind: "pushed", scopeId: scope.id });
  }

  pop(scopeId: string): void {
    const idx = this.stack.findIndex((s) => s.id === scopeId);
    if (idx < 0) return;
    this.stack.splice(idx, 1);
    this.emit({ kind: "popped", scopeId });
  }

  replace(scope: InputScope): void {
    this.pop(scope.id);
    // Re-add without dedup check since we just removed it
    this.stack.push(scope);
    this.emit({ kind: "pushed", scopeId: scope.id });
  }

  async handleKeyEvent(e: KeyEventLike): Promise<boolean> {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const scope = this.stack[i]!;
      const action = scope.keyboard.resolve(e);
      if (!action) continue;
      const handled = await scope.dispatch(action);
      if (handled) {
        this.emit({ kind: "dispatched", action, scopeId: scope.id });
        return true;
      }
    }
    return false;
  }

  async dispatch(action: string): Promise<boolean> {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const scope = this.stack[i]!;
      const handled = await scope.dispatch(action);
      if (handled) {
        this.emit({ kind: "dispatched", action, scopeId: scope.id });
        return true;
      }
    }
    this.emit({ kind: "unhandled", action });
    return false;
  }

  get activeScope(): InputScope | null {
    return this.stack[this.stack.length - 1] ?? null;
  }

  get stackDepth(): number {
    return this.stack.length;
  }

  getScope(id: string): InputScope | undefined {
    return this.stack.find((s) => s.id === id);
  }

  get allScopes(): InputScope[] {
    return [...this.stack];
  }

  on(listener: InputRouterListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit(event: InputRouterEvent): void {
    for (const l of this.listeners) l(event);
  }
}
