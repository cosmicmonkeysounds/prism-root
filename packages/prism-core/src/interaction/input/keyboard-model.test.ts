import { describe, it, expect } from "vitest";
import {
  parseShortcut,
  normaliseKeyEvent,
  keyToShortcut,
  KeyboardModel,
} from "./keyboard-model.js";
import type { KeyEventLike } from "./input-types.js";

function fakeKey(
  key: string,
  mods: { ctrl?: boolean; meta?: boolean; shift?: boolean; alt?: boolean } = {},
): KeyEventLike {
  return {
    key,
    ctrlKey: mods.ctrl ?? false,
    metaKey: mods.meta ?? false,
    shiftKey: mods.shift ?? false,
    altKey: mods.alt ?? false,
  };
}

describe("parseShortcut", () => {
  it("parses a simple key", () => {
    expect(parseShortcut("escape")).toEqual({ key: "escape", cmd: false, shift: false, alt: false });
  });

  it("parses cmd+k", () => {
    expect(parseShortcut("cmd+k")).toEqual({ key: "k", cmd: true, shift: false, alt: false });
  });

  it("treats ctrl as cmd", () => {
    expect(parseShortcut("ctrl+z")).toEqual({ key: "z", cmd: true, shift: false, alt: false });
  });

  it("parses cmd+shift+z", () => {
    expect(parseShortcut("cmd+shift+z")).toEqual({ key: "z", cmd: true, shift: true, alt: false });
  });

  it("parses alt+p", () => {
    expect(parseShortcut("alt+p")).toEqual({ key: "p", cmd: false, shift: false, alt: true });
  });

  it("is case-insensitive", () => {
    expect(parseShortcut("CMD+SHIFT+Z")).toEqual({ key: "z", cmd: true, shift: true, alt: false });
  });
});

describe("normaliseKeyEvent", () => {
  it("normalises Meta key as cmd", () => {
    const nk = normaliseKeyEvent(fakeKey("k", { meta: true }));
    expect(nk).toEqual({ key: "k", cmd: true, shift: false, alt: false });
  });

  it("normalises Ctrl key as cmd", () => {
    const nk = normaliseKeyEvent(fakeKey("z", { ctrl: true }));
    expect(nk).toEqual({ key: "z", cmd: true, shift: false, alt: false });
  });

  it("lowercases the key", () => {
    const nk = normaliseKeyEvent(fakeKey("K", { meta: true }));
    expect(nk.key).toBe("k");
  });

  it("handles shift+alt", () => {
    const nk = normaliseKeyEvent(fakeKey("a", { shift: true, alt: true }));
    expect(nk).toEqual({ key: "a", cmd: false, shift: true, alt: true });
  });
});

describe("keyToShortcut", () => {
  it("serialises cmd+k", () => {
    expect(keyToShortcut({ key: "k", cmd: true, shift: false, alt: false })).toBe("cmd+k");
  });

  it("serialises cmd+shift+z", () => {
    expect(keyToShortcut({ key: "z", cmd: true, shift: true, alt: false })).toBe("cmd+shift+z");
  });

  it("serialises plain key", () => {
    expect(keyToShortcut({ key: "escape", cmd: false, shift: false, alt: false })).toBe("escape");
  });

  it("round-trips with parseShortcut", () => {
    const shortcuts = ["cmd+k", "cmd+shift+z", "alt+p", "escape", "f2"];
    for (const s of shortcuts) {
      expect(keyToShortcut(parseShortcut(s))).toBe(s);
    }
  });
});

describe("KeyboardModel", () => {
  it("resolves a bound shortcut", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+k", "palette:open");
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBe("palette:open");
  });

  it("returns null for unbound shortcut", () => {
    const kb = new KeyboardModel();
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBeNull();
  });

  it("later binding for same shortcut wins", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+k", "first");
    kb.bind("cmd+k", "second");
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBe("second");
  });

  it("discriminates cmd+k from plain k", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+k", "with-cmd");
    kb.bind("k", "plain");
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBe("with-cmd");
    expect(kb.resolve(fakeKey("k"))).toBe("plain");
  });

  it("discriminates cmd+z from cmd+shift+z", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+z", "undo");
    kb.bind("cmd+shift+z", "redo");
    expect(kb.resolve(fakeKey("z", { meta: true }))).toBe("undo");
    expect(kb.resolve(fakeKey("z", { meta: true, shift: true }))).toBe("redo");
  });

  it("resolves escape", () => {
    const kb = new KeyboardModel();
    kb.bind("escape", "close");
    expect(kb.resolve(fakeKey("Escape"))).toBe("close");
  });

  it("unbind removes binding", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+k", "open");
    kb.unbind("cmd+k");
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBeNull();
  });

  it("bindAll registers multiple bindings", () => {
    const kb = new KeyboardModel();
    kb.bindAll({ "cmd+k": "open", "cmd+s": "save" });
    expect(kb.resolve(fakeKey("k", { meta: true }))).toBe("open");
    expect(kb.resolve(fakeKey("s", { meta: true }))).toBe("save");
  });

  it("applyShortcutMap registers from Map", () => {
    const kb = new KeyboardModel();
    kb.applyShortcutMap(new Map([["cmd+p", "search"]]));
    expect(kb.resolve(fakeKey("p", { meta: true }))).toBe("search");
  });

  it("allBindings returns all registered pairs", () => {
    const kb = new KeyboardModel();
    kb.bind("cmd+k", "open").bind("escape", "close");
    const all = kb.allBindings();
    expect(all).toHaveLength(2);
    expect(all).toContainEqual({ shortcut: "cmd+k", action: "open" });
    expect(all).toContainEqual({ shortcut: "escape", action: "close" });
  });

  it("fluent chaining works", () => {
    const kb = new KeyboardModel();
    const result = kb.bind("cmd+k", "open").bind("escape", "close").unbind("escape");
    expect(result).toBe(kb);
  });
});
