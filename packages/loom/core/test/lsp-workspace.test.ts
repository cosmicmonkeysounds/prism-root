//! Integration parity for the ported `loom-lsp` `Workspace` — index
//! build + every request entry point against one clean document.

import { beforeEach, describe, expect, it } from "vitest";
import { CompletionItemKind, DiagnosticSeverity, SymbolKind, Workspace } from "../src/lsp/index.ts";

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
    const loc = def as { uri: string; range: { start: { line: number; character: number } } };
    expect(loc.uri).toBe(URI);
    expect(loc.range.start.line).toBe(0);
    // Tight name range: lands on `WREN` (col 10), not the block head (col 0).
    expect(loc.range.start.character).toBe(10);
  });

  it("jumps a TRAIT mixin reference to the trait declaration", () => {
    // Cursor on `Keeper` in `CHARACTER WREN is Keeper` (line 0).
    const def = ws.definitionAt(URI, { line: 0, character: 20 });
    expect(Array.isArray(def)).toBe(false);
    const loc = def as { uri: string; range: { start: { line: number } } };
    expect(loc.uri).toBe(URI);
    expect(loc.range.start.line).toBe(3); // `TRAIT Keeper`
  });

  it("resolves an anchor name to its <anchor:> site", () => {
    // Cursor on `arrival` inside `<anchor: arrival>` (line 11).
    const def = ws.definitionAt(URI, { line: 11, character: 10 });
    const locs = Array.isArray(def) ? def : def ? [def] : [];
    expect(locs).toHaveLength(1);
    expect(locs[0]!.range.start.line).toBe(11);
  });
});

describe("Workspace.updateMany", () => {
  it("indexes many documents with a single rebuild", () => {
    const w = new Workspace();
    w.updateMany([
      ["inmemory://a.loom", "== alpha\n  cast: X\n"],
      ["inmemory://b.loom", "== beta\n  cast: Y\n"],
    ]);
    expect([...w.beats.keys()].sort()).toEqual(["alpha", "beta"]);
    expect(w.diagnosticsFor("inmemory://a.loom")).not.toBeNull();
    expect(w.diagnosticsFor("inmemory://b.loom")).not.toBeNull();
  });

  it("is a no-op on an empty batch", () => {
    const w = new Workspace();
    w.open(URI, SRC);
    const before = [...w.beats.keys()].sort();
    w.updateMany([]);
    expect([...w.beats.keys()].sort()).toEqual(before);
  });
});

describe("Workspace.reset", () => {
  it("drops every document and clears the index", () => {
    const w = new Workspace();
    w.open("inmemory://a.loom", "== alpha\n");
    w.open("inmemory://b.loom", "== beta\n");
    expect([...w.beats.keys()].sort()).toEqual(["alpha", "beta"]);
    w.reset();
    expect([...w.beats.keys()]).toEqual([]);
    expect(w.diagnosticsFor("inmemory://a.loom")).toBeNull();
  });
});

describe("nameRange token-boundary (definitionAt)", () => {
  it("resolves a name to the whole-word site, not a keyword substring", () => {
    // `OLE` is a substring of the leading `ROLE` keyword — a raw indexOf
    // would land the definition inside `ROLE` (char 1). Token-boundary
    // matching must land on the real name at char 5.
    const w = new Workspace();
    const uri = "inmemory://ole.loom";
    w.open(uri, "ROLE OLE\n  voice: low\n== b\n  cast: OLE\n");
    const def = w.definitionAt(uri, { line: 3, character: 9 }); // on `OLE` in cast
    const loc = Array.isArray(def) ? def[0]! : def!;
    expect(loc.range.start.line).toBe(0);
    expect(loc.range.start.character).toBe(5);
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

// ── Redesign LSP surface: parameterized traits, class-owned beats, ──────
// derived templates, and cross-file project diagnostics. ────────────────

const REDESIGN_URI = "inmemory://redesign.loom";
function openRedesign(...ls: string[]): Workspace {
  const w = new Workspace();
  w.open(REDESIGN_URI, ls.join("\n") + "\n");
  return w;
}

describe("Workspace trait index", () => {
  it("records a trait's params and shipped beats", () => {
    const w = openRedesign(
      "TRAIT Scanner(beat)", //     0
      "  on scan guest", //         1
      "    -> self.beat", //        2
      "", //                        3
      "TRAIT Gatekeeper", //        4
      "  beat confront(guest)", //  5
      "    SELF", //                6
      "      Hi.", //               7
    );
    const scanner = w.traits.get("Scanner")!;
    expect(scanner.params).toEqual(["beat"]);
    expect(scanner.beats).toEqual([]);
    const gate = w.traits.get("Gatekeeper")!;
    expect(gate.params).toEqual([]);
    expect(gate.beats).toEqual(["confront"]);
    // `keys()` still works for completion.
    expect([...w.traits.keys()].sort()).toEqual(["Gatekeeper", "Scanner"]);
  });
});

describe("Workspace.hoverAt — traits", () => {
  it("describes a parameterized trait", () => {
    const w = openRedesign("TRAIT Scanner(beat)", "  on scan guest", "    -> self.beat");
    const hover = w.hoverAt(REDESIGN_URI, { line: 0, character: 9 });
    expect(hover).not.toBeNull();
    expect(hover!.contents.value).toContain("TRAIT");
    expect(hover!.contents.value).toContain("Scanner(beat)");
  });

  it("lists the beats a trait ships", () => {
    const w = openRedesign(
      "TRAIT Gatekeeper",
      "  beat confront(guest)",
      "    SELF",
      "      Hi.",
    );
    const hover = w.hoverAt(REDESIGN_URI, { line: 0, character: 10 });
    expect(hover!.contents.value).toContain("ships: confront");
  });
});

describe("Workspace.completionAt — trait args + qualified diverts", () => {
  it("completes beats inside `is Trait(` args", () => {
    const w = openRedesign(
      "== crawler_report(guest)", //         0
      "  cast: Crawler", //                  1
      "  NARRATOR", //                       2
      "    Hi.", //                          3
      "", //                                 4
      "TRAIT Scanner(beat)", //              5
      "  on scan guest", //                  6
      "    -> self.beat", //                 7
      "", //                                 8
      "CHARACTER Crawler is Scanner(", //    9
    );
    // Cursor right after the `(` on line 9.
    const items = w.completionAt(REDESIGN_URI, { line: 9, character: 29 });
    expect(items.map((i) => i.label)).toContain("crawler_report");
    expect(items.every((i) => i.kind === CompletionItemKind.Function)).toBe(true);
  });

  it("completes owned + inherited beats after `-> self.`", () => {
    const w = openRedesign(
      "TRAIT Router(beat)", //        0
      "  beat relay(guest)", //       1
      "    SELF", //                  2
      "      Relayed.", //            3
      "", //                         4
      "CHARACTER Alpha is Router", //  5
      "  faction: F", //             6
      "  on scan guest", //          7
      "    -> self.report", //       8
      "  beat report(guest)", //     9
      "    SELF", //                 10
      "      Alpha here.", //        11
    );
    // Cursor after `-> self.` on line 8 (col 12).
    const items = w.completionAt(REDESIGN_URI, { line: 8, character: 12 });
    const labels = items.map((i) => i.label).sort();
    // Own beat `report` + trait-shipped `relay`.
    expect(labels).toEqual(["relay", "report"]);
  });

  it("does not resolve `-> self.` on a top-level beat to a preceding character", () => {
    const w = openRedesign(
      "CHARACTER Wren", //        0
      "  faction: F", //          1
      "  beat greet(x)", //       2
      "    SELF", //              3
      "      Hi.", //             4
      "", //                      5
      "== toplevel(guest)", //    6
      "  -> self.", //            7
    );
    // Cursor after `-> self.` on line 7 — NOT inside Wren's body, so `self`
    // resolves to nothing and no beats are offered (would leak `greet` before).
    expect(w.completionAt(REDESIGN_URI, { line: 7, character: 10 })).toEqual([]);
  });

  it("completes an owner's beats after `-> Owner.`", () => {
    const w = openRedesign(
      "CHARACTER Alpha", //          0
      "  faction: F", //            1
      "  beat report(guest)", //    2
      "    SELF", //                3
      "      Alpha here.", //       4
      "", //                        5
      "CHARACTER Caller", //        6
      "  faction: F", //            7
      "  on scan guest", //         8
      "    -> Alpha.report", //     9
    );
    // Cursor after `-> Alpha.` on line 9 (col 13).
    const items = w.completionAt(REDESIGN_URI, { line: 9, character: 13 });
    expect(items.map((i) => i.label)).toEqual(["report"]);
  });

  it("offers SELF and ME alongside characters after `is`", () => {
    const w = openRedesign("TRAIT Keeper", "", "CHARACTER WREN is ");
    const items = w.completionAt(REDESIGN_URI, { line: 2, character: 18 });
    const labels = items.map((i) => i.label);
    expect(labels).toContain("SELF");
    expect(labels).toContain("ME");
    expect(labels).toContain("Keeper");
  });
});

describe("Workspace project diagnostics", () => {
  function diagsOf(w: Workspace): { message: string; severity?: number }[] {
    return w.diagnosticsFor(REDESIGN_URI)!.diagnostics;
  }

  it("reports an unfilled required trait param on the applier", () => {
    const w = openRedesign(
      "TRAIT Scanner(beat)",
      "  on scan guest",
      "    -> self.beat",
      "",
      "CHARACTER Crawler is Scanner",
    );
    const msgs = diagsOf(w).map((d) => d.message);
    expect(msgs.some((m) => m.includes("requires an argument for `beat`"))).toBe(true);
    // It lands on Crawler's declaration line, not the file head.
    const diag = diagsOf(w).find((d) => d.message.includes("requires an argument"))!;
    expect(w.diagnosticsFor(REDESIGN_URI)!.diagnostics).toContain(diag);
  });

  it("reports a trait arg that names no beat", () => {
    const w = openRedesign(
      "TRAIT Scanner(beat)",
      "  on scan guest",
      "    -> self.beat",
      "",
      "CHARACTER Crawler is Scanner(nonexistent_beat)",
    );
    const msgs = diagsOf(w).map((d) => d.message);
    expect(msgs.some((m) => m.includes("`nonexistent_beat`") && m.includes("names no beat"))).toBe(
      true,
    );
  });

  it("flags two traits shipping the same beat name as a conflict", () => {
    const w = openRedesign(
      "TRAIT A",
      "  beat x(g)",
      "    SELF",
      "      From A.",
      "",
      "TRAIT B",
      "  beat x(g)",
      "    SELF",
      "      From B.",
      "",
      "CHARACTER C is A, B",
    );
    const msgs = diagsOf(w).map((d) => d.message);
    expect(msgs.some((m) => m.includes("both ship a beat named `x`"))).toBe(true);
  });

  it("warns (not errors) on an unfilled derived slot", () => {
    const w = openRedesign(
      "FACTION F",
      "  ethos: order",
      "",
      "ROLE Guest",
      "  faction: any of FACTION",
      "",
      "TRAIT Gate",
      "  beat g(guest)",
      "    SELF",
      "      slot: line",
      "",
      "CHARACTER Lonely is Gate",
      "  faction: F",
      "  on scan guest",
      "    -> self.g",
    );
    const slotDiag = diagsOf(w).find((d) => d.message.includes("unfilled slot `line`"));
    expect(slotDiag).toBeDefined();
    expect(slotDiag!.severity).toBe(DiagnosticSeverity.Warning);
  });

  it("reports an unfilled required slot on an abstract CHARACTER", () => {
    const w = openRedesign("CHARACTER Keeper", "  voice: any");
    const msgs = diagsOf(w).map((d) => d.message);
    expect(msgs.some((m) => m.includes("required slot `voice`"))).toBe(true);
    // Errors by default.
    const diag = diagsOf(w).find((d) => d.message.includes("required slot `voice`"))!;
    expect(diag.severity).toBe(DiagnosticSeverity.Error);
  });

  it("clears project diagnostics once the source is fixed", () => {
    const w = openRedesign("CHARACTER Keeper", "  voice: any");
    expect(diagsOf(w).some((d) => d.message.includes("required slot"))).toBe(true);
    w.update(REDESIGN_URI, "CHARACTER Keeper\n  voice: low\n");
    expect(diagsOf(w).some((d) => d.message.includes("required slot"))).toBe(false);
  });
});
