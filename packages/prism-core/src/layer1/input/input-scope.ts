import type { UndoHook } from "./input-types.js";
import { KeyboardModel } from "./keyboard-model.js";

export class InputScope {
  readonly id: string;
  readonly label: string;
  readonly keyboard: KeyboardModel;
  readonly handlers: Map<string, () => void | Promise<void>>;
  undoHook: UndoHook | null;

  constructor(id: string, label: string) {
    this.id = id;
    this.label = label;
    this.keyboard = new KeyboardModel();
    this.handlers = new Map();
    this.undoHook = null;

    this.keyboard.bind("cmd+z", "undo");
    this.keyboard.bind("cmd+shift+z", "redo");
    this.keyboard.bind("escape", "escape");
  }

  on(action: string, handler: () => void | Promise<void>): this {
    this.handlers.set(action, handler);
    return this;
  }

  async dispatch(action: string): Promise<boolean> {
    const handler = this.handlers.get(action);
    if (handler) {
      await handler();
      return true;
    }
    if (action === "undo" && this.undoHook) {
      await this.undoHook.undo();
      return true;
    }
    if (action === "redo" && this.undoHook) {
      await this.undoHook.redo();
      return true;
    }
    return false;
  }
}
