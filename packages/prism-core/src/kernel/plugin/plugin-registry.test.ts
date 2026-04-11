import { describe, it, expect, beforeEach } from "vitest";
import { PluginRegistry } from "./plugin-registry.js";
import type { PrismPlugin, PluginRegistryEvent } from "./index.js";

const editorPlugin: PrismPlugin = {
  id: "editor",
  name: "Editor",
  contributes: {
    views: [
      {
        id: "editor:main",
        label: "Editor",
        zone: "content",
        componentId: "EditorPanel",
      },
    ],
    commands: [
      {
        id: "editor:format",
        label: "Format Document",
        category: "Edit",
        action: "editor.format",
      },
    ],
    keybindings: [
      { command: "editor:format", key: "cmd+shift+f" },
    ],
  },
};

const graphPlugin: PrismPlugin = {
  id: "graph",
  name: "Graph",
  contributes: {
    views: [
      {
        id: "graph:main",
        label: "Graph View",
        zone: "content",
        componentId: "GraphPanel",
      },
    ],
    commands: [
      {
        id: "graph:layout",
        label: "Auto Layout",
        category: "View",
        action: "graph.autoLayout",
      },
    ],
    contextMenus: [
      {
        id: "graph:delete-node",
        label: "Delete Node",
        context: "graph-node",
        action: "graph.deleteNode",
        danger: true,
      },
    ],
  },
};

const barePlugin: PrismPlugin = {
  id: "bare",
  name: "Bare Plugin",
};

describe("PluginRegistry", () => {
  let registry: PluginRegistry;

  beforeEach(() => {
    registry = new PluginRegistry();
  });

  describe("register", () => {
    it("registers a plugin and its contributions", () => {
      registry.register(editorPlugin);
      expect(registry.has("editor")).toBe(true);
      expect(registry.size).toBe(1);
    });

    it("auto-registers views into contribution registry", () => {
      registry.register(editorPlugin);
      expect(registry.views.get("editor:main")).toBeDefined();
    });

    it("auto-registers commands into contribution registry", () => {
      registry.register(editorPlugin);
      expect(registry.commands.get("editor:format")).toBeDefined();
    });

    it("auto-registers keybindings", () => {
      registry.register(editorPlugin);
      expect(registry.keybindings.size).toBe(1);
    });

    it("auto-registers context menus", () => {
      registry.register(graphPlugin);
      expect(registry.contextMenus.get("graph:delete-node")).toBeDefined();
    });

    it("handles plugins with no contributions", () => {
      registry.register(barePlugin);
      expect(registry.has("bare")).toBe(true);
      expect(registry.views.size).toBe(0);
    });

    it("returns an unregister function", () => {
      const unregister = registry.register(editorPlugin);
      expect(registry.has("editor")).toBe(true);
      unregister();
      expect(registry.has("editor")).toBe(false);
    });
  });

  describe("unregister", () => {
    it("removes plugin and all its contributions", () => {
      registry.register(editorPlugin);
      registry.register(graphPlugin);
      registry.unregister("editor");
      expect(registry.has("editor")).toBe(false);
      expect(registry.views.get("editor:main")).toBeUndefined();
      expect(registry.commands.get("editor:format")).toBeUndefined();
      expect(registry.keybindings.size).toBe(0);
      // graph contributions survive
      expect(registry.views.get("graph:main")).toBeDefined();
    });

    it("returns false for unknown plugin", () => {
      expect(registry.unregister("nope")).toBe(false);
    });
  });

  describe("queries", () => {
    it("get() returns plugin by ID", () => {
      registry.register(editorPlugin);
      expect(registry.get("editor")?.name).toBe("Editor");
    });

    it("all() returns all plugins", () => {
      registry.register(editorPlugin);
      registry.register(graphPlugin);
      expect(registry.all()).toHaveLength(2);
    });
  });

  describe("events", () => {
    it("fires registered event", () => {
      const events: PluginRegistryEvent[] = [];
      registry.subscribe((e) => events.push(e));
      registry.register(editorPlugin);
      expect(events).toHaveLength(1);
      expect(events[0].type).toBe("registered");
      expect(events[0].pluginId).toBe("editor");
    });

    it("fires unregistered event", () => {
      const events: PluginRegistryEvent[] = [];
      registry.register(editorPlugin);
      registry.subscribe((e) => events.push(e));
      registry.unregister("editor");
      expect(events).toHaveLength(1);
      expect(events[0].type).toBe("unregistered");
    });

    it("subscribe returns unsubscribe function", () => {
      const events: PluginRegistryEvent[] = [];
      const unsub = registry.subscribe((e) => events.push(e));
      registry.register(editorPlugin);
      expect(events).toHaveLength(1);
      unsub();
      registry.register(graphPlugin);
      expect(events).toHaveLength(1);
    });
  });

  describe("contribution queries across plugins", () => {
    it("views from multiple plugins are aggregated", () => {
      registry.register(editorPlugin);
      registry.register(graphPlugin);
      expect(registry.views.all()).toHaveLength(2);
    });

    it("commands from multiple plugins are aggregated", () => {
      registry.register(editorPlugin);
      registry.register(graphPlugin);
      expect(registry.commands.all()).toHaveLength(2);
    });

    it("byPlugin filters contributions to a single plugin", () => {
      registry.register(editorPlugin);
      registry.register(graphPlugin);
      const editorViews = registry.views.byPlugin("editor");
      expect(editorViews).toHaveLength(1);
      expect(editorViews[0].id).toBe("editor:main");
    });
  });
});
