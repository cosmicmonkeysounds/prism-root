import { describe, it, expect } from "vitest";
import { PageRegistry } from "./page-registry.js";

type TestTarget = { kind: string; id?: string };

function makeRegistry() {
  return new PageRegistry<TestTarget>()
    .register("object", {
      defaultViewMode: "list",
      defaultTab: "overview",
      getObjectId: (t) => t.id ?? null,
    })
    .register("home", {
      defaultViewMode: "dashboard",
      defaultTab: "home",
    });
}

describe("PageRegistry", () => {
  it("registers and retrieves definitions", () => {
    const reg = makeRegistry();
    expect(reg.get("object").defaultViewMode).toBe("list");
    expect(reg.get("home").defaultTab).toBe("home");
  });

  it("returns fallback for unknown kind", () => {
    const reg = makeRegistry();
    const def = reg.get("unknown");
    expect(def.defaultViewMode).toBe("list");
    expect(def.defaultTab).toBe("overview");
  });

  it("creates page with registered defaults", () => {
    const reg = makeRegistry();
    const page = reg.createPage({ kind: "object", id: "task-1" });
    expect(page.viewMode).toBe("list");
    expect(page.activeTab).toBe("overview");
    expect(page.objectId).toBe("task-1");
  });

  it("creates page with custom pageId", () => {
    const reg = makeRegistry();
    const page = reg.createPage({ kind: "home" }, "custom-id");
    expect(page.id).toBe("custom-id");
  });

  it("getObjectId returns null when not provided", () => {
    const reg = makeRegistry();
    const page = reg.createPage({ kind: "home" });
    expect(page.objectId).toBeNull();
  });

  it("has returns true for registered kinds", () => {
    const reg = makeRegistry();
    expect(reg.has("object")).toBe(true);
    expect(reg.has("missing")).toBe(false);
  });

  it("registeredKinds returns all kinds", () => {
    const reg = makeRegistry();
    expect(reg.registeredKinds()).toEqual(["object", "home"]);
  });

  it("fluent chaining works", () => {
    const reg = new PageRegistry<TestTarget>();
    const result = reg.register("a", { defaultViewMode: "v", defaultTab: "t" });
    expect(result).toBe(reg);
  });
});
