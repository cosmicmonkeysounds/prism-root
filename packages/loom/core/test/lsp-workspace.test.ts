//! Integration parity for the ported `loom-lsp` `Workspace` — index
//! build + every request entry point against one clean document.

import { beforeEach, describe, expect, it } from "vitest";
import { CompletionItemKind, SymbolKind, Workspace } from "../src/lsp/index.ts";

const URI = "inmemory://test.loom";

// Built line-by-line so the asserted ranges are unambiguous and so the
// ```todo fence backticks don't collide with a JS template literal.
const LINES = [
  "CHARACTER WREN is Keeper", // 0
  "  voice: low", //            1
  "", //                        2
  "TRAIT Keeper", //            3
  "", //                        4
  "== opening", //              5
  "  cast: WREN", //            6
  "  setting: Lighthouse", //   7
  "", //                        8
  "A bell rope swings.", //     9
  "", //                       10
  "<anchor: arrival>", //      11
  "", //                       12
  "* Ring the bell.", //       13
  "  -> ringing", //           14
  "", //                       15
  "== ringing", //             16
  "  setting: Belfry", //      17
  "", //                       18
  "```todo", //                19
  "Wire the bell SFX.", //     20
  "```", //                    21
  "", //                       22
];
const SRC = LINES.join("\n") + "\n";

let ws: Workspace;
beforeEach(() => {
  ws = new Workspace();
  ws.open(URI, SRC);
});

describe("Workspace index", () => {
  it("parses the document clean", () => {
    const params = ws.diagnosticsFor(URI);
    expect(params).not.toBeNull();
    expect(params!.uri).toBe(URI);
    expect(params!.diagnostics).toEqual([]);
  });

  it("indexes characters, traits, and beats", () => {
    expect([...ws.characters.keys()]).toEqual(["WREN"]);
    expect(ws.characters.get("WREN")!.mixins).toEqual(["Keeper"]);
    expect([...ws.traits.keys()]).toEqual(["Keeper"]);
    expect([...ws.beats.keys()].sort()).toEqual(["opening", "ringing"]);
  });

  it("indexes anchors by name", () => {
    const arrival = ws.anchors.get("arrival");
    expect(arrival).toBeDefined();
    expect(arrival!).toHaveLength(1);
    expect(arrival![0]!.range.start.line).toBe(11);
  });

  it("indexes todo fences off the fence tail", () => {
    expect(ws.todos).toHaveLength(1);
    expect(ws.todos[0]!.text).toBe("Wire the bell SFX.");
  });

  it("drops a document on close", () => {
    ws.close(URI);
    expect(ws.diagnosticsFor(URI)).toBeNull();
    expect([...ws.beats.keys()]).toEqual([]);
  });
});

describe("Workspace.documentSymbols", () => {
  it("returns one flat symbol per top-level item", () => {
    const syms = ws.documentSymbols(URI);
    expect(syms).not.toBeNull();
    const byName = new Map(syms!.map((s) => [s.name, s.kind]));
    expect(byName.get("WREN")).toBe(SymbolKind.Class);
    expect(byName.get("Keeper")).toBe(SymbolKind.Interface);
    expect(byName.get("opening")).toBe(SymbolKind.Function);
    expect(byName.get("ringing")).toBe(SymbolKind.Function);
  });
});

describe("Workspace.completionAt", () => {
  it("offers beats after a divert arrow", () => {
    const items = ws.completionAt(URI, { line: 14, character: 5 });
    expect(items.map((i) => i.label).sort()).toEqual(["opening", "ringing"]);
    expect(items.every((i) => i.kind === CompletionItemKind.Function)).toBe(true);
  });

  it("offers directive verbs inside an open <…>", () => {
    const items = ws.completionAt(URI, { line: 11, character: 3 });
    const labels = items.map((i) => i.label);
    expect(labels).toContain("anchor");
    expect(labels).toContain("sfx");
    expect(items.every((i) => i.kind === CompletionItemKind.Keyword)).toBe(true);
  });

  it("offers characters + traits after `is`", () => {
    const items = ws.completionAt(URI, { line: 0, character: 18 });
    const labels = items.map((i) => i.label);
    expect(labels).toContain("WREN");
    expect(labels).toContain("Keeper");
  });

  it("offers nothing in plain prose", () => {
    expect(ws.completionAt(URI, { line: 9, character: 5 })).toEqual([]);
  });
});

describe("Workspace.hoverAt", () => {
  it("describes a beat under a divert", () => {
    const hover = ws.hoverAt(URI, { line: 14, character: 7 });
    expect(hover).not.toBeNull();
    expect(hover!.contents.kind).toBe("markdown");
    expect(hover!.contents.value).toContain("ringing");
    expect(hover!.contents.value).toContain("Belfry");
  });

  it("describes a CHARACTER reference", () => {
    const hover = ws.hoverAt(URI, { line: 6, character: 9 });
    expect(hover).not.toBeNull();
    expect(hover!.contents.value).toContain("CHARACTER");
    expect(hover!.contents.value).toContain("is Keeper");
    expect(hover!.contents.value).toContain("voice: low");
  });

  it("returns null on plain prose", () => {
    expect(ws.hoverAt(URI, { line: 9, character: 2 })).toBeNull();
  });
});

describe("Workspace.definitionAt", () => {
  it("jumps a divert to the beat declaration", () => {
    const def = ws.definitionAt(URI, { line: 14, character: 7 });
    expect(Array.isArray(def)).toBe(true);
    const locs = def as Array<{ uri: string; range: { start: { line: number } } }>;
    expect(locs).toHaveLength(1);
    expect(locs[0]!.uri).toBe(URI);
    expect(locs[0]!.range.start.line).toBe(16);
  });

  it("jumps a CHARACTER cue to its declaration", () => {
    const def = ws.definitionAt(URI, { line: 6, character: 9 });
    expect(Array.isArray(def)).toBe(false);
    const loc = def as { uri: string; range: { start: { line: number } } };
    expect(loc.uri).toBe(URI);
    expect(loc.range.start.line).toBe(0);
  });
});

describe("Workspace references", () => {
  it("finds every occurrence under the cursor", () => {
    const hits = ws.referencesAt(URI, { line: 6, character: 9 });
    expect(hits).toHaveLength(2);
    expect(hits.map((h) => h.range.start.line).sort((a, b) => a - b)).toEqual([0, 6]);
  });

  it("finds occurrences by bare name", () => {
    expect(ws.referencesByName("WREN")).toHaveLength(2);
    expect(ws.referencesByName("Keeper")).toHaveLength(2);
    expect(ws.referencesByName("nope")).toHaveLength(0);
  });
});
