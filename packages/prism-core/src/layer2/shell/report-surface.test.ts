/**
 * Tests for ReportSurface pure helpers.
 */

import { describe, it, expect } from "vitest";
import {
  extractRecords,
  groupRecords,
  renderReportBody,
} from "./report-surface.js";

describe("extractRecords", () => {
  it("parses a JSON array", () => {
    const records = extractRecords('[{"a":1},{"a":2}]');
    expect(records).toEqual([{ a: 1 }, { a: 2 }]);
  });

  it("extracts records from YAML with a records key", () => {
    const yaml = 'records: [{"name":"Alice"}, {"name":"Bob"}]';
    const records = extractRecords(yaml);
    expect(records).toHaveLength(2);
    expect(records[0]?.name).toBe("Alice");
  });

  it("returns empty array for empty input", () => {
    expect(extractRecords("")).toEqual([]);
    expect(extractRecords("   ")).toEqual([]);
  });

  it("returns empty array when JSON parse fails", () => {
    expect(extractRecords("[not json")).toEqual([]);
  });
});

describe("groupRecords", () => {
  const records = [
    { name: "Alice", dept: "eng", salary: 100 },
    { name: "Bob", dept: "eng", salary: 120 },
    { name: "Carol", dept: "sales", salary: 90 },
  ];

  it("returns single 'All Records' group when groupBy is omitted", () => {
    const groups = groupRecords(records, undefined, undefined);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.key).toBe("All Records");
    expect(groups[0]?.count).toBe(3);
  });

  it("groups by specified field", () => {
    const groups = groupRecords(records, "dept", undefined);
    expect(groups).toHaveLength(2);
    const eng = groups.find((g) => g.key === "eng");
    expect(eng?.count).toBe(2);
  });

  it("computes sum/avg/min/max for summary field", () => {
    const groups = groupRecords(records, "dept", "salary");
    const eng = groups.find((g) => g.key === "eng");
    expect(eng?.sum).toBe(220);
    expect(eng?.avg).toBe(110);
    expect(eng?.min).toBe(100);
    expect(eng?.max).toBe(120);
  });

  it("uses '(none)' for null/empty group values", () => {
    const groups = groupRecords(
      [{ name: "Alice", dept: null }, { name: "Bob", dept: "" }],
      "dept",
      undefined,
    );
    expect(groups).toHaveLength(1);
    expect(groups[0]?.key).toBe("(none)");
    expect(groups[0]?.count).toBe(2);
  });

  it("leaves summary stats null when no numeric values", () => {
    const groups = groupRecords(
      [{ dept: "eng", salary: "nope" }],
      "dept",
      "salary",
    );
    expect(groups[0]?.sum).toBe(null);
    expect(groups[0]?.avg).toBe(null);
  });
});

describe("renderReportBody", () => {
  const records = [
    { name: "Alice", dept: "eng", salary: 100 },
    { name: "Bob", dept: "eng", salary: 120 },
    { name: "Carol", dept: "sales", salary: 90 },
  ];

  it("includes the title when given", () => {
    const html = renderReportBody(records, { title: "Q1 Report" });
    expect(html).toContain('class="report-title"');
    expect(html).toContain("Q1 Report");
  });

  it("emits one group block per group", () => {
    const html = renderReportBody(records, { groupBy: "dept" });
    expect(html).toContain('class="report-group"');
    expect(html).toContain("eng");
    expect(html).toContain("sales");
  });

  it("includes grand total line", () => {
    const html = renderReportBody(records, { summaryField: "salary" });
    expect(html).toContain("Total records: 3");
    expect(html).toContain("Total salary: 310.00");
  });

  it("escapes HTML in record values", () => {
    const html = renderReportBody([{ name: "<script>" }], {});
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain("<script>");
  });
});
