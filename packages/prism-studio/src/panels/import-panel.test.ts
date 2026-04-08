import { describe, it, expect } from "vitest";
import {
  detectImportFormat,
  parseImportCsv,
  parseImportJson,
  mapRowsToObjects,
} from "./import-panel.js";

describe("detectImportFormat", () => {
  it("detects csv from filename", () => {
    expect(detectImportFormat("data.csv", "")).toBe("csv");
  });
  it("detects tsv from filename", () => {
    expect(detectImportFormat("data.tsv", "")).toBe("csv");
  });
  it("detects json from filename", () => {
    expect(detectImportFormat("data.json", "")).toBe("json");
  });
  it("falls back to content sniffing", () => {
    expect(detectImportFormat("unknown.txt", "[{")).toBe("json");
    expect(detectImportFormat("unknown.txt", "{\"a\":1}")).toBe("json");
    expect(detectImportFormat("unknown.txt", "name,age\nAlice,30")).toBe("csv");
  });
});

describe("parseImportCsv", () => {
  it("parses headers and rows", () => {
    const { header, rows } = parseImportCsv("name,age\nAlice,30\nBob,25");
    expect(header).toEqual(["name", "age"]);
    expect(rows).toEqual([
      ["Alice", "30"],
      ["Bob", "25"],
    ]);
  });

  it("handles quoted fields with commas", () => {
    const { rows } = parseImportCsv('name,note\nAlice,"hello, world"');
    expect(rows[0]).toEqual(["Alice", "hello, world"]);
  });

  it('handles escaped quotes ("")', () => {
    const { rows } = parseImportCsv('name,quote\nAlice,"say ""hi"""');
    expect(rows[0]).toEqual(["Alice", 'say "hi"']);
  });

  it("handles CRLF line endings", () => {
    const { header, rows } = parseImportCsv("a,b\r\n1,2\r\n3,4");
    expect(header).toEqual(["a", "b"]);
    expect(rows).toEqual([
      ["1", "2"],
      ["3", "4"],
    ]);
  });

  it("autodetects tab delimiter", () => {
    const { header } = parseImportCsv("a\tb\tc\n1\t2\t3");
    expect(header).toEqual(["a", "b", "c"]);
  });
});

describe("parseImportJson", () => {
  it("parses a plain array of objects", () => {
    const { header, rows } = parseImportJson('[{"name":"A","age":1},{"name":"B","age":2}]');
    expect(header.sort()).toEqual(["age", "name"]);
    expect(rows).toHaveLength(2);
  });

  it("accepts {records: [...]} envelope", () => {
    const { rows } = parseImportJson('{"records":[{"x":1}]}');
    expect(rows).toHaveLength(1);
  });

  it("throws for invalid shapes", () => {
    expect(() => parseImportJson('{"not":"an array"}')).toThrow();
  });

  it("unions keys across rows", () => {
    const { header } = parseImportJson('[{"a":1},{"b":2}]');
    expect(header.sort()).toEqual(["a", "b"]);
  });
});

describe("mapRowsToObjects", () => {
  it("applies column mapping", () => {
    const result = mapRowsToObjects(
      ["source_name", "source_age"],
      [["Alice", "30"]],
      { source_name: "name", source_age: "age" },
    );
    expect(result[0]).toEqual({ name: "Alice", age: "30" });
  });

  it("skips unmapped columns", () => {
    const result = mapRowsToObjects(
      ["a", "b", "c"],
      [["1", "2", "3"]],
      { a: "x", c: "z" },
    );
    expect(result[0]).toEqual({ x: "1", z: "3" });
  });

  it("produces empty object when mapping is empty", () => {
    const result = mapRowsToObjects(["a"], [["1"]], {});
    expect(result[0]).toEqual({});
  });
});
