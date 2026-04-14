import { describe, expect, it } from "vitest";
import {
  SHELL_PUCK_CONFIG,
  SHELL_SLOTS,
  LENS_OUTLET_PUCK_CONFIG,
  createDefaultStudioShellTree,
  DEFAULT_STUDIO_SHELL_TREE,
} from "./shell-components.js";

describe("SHELL_PUCK_CONFIG", () => {
  it("exposes slot fields for every named shell slot", () => {
    const fields = SHELL_PUCK_CONFIG.fields as Record<
      string,
      { type: string }
    >;
    for (const slot of SHELL_SLOTS) {
      expect(fields[slot]).toBeDefined();
      expect(fields[slot]?.type).toBe("slot");
    }
  });

  it("comes with sensible default bar sizes for every bar", () => {
    expect(SHELL_PUCK_CONFIG.defaultProps).toMatchObject({
      activityBarWidth: 48,
      topBarHeight: 36,
      leftBarWidth: 260,
      rightBarWidth: 280,
      bottomBarHeight: 0,
    });
  });

  it("declares Shell category, embeddable, and slot zones", () => {
    expect(SHELL_PUCK_CONFIG.category).toBe("Shell");
    expect(SHELL_PUCK_CONFIG.embeddable).toBe(true);
    expect(SHELL_PUCK_CONFIG.zones).toEqual(SHELL_SLOTS);
  });

  it("lists six slots in drop-palette order", () => {
    expect([...SHELL_SLOTS]).toEqual([
      "activityBar",
      "topBar",
      "leftBar",
      "main",
      "rightBar",
      "bottomBar",
    ]);
  });

  it("attaches a render function the adapter can use", () => {
    expect(typeof SHELL_PUCK_CONFIG.render).toBe("function");
  });
});

describe("LENS_OUTLET_PUCK_CONFIG", () => {
  it("exposes an emptyMessage text field", () => {
    const fields = LENS_OUTLET_PUCK_CONFIG.fields as Record<
      string,
      { type: string }
    >;
    expect(fields["emptyMessage"]?.type).toBe("text");
  });

  it("attaches a render function the adapter can use", () => {
    expect(typeof LENS_OUTLET_PUCK_CONFIG.render).toBe("function");
  });
});

describe("createDefaultStudioShellTree", () => {
  it("builds a Shell root with every chrome slot populated", () => {
    const tree = createDefaultStudioShellTree();
    expect(tree.content).toEqual([]);
    const props = tree.root.props as Record<string, unknown>;
    const activityBar = props["activityBar"] as Array<{ type: string }>;
    const leftBar = props["leftBar"] as Array<{ type: string }>;
    const topBar = props["topBar"] as Array<{ type: string }>;
    const main = props["main"] as Array<{ type: string }>;
    const rightBar = props["rightBar"] as Array<{ type: string }>;

    expect(activityBar.map((c) => c.type)).toEqual(["ActivityBar"]);
    expect(leftBar.map((c) => c.type)).toEqual([
      "ObjectExplorer",
      "ComponentPalette",
    ]);
    expect(topBar.map((c) => c.type)).toEqual([
      "ShellModeMenu",
      "TabBar",
      "PresenceIndicator",
      "UndoStatusBar",
    ]);
    expect(main.map((c) => c.type)).toEqual(["LensOutlet"]);
    expect(rightBar.map((c) => c.type)).toEqual(["InspectorPanel"]);
  });

  it("lets callers override individual widget ids", () => {
    const tree = createDefaultStudioShellTree({
      widgetIds: { inspectorPanel: "CustomInspector" },
    });
    const props = tree.root.props as Record<string, unknown>;
    const rightBar = props["rightBar"] as Array<{ type: string }>;
    expect(rightBar[0]?.type).toBe("CustomInspector");
  });

  it("exports a pre-built DEFAULT_STUDIO_SHELL_TREE constant", () => {
    expect(DEFAULT_STUDIO_SHELL_TREE.content).toEqual([]);
    const props = DEFAULT_STUDIO_SHELL_TREE.root.props as Record<string, unknown>;
    expect(props["main"]).toBeDefined();
  });
});
