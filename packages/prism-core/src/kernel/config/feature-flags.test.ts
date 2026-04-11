import { describe, it, expect, beforeEach } from "vitest";
import { FeatureFlags } from "./feature-flags.js";
import { ConfigRegistry } from "./config-registry.js";
import { ConfigModel } from "./config-model.js";

describe("FeatureFlags", () => {
  let registry: ConfigRegistry;
  let model: ConfigModel;
  let flags: FeatureFlags;

  beforeEach(() => {
    registry = new ConfigRegistry();
    model = new ConfigModel(registry);
    flags = new FeatureFlags(registry, model);
  });

  // ── isEnabled ─────────────────────────────────────────────────────────────

  it("returns default when no conditions or config override", () => {
    registry.registerFlag({
      id: "test-flag",
      label: "Test",
      default: true,
    });
    expect(flags.isEnabled("test-flag")).toBe(true);
  });

  it("returns false for unknown flag", () => {
    expect(flags.isEnabled("nonexistent")).toBe(false);
  });

  it("config override takes precedence over conditions", () => {
    registry.registerFlag({
      id: "test-flag",
      label: "Test",
      default: false,
      settingKey: "test.enabled",
      conditions: [{ type: "always", value: true }],
    });
    registry.register({
      key: "test.enabled",
      type: "boolean",
      default: false,
      label: "Test enabled",
    });
    model.set("test.enabled", false, "user");
    expect(flags.isEnabled("test-flag")).toBe(false);
  });

  it("evaluates 'always' condition", () => {
    registry.registerFlag({
      id: "always-on",
      label: "Always",
      default: false,
      conditions: [{ type: "always", value: true }],
    });
    expect(flags.isEnabled("always-on")).toBe(true);
  });

  it("evaluates 'config' condition with context", () => {
    registry.registerFlag({
      id: "config-flag",
      label: "Config",
      default: false,
      conditions: [
        { type: "config", key: "env", equals: "production", value: true },
      ],
    });
    expect(
      flags.isEnabled("config-flag", {
        config: { env: "production" },
      }),
    ).toBe(true);
    expect(
      flags.isEnabled("config-flag", {
        config: { env: "development" },
      }),
    ).toBe(false);
  });

  it("first matching condition wins", () => {
    registry.registerFlag({
      id: "multi",
      label: "Multi",
      default: false,
      conditions: [
        { type: "always", value: true },
        { type: "always", value: false },
      ],
    });
    expect(flags.isEnabled("multi")).toBe(true);
  });

  // ── built-in ai-features flag ─────────────────────────────────────────────

  it("ai-features flag delegates to ai.enabled setting", () => {
    expect(flags.isEnabled("ai-features")).toBe(true); // default
    model.set("ai.enabled", false, "workspace");
    expect(flags.isEnabled("ai-features")).toBe(false);
  });

  // ── getAll ────────────────────────────────────────────────────────────────

  it("getAll returns map of all flags", () => {
    const all = flags.getAll();
    expect(typeof all["ai-features"]).toBe("boolean");
    expect(typeof all["sync"]).toBe("boolean");
  });

  // ── watch ─────────────────────────────────────────────────────────────────

  it("watch calls immediately with current value", () => {
    const values: boolean[] = [];
    flags.watch("ai-features", (v) => values.push(v));
    expect(values).toEqual([true]);
  });

  it("watch fires when linked config key changes", () => {
    const values: boolean[] = [];
    flags.watch("ai-features", (v) => values.push(v));
    model.set("ai.enabled", false, "workspace");
    expect(values).toEqual([true, false]);
  });

  it("watch unsubscribe stops notifications", () => {
    const values: boolean[] = [];
    const unsub = flags.watch("ai-features", (v) => values.push(v));
    unsub();
    model.set("ai.enabled", false, "workspace");
    expect(values).toEqual([true]);
  });
});
