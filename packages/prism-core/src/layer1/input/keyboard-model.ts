import type { NormalisedKey, KeyEventLike } from "./input-types.js";

export function parseShortcut(shortcut: string): NormalisedKey {
  const parts = shortcut.toLowerCase().split("+");
  const key = parts[parts.length - 1] as string;
  return {
    key,
    cmd: parts.includes("cmd") || parts.includes("ctrl"),
    shift: parts.includes("shift"),
    alt: parts.includes("alt"),
  };
}

export function normaliseKeyEvent(e: KeyEventLike): NormalisedKey {
  return {
    key: e.key.toLowerCase(),
    cmd: e.ctrlKey || e.metaKey,
    shift: e.shiftKey,
    alt: e.altKey,
  };
}

export function keyToShortcut(nk: NormalisedKey): string {
  const parts: string[] = [];
  if (nk.cmd) parts.push("cmd");
  if (nk.shift) parts.push("shift");
  if (nk.alt) parts.push("alt");
  parts.push(nk.key);
  return parts.join("+");
}

function keysMatch(a: NormalisedKey, b: NormalisedKey): boolean {
  return a.key === b.key && a.cmd === b.cmd && a.shift === b.shift && a.alt === b.alt;
}

export class KeyboardModel {
  private bindings = new Map<string, { parsed: NormalisedKey; action: string }>();

  bind(shortcut: string, action: string): this {
    this.bindings.set(shortcut, { parsed: parseShortcut(shortcut), action });
    return this;
  }

  bindAll(map: Record<string, string>): this {
    for (const [shortcut, action] of Object.entries(map)) this.bind(shortcut, action);
    return this;
  }

  unbind(shortcut: string): this {
    this.bindings.delete(shortcut);
    return this;
  }

  applyShortcutMap(map: Map<string, string>): this {
    for (const [shortcut, action] of map) this.bind(shortcut, action);
    return this;
  }

  resolve(e: KeyEventLike): string | null {
    const nk = normaliseKeyEvent(e);
    for (const { parsed, action } of this.bindings.values()) {
      if (keysMatch(parsed, nk)) return action;
    }
    return null;
  }

  allBindings(): Array<{ shortcut: string; action: string }> {
    return [...this.bindings.entries()].map(([shortcut, { action }]) => ({ shortcut, action }));
  }
}
