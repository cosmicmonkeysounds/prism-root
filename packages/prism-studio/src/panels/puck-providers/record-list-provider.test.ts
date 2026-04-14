import { describe, expect, it } from "vitest";
import {
  parseFilterExpression,
  parseMetaFields,
  buildTemplate,
  buildViewConfig,
} from "./record-list-provider.js";

describe("parseFilterExpression", () => {
  it("returns [] for empty or whitespace input", () => {
    expect(parseFilterExpression("")).toEqual([]);
    expect(parseFilterExpression("   ")).toEqual([]);
  });

  it("parses a single clause", () => {
    expect(parseFilterExpression("status eq open")).toEqual([
      { field: "status", op: "eq", value: "open" },
    ]);
  });

  it("parses multiple semicolon-separated clauses", () => {
    const result = parseFilterExpression("status eq open; priority eq high");
    expect(result).toEqual([
      { field: "status", op: "eq", value: "open" },
      { field: "priority", op: "eq", value: "high" },
    ]);
  });

  it("comma-splits the value for `in` and `nin`", () => {
    expect(parseFilterExpression("priority in high,urgent")).toEqual([
      { field: "priority", op: "in", value: ["high", "urgent"] },
    ]);
    expect(parseFilterExpression("status nin done,archived")).toEqual([
      { field: "status", op: "nin", value: ["done", "archived"] },
    ]);
  });

  it("omits the value for `empty` and `notempty`", () => {
    expect(parseFilterExpression("description empty")).toEqual([
      { field: "description", op: "empty" },
    ]);
    expect(parseFilterExpression("tags notempty")).toEqual([
      { field: "tags", op: "notempty" },
    ]);
  });

  it("falls back to `contains` when the operator is unknown", () => {
    expect(parseFilterExpression("name weird foo")).toEqual([
      { field: "name", op: "contains", value: "foo" },
    ]);
  });

  it("handles multi-word values by joining them", () => {
    expect(parseFilterExpression("name contains buy milk")).toEqual([
      { field: "name", op: "contains", value: "buy milk" },
    ]);
  });

  it("drops clauses missing a field or value", () => {
    expect(parseFilterExpression("status eq")).toEqual([]);
    expect(parseFilterExpression("  ; status eq open ;  ")).toEqual([
      { field: "status", op: "eq", value: "open" },
    ]);
  });
});

describe("parseMetaFields", () => {
  it("returns [] for empty input", () => {
    expect(parseMetaFields("")).toEqual([]);
    expect(parseMetaFields("   ")).toEqual([]);
  });

  it("parses field:kind pairs", () => {
    expect(parseMetaFields("status:badge, date:date")).toEqual([
      { field: "status", kind: "badge" },
      { field: "date", kind: "date" },
    ]);
  });

  it("defaults unknown kinds to text", () => {
    expect(parseMetaFields("priority:weird")).toEqual([
      { field: "priority", kind: "text" },
    ]);
  });

  it("defaults missing kind to text", () => {
    expect(parseMetaFields("tags")).toEqual([
      { field: "tags", kind: "text" },
    ]);
  });

  it("drops entries without a field", () => {
    expect(parseMetaFields(":badge, status:badge")).toEqual([
      { field: "status", kind: "badge" },
    ]);
  });
});

describe("buildTemplate", () => {
  it("falls back to name for title when titleField is missing", () => {
    expect(buildTemplate({})).toEqual({ title: { field: "name" } });
  });

  it("honours explicit title, subtitle, and meta fields", () => {
    const t = buildTemplate({
      titleField: "summary",
      subtitleField: "location",
      metaFields: "date:date, status:badge",
    });
    expect(t).toEqual({
      title: { field: "summary" },
      subtitle: { field: "location" },
      meta: [
        { field: "date", kind: "date" },
        { field: "status", kind: "badge" },
      ],
    });
  });

  it("omits subtitle and meta when absent", () => {
    expect(buildTemplate({ titleField: "name" })).toEqual({
      title: { field: "name" },
    });
  });
});

describe("buildViewConfig", () => {
  it("returns empty config when nothing is set", () => {
    expect(buildViewConfig({})).toEqual({});
  });

  it("includes filters parsed from the expression", () => {
    const cfg = buildViewConfig({ filterExpression: "status eq open" });
    expect(cfg.filters).toEqual([
      { field: "status", op: "eq", value: "open" },
    ]);
  });

  it("defaults sort direction to desc", () => {
    const cfg = buildViewConfig({ sortField: "updatedAt" });
    expect(cfg.sorts).toEqual([{ field: "updatedAt", dir: "desc" }]);
  });

  it("respects an explicit ascending sort", () => {
    const cfg = buildViewConfig({ sortField: "name", sortDir: "asc" });
    expect(cfg.sorts).toEqual([{ field: "name", dir: "asc" }]);
  });

  it("includes a positive limit", () => {
    expect(buildViewConfig({ limit: 25 }).limit).toBe(25);
  });

  it("omits non-positive or missing limits", () => {
    expect(buildViewConfig({ limit: 0 }).limit).toBeUndefined();
    expect(buildViewConfig({}).limit).toBeUndefined();
  });

  it("composes all three pieces", () => {
    const cfg = buildViewConfig({
      filterExpression: "status eq open; priority in high,urgent",
      sortField: "date",
      sortDir: "asc",
      limit: 10,
    });
    expect(cfg).toEqual({
      filters: [
        { field: "status", op: "eq", value: "open" },
        { field: "priority", op: "in", value: ["high", "urgent"] },
      ],
      sorts: [{ field: "date", dir: "asc" }],
      limit: 10,
    });
  });
});
