//! Graph-editor edit ops (`parser/edit.ts` additions + `lsp/rename.ts`).

import { describe, expect, it } from "vitest";
import {
  EditError,
  appendBodyLines,
  appendChoice,
  appendDeclaration,
  appendDivert,
  applyEdits,
  insertBodyLines,
  parse,
  removeBodyItem,
  renameBeatDecl,
  replaceExact,
  retargetDivert,
} from "../src/parser/index.ts";
import { Workspace } from "../src/lsp/index.ts";

const STORY = `== opening
  cast: Wren

A bell rope swings.
* Climb the tower
  -> belfry
-> waiting

== waiting
  The fog rolls in.

== belfry
  -> END
`;

function edited(source: string, f: (file: ReturnType<typeof parse>[0]) => ReturnType<typeof appendDivert>): string {
  const [file] = parse(source);
  return applyEdits(source, f(file));
}

describe("replaceExact / retargetDivert", () => {
  it("splices when the anchor still matches", () => {
    const at = STORY.indexOf("belfry");
    const next = applyEdits(STORY, retargetDivert(STORY, at, at + 6, "belfry", "waiting"));
    expect(next).toContain("-> waiting\n-> waiting");
    // Round-trips through the parser.
    const [, diags] = parse(next);
    expect(diags).toEqual([]);
  });

  it("refuses a stale anchor", () => {
    expect(() => replaceExact(STORY, 0, 6, "belfry", "x")).toThrowError(EditError);
    try {
      replaceExact(STORY, 0, 6, "belfry", "x");
    } catch (e) {
      expect((e as EditError).code).toBe("staleAnchor");
    }
  });

  it("no-ops when old and new are identical", () => {
    const at = STORY.indexOf("belfry");
    expect(retargetDivert(STORY, at, at + 6, "belfry", "belfry")).toEqual([]);
  });
});

describe("appendDivert / appendChoice / appendBodyLines", () => {
  it("appends a divert as the beat's last line, matching indent", () => {
    const next = edited(STORY, (f) => appendDivert(STORY, f, "waiting", "belfry"));
    expect(next).toContain("  The fog rolls in.\n  -> belfry\n");
    const [file, diags] = parse(next);
    expect(diags).toEqual([]);
    const waiting = file.items.find((i) => i.kind === "beat" && i.value.name === "waiting");
    expect(waiting && waiting.kind === "beat" && waiting.value.body.at(-1)?.kind).toBe("divert");
  });

  it("appends under a bare opener with default indent", () => {
    const src = "== a\n\n== b\n  stuff\n";
    const [file] = parse(src);
    const next = applyEdits(src, appendDivert(src, file, "a", "b"));
    expect(next).toContain("== a\n  -> b\n");
  });

  it("appends a wired choice option", () => {
    const next = edited(STORY, (f) =>
      appendChoice(STORY, f, "waiting", "Ring anyway", { sticky: true, target: "belfry" }),
    );
    expect(next).toContain("  + Ring anyway\n    -> belfry\n");
    const [file] = parse(next);
    const waiting = file.items.find((i) => i.kind === "beat" && i.value.name === "waiting");
    const last = waiting?.kind === "beat" ? waiting.value.body.at(-1) : undefined;
    expect(last?.kind).toBe("choice");
  });

  it("appends raw body lines preserving relative indent", () => {
    const next = edited(STORY, (f) =>
      appendBodyLines(STORY, f, "belfry", ["<sfx: bell>", "The peal rolls out."]),
    );
    expect(next).toContain("  -> END\n  <sfx: bell>\n  The peal rolls out.\n");
  });
});

describe("removeBodyItem", () => {
  it("removes a body item's whole block including nested lines", () => {
    const [file] = parse(STORY);
    const opening = file.items.find((i) => i.kind === "beat" && i.value.name === "opening");
    const body = opening?.kind === "beat" ? opening.value.body : [];
    const choiceIdx = body.findIndex((i) => i.kind === "choice");
    const next = applyEdits(STORY, removeBodyItem(STORY, file, "opening", choiceIdx));
    expect(next).not.toContain("Climb the tower");
    expect(next).not.toContain("-> belfry\n-> waiting"); // nested divert went with it
    expect(next).toContain("-> waiting");
    const [, diags] = parse(next);
    expect(diags).toEqual([]);
  });

  it("throws itemNotFound out of range", () => {
    const [file] = parse(STORY);
    expect(() => removeBodyItem(STORY, file, "waiting", 9)).toThrowError(EditError);
  });
});

describe("renameBeatDecl", () => {
  it("renames only the opener line token", () => {
    const next = edited(STORY, (f) => renameBeatDecl(STORY, f, "waiting", "vigil"));
    expect(next).toContain("== vigil\n");
    // The divert reference is deliberately untouched at this layer.
    expect(next).toContain("-> waiting\n");
  });
});

describe("renameBeatEdits (workspace layer)", () => {
  const MAIN = `entry: opening

== opening
* Up
  -> cast/belfry#top
-> waiting

== waiting
  -> opening
`;
  const CAST = `CHARACTER Sexton
  on scan guest
    -> self.rounds
  beat rounds(guest)
    -> opening

== belfry
  -> opening
`;

  function ws(): Workspace {
    const w = new Workspace();
    w.updateMany([
      ["file:///main.loom", MAIN],
      ["file:///cast.loom", CAST],
    ]);
    return w;
  }

  function applyAll(w: Workspace, edits: Map<string, ReturnType<typeof appendDivert>>): Map<string, string> {
    const out = new Map<string, string>();
    for (const [uri, list] of edits) {
      out.set(uri, applyEdits(w.docs.get(uri)!.text, list));
    }
    return out;
  }

  it("renames the declaration, every cross-file reference, and entry:", () => {
    const w = ws();
    const next = applyAll(w, w.renameBeat("opening", "doors"));
    const main = next.get("file:///main.loom")!;
    const cast = next.get("file:///cast.loom")!;
    expect(main).toContain("entry: doors\n");
    expect(main).toContain("== doors\n");
    expect(main).toContain("  -> doors\n"); // waiting's reference
    expect(cast).toContain("    -> doors\n"); // both cast references
    expect(cast).not.toContain("-> opening");
    // Both docs still parse clean and the graph re-resolves.
    w.updateMany([...next.entries()]);
    const g = w.storyGraph();
    expect(g.entry).toBe("doors");
    expect(g.edges.some((e) => e.to === "doors" && e.from === "waiting")).toBe(true);
  });

  it("preserves qualifier separators and knots", () => {
    const w = ws();
    const next = applyAll(w, w.renameBeat("belfry", "tower"));
    const main = next.get("file:///main.loom")!;
    expect(main).toContain("-> cast/tower#top");
  });

  it("renames an owned beat and its self-qualified references", () => {
    const w = ws();
    const next = applyAll(w, w.renameBeat("Sexton.rounds", "patrol"));
    const cast = next.get("file:///cast.loom")!;
    expect(cast).toContain("beat patrol(guest)");
    expect(cast).toContain("-> self.patrol");
  });

  it("refuses a taken name", () => {
    const w = ws();
    expect(() => w.renameBeat("waiting", "belfry")).toThrowError(EditError);
    try {
      w.renameBeat("waiting", "belfry");
    } catch (e) {
      expect((e as EditError).code).toBe("nameTaken");
    }
  });

  it("refuses an invalid identifier", () => {
    const w = ws();
    expect(() => w.renameBeat("waiting", "not a name")).toThrowError(EditError);
  });
});

describe("insertBodyLines", () => {
  it("inserts before a top-level body item, matching its indent", () => {
    const src = "== opening\n  A bell rope swings.\n  -> waiting\n\n== waiting\n  fog\n";
    const next = edited(src, (f) => insertBodyLines(src, f, "opening", 1, ["The verger coughs."]));
    expect(next).toContain("  A bell rope swings.\n  The verger coughs.\n  -> waiting");
    const [, diags] = parse(next);
    expect(diags).toEqual([]);
  });

  it("appends when the index is past the end", () => {
    const src = "== opening\n  A bell rope swings.\n";
    const next = edited(src, (f) => insertBodyLines(src, f, "opening", 99, ["-> END"]));
    expect(next).toContain("A bell rope swings.\n  -> END");
  });

  it("inserts at the top of the body", () => {
    const next = edited(STORY, (f) => insertBodyLines(STORY, f, "waiting", 0, ["A first line."]));
    expect(next).toContain("== waiting\n  A first line.\n  The fog rolls in.");
  });

  it("refuses a negative index", () => {
    const [file] = parse(STORY);
    expect(() => insertBodyLines(STORY, file, "waiting", -1, ["x"])).toThrowError(EditError);
  });
});

describe("appendDeclaration", () => {
  it("appends a parsing declaration block with props", () => {
    const next = applyEdits(STORY, appendDeclaration(STORY, "CHARACTER", "Verger", { voice: "dry", faction: "Mods" }));
    expect(next).toContain("\n\nCHARACTER Verger\n  voice: dry\n  faction: Mods\n");
    const [file, diags] = parse(next);
    expect(diags).toEqual([]);
    expect(file.items.some((i) => i.kind === "declaration" && i.value.name === "Verger")).toBe(true);
  });

  it("starts a fresh file without leading separators", () => {
    const next = applyEdits("", appendDeclaration("", "LOCATION", "Belfry", {}));
    expect(next).toBe("LOCATION Belfry\n");
    const [, diags] = parse(next);
    expect(diags).toEqual([]);
  });
});
