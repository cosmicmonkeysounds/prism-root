import { describe, expect, it } from "vitest";
import { createAdminPuckConfig } from "./puck-config.js";
import { createDefaultAdminLayout } from "./default-layout.js";

describe("createAdminPuckConfig", () => {
  const config = createAdminPuckConfig();

  it("registers every widget as a Puck component", () => {
    const expected = [
      "SourceHeader",
      "HealthBadge",
      "UptimeCard",
      "MetricCard",
      "MetricChart",
      "ServiceList",
      "ActivityTail",
    ];
    for (const name of expected) {
      expect(config.components).toHaveProperty(name);
    }
  });

  it("groups widgets into categories", () => {
    const cats = config.categories ?? {};
    expect(cats["summary"]?.components).toContain("SourceHeader");
    expect(cats["metrics"]?.components).toContain("MetricCard");
    expect(cats["metrics"]?.components).toContain("MetricChart");
    expect(cats["lists"]?.components).toContain("ServiceList");
    expect(cats["lists"]?.components).toContain("ActivityTail");
  });

  it("every component has defaultProps and a render fn", () => {
    for (const [name, comp] of Object.entries(config.components)) {
      const c = comp as { defaultProps?: unknown; render?: unknown };
      expect(c.defaultProps, `${name}.defaultProps`).toBeDefined();
      expect(typeof c.render, `${name}.render`).toBe("function");
    }
  });
});

describe("createDefaultAdminLayout", () => {
  it("returns a non-empty Puck Data object", () => {
    const data = createDefaultAdminLayout();
    expect(data.root).toBeDefined();
    expect(Array.isArray(data.content)).toBe(true);
    expect(data.content.length).toBeGreaterThan(0);
  });

  it("references only registered components", () => {
    const data = createDefaultAdminLayout();
    const known = new Set(Object.keys(createAdminPuckConfig().components));
    for (const node of data.content) {
      expect(known.has(node.type)).toBe(true);
    }
  });

  it("assigns unique ids to each node", () => {
    const data = createDefaultAdminLayout();
    const ids = data.content.map((n) => (n.props as { id?: string }).id ?? "");
    expect(new Set(ids).size).toBe(ids.length);
  });
});
