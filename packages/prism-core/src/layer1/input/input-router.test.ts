import { describe, it, expect, vi } from "vitest";
import { InputScope } from "./input-scope.js";
import { InputRouter } from "./input-router.js";
import type { KeyEventLike, InputRouterEvent } from "./input-types.js";

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

describe("InputScope", () => {
  it("has default undo/redo/escape bindings", () => {
    const scope = new InputScope("test", "Test");
    const bindings = scope.keyboard.allBindings();
    const actions = bindings.map((b) => b.action);
    expect(actions).toContain("undo");
    expect(actions).toContain("redo");
    expect(actions).toContain("escape");
  });

  it("fluent .on() registers handlers", () => {
    const fn = vi.fn();
    const scope = new InputScope("test", "Test").on("save", fn);
    expect(scope).toBeInstanceOf(InputScope);
    expect(scope.handlers.has("save")).toBe(true);
  });

  it("dispatch calls registered handler", async () => {
    const fn = vi.fn();
    const scope = new InputScope("test", "Test").on("save", fn);
    const handled = await scope.dispatch("save");
    expect(handled).toBe(true);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("dispatch returns false for unknown action", async () => {
    const scope = new InputScope("test", "Test");
    expect(await scope.dispatch("unknown")).toBe(false);
  });

  it("dispatch falls back to undoHook for undo/redo", async () => {
    const undo = vi.fn();
    const redo = vi.fn();
    const scope = new InputScope("test", "Test");
    scope.undoHook = { undo, redo };
    await scope.dispatch("undo");
    await scope.dispatch("redo");
    expect(undo).toHaveBeenCalledOnce();
    expect(redo).toHaveBeenCalledOnce();
  });

  it("explicit handler takes precedence over undoHook", async () => {
    const hookUndo = vi.fn();
    const handlerUndo = vi.fn();
    const scope = new InputScope("test", "Test").on("undo", handlerUndo);
    scope.undoHook = { undo: hookUndo, redo: vi.fn() };
    await scope.dispatch("undo");
    expect(handlerUndo).toHaveBeenCalledOnce();
    expect(hookUndo).not.toHaveBeenCalled();
  });
});

describe("InputRouter", () => {
  describe("stack management", () => {
    it("push adds scope to stack", () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      expect(router.stackDepth).toBe(1);
      expect(router.activeScope?.id).toBe("a");
    });

    it("push deduplicates by id", () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      router.push(new InputScope("a", "A2"));
      expect(router.stackDepth).toBe(1);
    });

    it("pop removes scope from stack", () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      router.push(new InputScope("b", "B"));
      router.pop("a");
      expect(router.stackDepth).toBe(1);
      expect(router.activeScope?.id).toBe("b");
    });

    it("pop is no-op for missing id", () => {
      const router = new InputRouter();
      router.pop("nonexistent");
      expect(router.stackDepth).toBe(0);
    });

    it("replace swaps scope", () => {
      const router = new InputRouter();
      const original = new InputScope("a", "Original");
      router.push(original);
      const replacement = new InputScope("a", "Replaced");
      replacement.on("custom", vi.fn());
      router.replace(replacement);
      expect(router.stackDepth).toBe(1);
      expect(router.activeScope?.label).toBe("Replaced");
    });
  });

  describe("handleKeyEvent", () => {
    it("resolves top scope first", async () => {
      const router = new InputRouter();
      const bottom = new InputScope("bottom", "Bottom");
      const top = new InputScope("top", "Top");
      const bottomFn = vi.fn();
      const topFn = vi.fn();
      bottom.keyboard.bind("cmd+s", "save");
      bottom.on("save", bottomFn);
      top.keyboard.bind("cmd+s", "save");
      top.on("save", topFn);
      router.push(bottom);
      router.push(top);

      const result = await router.handleKeyEvent(fakeKey("s", { meta: true }));
      expect(result).toBe(true);
      expect(topFn).toHaveBeenCalledOnce();
      expect(bottomFn).not.toHaveBeenCalled();
    });

    it("falls through to lower scope if top does not handle", async () => {
      const router = new InputRouter();
      const bottom = new InputScope("bottom", "Bottom");
      const top = new InputScope("top", "Top");
      const bottomFn = vi.fn();
      bottom.keyboard.bind("cmd+s", "save");
      bottom.on("save", bottomFn);
      // top has no cmd+s binding
      router.push(bottom);
      router.push(top);

      const result = await router.handleKeyEvent(fakeKey("s", { meta: true }));
      expect(result).toBe(true);
      expect(bottomFn).toHaveBeenCalledOnce();
    });

    it("returns false when no scope handles event", async () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      const result = await router.handleKeyEvent(fakeKey("q", { meta: true }));
      expect(result).toBe(false);
    });
  });

  describe("dispatch (action string)", () => {
    it("dispatches to top scope with handler", async () => {
      const router = new InputRouter();
      const fn = vi.fn();
      const scope = new InputScope("a", "A").on("my-action", fn);
      router.push(scope);
      const result = await router.dispatch("my-action");
      expect(result).toBe(true);
      expect(fn).toHaveBeenCalledOnce();
    });

    it("emits unhandled when no scope handles", async () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      const events: InputRouterEvent[] = [];
      router.on((e) => events.push(e));
      await router.dispatch("unknown-action");
      expect(events).toContainEqual({ kind: "unhandled", action: "unknown-action" });
    });
  });

  describe("read accessors", () => {
    it("activeScope returns null for empty stack", () => {
      const router = new InputRouter();
      expect(router.activeScope).toBeNull();
    });

    it("getScope finds by id", () => {
      const router = new InputRouter();
      const scope = new InputScope("a", "A");
      router.push(scope);
      expect(router.getScope("a")).toBe(scope);
      expect(router.getScope("b")).toBeUndefined();
    });

    it("allScopes returns copy", () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      router.push(new InputScope("b", "B"));
      const scopes = router.allScopes;
      expect(scopes).toHaveLength(2);
      scopes.pop();
      expect(router.stackDepth).toBe(2);
    });
  });

  describe("events", () => {
    it("emits pushed on push", () => {
      const router = new InputRouter();
      const events: InputRouterEvent[] = [];
      router.on((e) => events.push(e));
      router.push(new InputScope("a", "A"));
      expect(events).toEqual([{ kind: "pushed", scopeId: "a" }]);
    });

    it("emits popped on pop", () => {
      const router = new InputRouter();
      router.push(new InputScope("a", "A"));
      const events: InputRouterEvent[] = [];
      router.on((e) => events.push(e));
      router.pop("a");
      expect(events).toEqual([{ kind: "popped", scopeId: "a" }]);
    });

    it("emits dispatched on handled event", async () => {
      const router = new InputRouter();
      const scope = new InputScope("a", "A").on("test", vi.fn());
      router.push(scope);
      const events: InputRouterEvent[] = [];
      router.on((e) => events.push(e));
      await router.dispatch("test");
      expect(events).toContainEqual({ kind: "dispatched", action: "test", scopeId: "a" });
    });

    it("unsubscribe removes listener", () => {
      const router = new InputRouter();
      const events: InputRouterEvent[] = [];
      const unsub = router.on((e) => events.push(e));
      router.push(new InputScope("a", "A"));
      unsub();
      router.push(new InputScope("b", "B"));
      expect(events).toHaveLength(1);
    });
  });
});
