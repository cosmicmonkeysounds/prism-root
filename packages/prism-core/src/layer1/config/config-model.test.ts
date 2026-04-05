import { describe, it, expect, beforeEach, vi } from "vitest";
import { ConfigModel } from "./config-model.js";
import { ConfigRegistry } from "./config-registry.js";
import { MemoryConfigStore } from "./config-store.js";
import type { SettingChange } from "./config-types.js";

describe("ConfigModel", () => {
  let registry: ConfigRegistry;
  let model: ConfigModel;

  beforeEach(() => {
    registry = new ConfigRegistry();
    model = new ConfigModel(registry);
  });

  // ── get / defaults ────────────────────────────────────────────────────────

  it("returns registry default when no scope has the key", () => {
    expect(model.get<string>("ui.theme")).toBe("system");
  });

  it("returns fallback for unknown key", () => {
    expect(model.get("unknown.key", "fallback")).toBe("fallback");
  });

  it("returns undefined for unknown key with no fallback", () => {
    expect(model.get("unknown.key")).toBeUndefined();
  });

  // ── load ──────────────────────────────────────────────────────────────────

  it("load replaces scope values", () => {
    model.load("workspace", { "ui.theme": "dark" });
    expect(model.get<string>("ui.theme")).toBe("dark");

    model.load("workspace", { "ui.theme": "light" });
    expect(model.get<string>("ui.theme")).toBe("light");
  });

  it("load clears previous scope values not in new set", () => {
    model.load("workspace", { "ui.theme": "dark", "editor.fontSize": 16 });
    model.load("workspace", { "ui.theme": "light" });
    expect(model.getAtScope("editor.fontSize", "workspace")).toBeUndefined();
  });

  // ── scope cascade ─────────────────────────────────────────────────────────

  it("user scope overrides workspace scope", () => {
    model.load("workspace", { "ui.theme": "dark" });
    model.load("user", { "ui.theme": "light" });
    expect(model.get<string>("ui.theme")).toBe("light");
  });

  it("workspace scope overrides default", () => {
    model.load("workspace", { "ui.theme": "dark" });
    expect(model.get<string>("ui.theme")).toBe("dark");
  });

  it("respects scope restrictions", () => {
    // ui.language only allows 'user' scope
    model.load("workspace", { "ui.language": "fr" });
    // Still returns default because workspace is not allowed for ui.language
    expect(model.get<string>("ui.language")).toBe("en");
  });

  // ── getAtScope / getScope ─────────────────────────────────────────────────

  it("getAtScope returns scope-specific value", () => {
    model.load("workspace", { "ui.theme": "dark" });
    model.load("user", { "ui.theme": "light" });
    expect(model.getAtScope("ui.theme", "workspace")).toBe("dark");
    expect(model.getAtScope("ui.theme", "user")).toBe("light");
  });

  it("getScope returns all values for a scope", () => {
    model.load("workspace", { "ui.theme": "dark", "sync.enabled": true });
    const scope = model.getScope("workspace");
    expect(scope["ui.theme"]).toBe("dark");
    expect(scope["sync.enabled"]).toBe(true);
  });

  // ── isOverridden ──────────────────────────────────────────────────────────

  it("isOverridden returns false for unset keys", () => {
    expect(model.isOverridden("ui.theme")).toBe(false);
  });

  it("isOverridden returns true when set in non-default scope", () => {
    model.load("user", { "ui.theme": "dark" });
    expect(model.isOverridden("ui.theme")).toBe(true);
  });

  // ── set ───────────────────────────────────────────────────────────────────

  it("set updates value in given scope", () => {
    model.set("ui.theme", "dark", "user");
    expect(model.get<string>("ui.theme")).toBe("dark");
    expect(model.getAtScope("ui.theme", "user")).toBe("dark");
  });

  it("set throws on validation failure", () => {
    expect(() => model.set("editor.fontSize", 100, "user")).toThrow(
      "Must be between 8 and 32",
    );
  });

  it("set throws on disallowed scope", () => {
    expect(() => model.set("ui.language", "fr", "workspace")).toThrow(
      "does not allow scope",
    );
  });

  it("set persists to attached store", () => {
    const store = new MemoryConfigStore();
    model.attachStore("user", store);
    model.set("ui.theme", "dark", "user");
    expect(store.snapshot["ui.theme"]).toBe("dark");
  });

  // ── reset ─────────────────────────────────────────────────────────────────

  it("reset removes value from scope and falls back", () => {
    model.load("workspace", { "ui.theme": "dark" });
    model.set("ui.theme", "light", "user");
    expect(model.get<string>("ui.theme")).toBe("light");

    model.reset("ui.theme", "user");
    expect(model.get<string>("ui.theme")).toBe("dark");
  });

  // ── watch ─────────────────────────────────────────────────────────────────

  it("watch calls immediately with current value", () => {
    const values: string[] = [];
    model.watch<string>("ui.theme", (v) => values.push(v));
    expect(values).toEqual(["system"]);
  });

  it("watch calls on change", () => {
    const values: string[] = [];
    model.watch<string>("ui.theme", (v) => values.push(v));
    model.set("ui.theme", "dark", "user");
    expect(values).toEqual(["system", "dark"]);
  });

  it("watch unsubscribe stops notifications", () => {
    const values: string[] = [];
    const unsub = model.watch<string>("ui.theme", (v) => values.push(v));
    unsub();
    model.set("ui.theme", "dark", "user");
    expect(values).toEqual(["system"]);
  });

  // ── on('change') ──────────────────────────────────────────────────────────

  it("on change listener fires for any key change", () => {
    const changes: SettingChange[] = [];
    model.on("change", (c) => changes.push(c));
    model.set("ui.theme", "dark", "user");
    expect(changes.length).toBe(1);
    expect(changes[0].key).toBe("ui.theme");
    expect(changes[0].previousValue).toBe("system");
    expect(changes[0].newValue).toBe("dark");
  });

  it("on change unsubscribe stops notifications", () => {
    const changes: SettingChange[] = [];
    const unsub = model.on("change", (c) => changes.push(c));
    unsub();
    model.set("ui.theme", "dark", "user");
    expect(changes.length).toBe(0);
  });

  it("does not fire watchers when resolved value unchanged", () => {
    model.load("user", { "ui.theme": "dark" });
    const fn = vi.fn();
    model.on("change", fn);
    // Set workspace to same value that user already overrides — resolved value unchanged
    model.load("workspace", { "ui.theme": "light" });
    // Since user scope still wins with 'dark', no change event for ui.theme
    // Actually workspace scope is lower priority, so resolved stays 'dark'
    expect(fn).not.toHaveBeenCalled();
  });

  // ── attachStore ───────────────────────────────────────────────────────────

  it("attachStore loads initial values from store", () => {
    const store = new MemoryConfigStore({ "ui.theme": "dark" });
    model.attachStore("user", store);
    expect(model.get<string>("ui.theme")).toBe("dark");
  });

  it("attachStore reacts to external changes", () => {
    const store = new MemoryConfigStore({ "ui.theme": "dark" });
    model.attachStore("user", store);
    store.simulateExternalChange({ "ui.theme": "light" });
    expect(model.get<string>("ui.theme")).toBe("light");
  });

  it("detachStore stops reacting to external changes", () => {
    const store = new MemoryConfigStore({ "ui.theme": "dark" });
    model.attachStore("user", store);
    model.detachStore("user");
    store.simulateExternalChange({ "ui.theme": "light" });
    // After detach, external changes don't propagate
    // But the loaded values remain
    expect(model.get<string>("ui.theme")).toBe("dark");
  });

  // ── toJSON ────────────────────────────────────────────────────────────────

  it("toJSON masks secret values", () => {
    model.set("ai.apiKey", "sk-secret-123", "workspace");
    const json = model.toJSON("workspace");
    expect(json["ai.apiKey"]).toBe("***");
  });

  it("toJSON returns non-secret values as-is", () => {
    model.set("ui.theme", "dark", "user");
    const json = model.toJSON("user");
    expect(json["ui.theme"]).toBe("dark");
  });
});
