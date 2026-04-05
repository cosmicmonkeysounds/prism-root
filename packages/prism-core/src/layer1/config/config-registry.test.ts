import { describe, it, expect, beforeEach } from "vitest";
import { ConfigRegistry } from "./config-registry.js";
import type { SettingDefinition, FeatureFlagDefinition } from "./config-types.js";

describe("ConfigRegistry", () => {
  let registry: ConfigRegistry;

  beforeEach(() => {
    registry = new ConfigRegistry();
  });

  // ── Built-in settings ─────────────────────────────────────────────────────

  it("has built-in settings after construction", () => {
    expect(registry.all().length).toBeGreaterThan(0);
  });

  it("includes ui.theme as a built-in", () => {
    const def = registry.get("ui.theme");
    expect(def).toBeDefined();
    expect(def.type).toBe("select");
    expect(def.default).toBe("system");
  });

  it("includes editor.fontSize as a built-in", () => {
    const def = registry.get("editor.fontSize");
    expect(def).toBeDefined();
    expect(def.default).toBe(14);
  });

  // ── Register / get ────────────────────────────────────────────────────────

  it("registers and retrieves a custom setting", () => {
    const custom: SettingDefinition<number> = {
      key: "custom.speed",
      type: "number",
      default: 42,
      label: "Speed",
    };
    registry.register(custom);
    expect(registry.get("custom.speed")).toBeDefined();
    expect(registry.getDefault("custom.speed")).toBe(42);
  });

  it("replaces existing definition with same key", () => {
    registry.register({
      key: "ui.theme",
      type: "string",
      default: "custom",
      label: "Custom theme",
    });
    expect(registry.getDefault("ui.theme")).toBe("custom");
  });

  it("registerAll registers multiple settings", () => {
    const defs: SettingDefinition[] = [
      { key: "a.one", type: "string", default: "1", label: "One" },
      { key: "a.two", type: "number", default: 2, label: "Two" },
    ];
    registry.registerAll(defs);
    expect(registry.get("a.one")).toBeDefined();
    expect(registry.get("a.two")).toBeDefined();
  });

  // ── Querying ──────────────────────────────────────────────────────────────

  it("byTag filters by tag", () => {
    const uiSettings = registry.byTag("ui");
    expect(uiSettings.length).toBeGreaterThan(0);
    for (const s of uiSettings) {
      expect(s.tags).toContain("ui");
    }
  });

  it("byScope filters by scope", () => {
    const userSettings = registry.byScope("user");
    expect(userSettings.length).toBeGreaterThan(0);
    for (const s of userSettings) {
      expect(!s.scopes || s.scopes.includes("user")).toBe(true);
    }
  });

  it("returns undefined for unknown key", () => {
    expect(registry.get("does.not.exist")).toBeUndefined();
    expect(registry.getDefault("does.not.exist")).toBeUndefined();
  });

  // ── Feature flags ─────────────────────────────────────────────────────────

  it("has built-in feature flags", () => {
    expect(registry.allFlags().length).toBeGreaterThan(0);
  });

  it("registers and retrieves a custom flag", () => {
    const flag: FeatureFlagDefinition = {
      id: "beta-feature",
      label: "Beta",
      default: false,
    };
    registry.registerFlag(flag);
    expect(registry.getFlag("beta-feature")).toBeDefined();
    expect(registry.getFlag("beta-feature")?.default).toBe(false);
  });

  it("registerAllFlags registers multiple flags", () => {
    const flags: FeatureFlagDefinition[] = [
      { id: "f1", label: "F1", default: true },
      { id: "f2", label: "F2", default: false },
    ];
    registry.registerAllFlags(flags);
    expect(registry.getFlag("f1")).toBeDefined();
    expect(registry.getFlag("f2")).toBeDefined();
  });

  // ── Reset ─────────────────────────────────────────────────────────────────

  it("reset clears custom entries and restores built-ins", () => {
    registry.register({
      key: "custom.x",
      type: "string",
      default: "x",
      label: "X",
    });
    registry.registerFlag({ id: "custom-flag", label: "CF", default: true });

    registry.reset();

    expect(registry.get("custom.x")).toBeUndefined();
    expect(registry.getFlag("custom-flag")).toBeUndefined();
    expect(registry.get("ui.theme")).toBeDefined();
    expect(registry.getFlag("ai-features")).toBeDefined();
  });
});
