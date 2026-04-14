import { describe, it, expect } from "vitest";
import { createPageBuilderRegistry } from "./entities.js";

describe("page builder registry — app builder entities", () => {
  const registry = createPageBuilderRegistry();

  it("registers app, app-shell, route, and behavior types", () => {
    expect(registry.get("app")).toBeDefined();
    expect(registry.get("app-shell")).toBeDefined();
    expect(registry.get("route")).toBeDefined();
    expect(registry.get("behavior")).toBeDefined();
  });

  it("app can be a workspace root", () => {
    expect(registry.canBeRoot("app")).toBe(true);
  });

  it("app accepts app-shell, route, page, and behavior as children", () => {
    expect(registry.canBeChildOf("app-shell", "app")).toBe(true);
    expect(registry.canBeChildOf("route", "app")).toBe(true);
    expect(registry.canBeChildOf("page", "app")).toBe(true);
    expect(registry.canBeChildOf("behavior", "app")).toBe(true);
  });

  it("routes cannot contain children", () => {
    expect(registry.canHaveChildren("route")).toBe(false);
  });

  it("behaviors cannot contain children", () => {
    expect(registry.canHaveChildren("behavior")).toBe(false);
  });

  it("routes and behaviors cannot be workspace roots", () => {
    expect(registry.canBeRoot("route")).toBe(false);
    expect(registry.canBeRoot("behavior")).toBe(false);
  });

  it("app-shell is a component — can live under any page-parent too", () => {
    expect(registry.get("app-shell")?.category).toBe("component");
    // page.canParent includes "component", so an app-shell *could* sit
    // under a page too (useful if the author wants a per-page chrome
    // override). Don't forbid it — just confirm the containment lookup
    // treats it like any other component.
    expect(registry.canBeChildOf("app-shell", "page")).toBe(true);
  });

  it("route field schema exposes path, label, pageId, parentRouteId, showInNav", () => {
    const def = registry.get("route");
    const fieldIds = (def?.fields ?? []).map((f) => f.id).sort();
    expect(fieldIds).toContain("path");
    expect(fieldIds).toContain("label");
    expect(fieldIds).toContain("pageId");
    expect(fieldIds).toContain("parentRouteId");
    expect(fieldIds).toContain("showInNav");
  });

  it("behavior field schema exposes trigger, source, enabled, targetObjectId", () => {
    const def = registry.get("behavior");
    const fieldIds = (def?.fields ?? []).map((f) => f.id).sort();
    expect(fieldIds).toContain("trigger");
    expect(fieldIds).toContain("source");
    expect(fieldIds).toContain("enabled");
    expect(fieldIds).toContain("targetObjectId");
  });

  it("app field schema exposes profileId with the five app enum options", () => {
    const def = registry.get("app");
    const profileField = def?.fields?.find((f) => f.id === "profileId");
    expect(profileField?.type).toBe("enum");
    const values = (profileField?.enumOptions ?? []).map((o) => o.value).sort();
    expect(values).toEqual(["cadence", "flux", "grip", "lattice", "studio"]);
  });

  it("app-shell defaults the sticky top bar to true", () => {
    const def = registry.get("app-shell");
    const sticky = def?.fields?.find((f) => f.id === "stickyTopBar");
    expect(sticky?.default).toBe(true);
  });
});
