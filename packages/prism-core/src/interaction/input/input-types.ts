export interface NormalisedKey {
  key: string;
  cmd: boolean;
  shift: boolean;
  alt: boolean;
}

export interface KeyEventLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export interface UndoHook {
  undo: () => void | Promise<void>;
  redo: () => void | Promise<void>;
}

export type InputRouterEvent =
  | { kind: "pushed"; scopeId: string }
  | { kind: "popped"; scopeId: string }
  | { kind: "dispatched"; action: string; scopeId: string }
  | { kind: "unhandled"; action: string };

export type InputRouterListener = (event: InputRouterEvent) => void;
