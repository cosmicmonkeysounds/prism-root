/**
 * Tests for CsvSurface helpers — parseCsv, serializeCsv, detectDelimiter.
 */

import { describe, it, expect } from "vitest";
import { parseCsv, serializeCsv, detectDelimiter } from "./csv-surface.js";

describe("detectDelimiter", () => {
  it("detects comma delimiter by default", () => {
    expect(detectDelimiter("a,b,c\n1,2,3")).toBe(",");
  });

  it("detects tab when present in first line", () => {
    expect(detectDelimiter("a\tb\tc\n1\t2\t3")).toBe("\t");
  });

  it("defaults to comma for empty input", () => {
    expect(detectDelimiter("")).toBe(",");
  });

  it("only looks at first line", () => {
    expect(detectDelimiter("a,b,c\nx\ty\tz")).toBe(",");
  });
});

describe("parseCsv", () => {
  it("parses a basic CSV", () => {
    const rows = parseCsv("a,b,c\n1,2,3");
    expect(rows).toEqual([
      ["a", "b", "c"],
      ["1", "2", "3"],
    ]);
  });

  it("handles trailing newlines", () => {
    const rows = parseCsv("a,b\n1,2\n");
    expect(rows).toEqual([
      ["a", "b"],
      ["1", "2"],
    ]);
  });

  it("handles CRLF line endings", () => {
    const rows = parseCsv("a,b\r\n1,2");
    expect(rows).toEqual([
      ["a", "b"],
      ["1", "2"],
    ]);
  });

  it("handles quoted fields with commas", () => {
    const rows = parseCsv('name,note\n"Smith, J.",hello');
    expect(rows).toEqual([
      ["name", "note"],
      ["Smith, J.", "hello"],
    ]);
  });

  it("handles quoted fields with escaped quotes", () => {
    const rows = parseCsv('a\n"He said ""hi"""');
    expect(rows).toEqual([["a"], ['He said "hi"']]);
  });

  it("handles quoted fields with newlines", () => {
    const rows = parseCsv('a,b\n"line1\nline2",x');
    expect(rows).toEqual([
      ["a", "b"],
      ["line1\nline2", "x"],
    ]);
  });

  it("handles empty cells", () => {
    const rows = parseCsv("a,b,c\n1,,3");
    expect(rows).toEqual([
      ["a", "b", "c"],
      ["1", "", "3"],
    ]);
  });

  it("parses TSV with tab delimiter", () => {
    const rows = parseCsv("a\tb\n1\t2", "\t");
    expect(rows).toEqual([
      ["a", "b"],
      ["1", "2"],
    ]);
  });

  it("returns empty array for empty input", () => {
    expect(parseCsv("")).toEqual([]);
  });
});

describe("serializeCsv", () => {
  it("serialises basic rows", () => {
    expect(
      serializeCsv([
        ["a", "b"],
        ["1", "2"],
      ]),
    ).toBe("a,b\n1,2");
  });

  it("quotes cells containing commas", () => {
    expect(serializeCsv([["Smith, J.", "x"]])).toBe('"Smith, J.",x');
  });

  it("escapes quotes inside quoted cells", () => {
    expect(serializeCsv([['He said "hi"']])).toBe('"He said ""hi"""');
  });

  it("quotes cells containing newlines", () => {
    expect(serializeCsv([["line1\nline2", "x"]])).toBe('"line1\nline2",x');
  });

  it("serialises TSV with tab delimiter", () => {
    expect(
      serializeCsv(
        [
          ["a", "b"],
          ["1", "2"],
        ],
        "\t",
      ),
    ).toBe("a\tb\n1\t2");
  });

  it("round-trips through parseCsv", () => {
    const original = 'name,note\n"Smith, J.","He said ""hi"""\nAlice,hello';
    const parsed = parseCsv(original);
    expect(serializeCsv(parsed)).toBe(original);
  });
});
