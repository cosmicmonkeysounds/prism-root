//! Project story-graph extraction (`lsp/graph.ts`).

import { describe, expect, it } from "vitest";
import { Workspace } from "../src/lsp/index.ts";
import type { GraphEdge, StoryGraph } from "../src/lsp/index.ts";
import { scenarioFiles } from "../examples/load.ts";

/** Build a workspace from `[uri, text]` pairs and return its graph. */
function graphOf(files: Array<[string, string]>): StoryGraph {
  const ws = new Workspace();
  ws.updateMany(files);
  return ws.storyGraph();
}

const narrative = (g: StoryGraph): GraphEdge[] => g.edges.filter((e) => e.narrative);
const between = (g: StoryGraph, from: string, to: string | null): GraphEdge[] =>
  g.edges.filter((e) => e.from === from && e.to === to);

const MAIN = `entry: opening

== opening
  cast: Wren, Sexton
  setting: Lighthouse

A bell rope swings.
* Climb the tower
  <set: courage += 1>
  -> cast/belfry
* [Stay put] Wait below
  -> waiting
+ Look around
  -> opening

== waiting
  <if: courage > 0>
    -> cast/belfry
  <else>
    -> END
`;

const CAST = `LOCATION Lighthouse
  contains: Belfry

CHARACTER Wren is Ringer(belfry)
  home: Lighthouse

TRAIT Ringer(chime)
  on scan guest
    -> self.chime

CHARACTER Sexton
  on scan guest
    -> self.rounds
  beat rounds(guest)
    SELF
      Keys jangle. Nothing escapes the Sexton.
    (belfry) ->
    -> missing_beat

== belfry
  The bells hang silent.
  -> END
`;

describe("buildStoryGraph — nodes", () => {
  const g = graphOf([
    ["file:///main.loom", MAIN],
    ["file:///cast.loom", CAST],
  ]);

  it("indexes top-level beats across every file", () => {
    expect(g.beats.get("opening")?.structural).toBe("file");
    expect(g.beats.get("waiting")?.structural).toBe("file");
    expect(g.beats.get("belfry")?.uri).toBe("file:///cast.loom");
  });

  it("marks the resolved entry beat", () => {
    expect(g.entry).toBe("opening");
    expect(g.beats.get("opening")?.entry).toBe(true);
    expect(g.beats.get("waiting")?.entry).toBe(false);
  });

  it("registers class-owned beats under Owner.name", () => {
    const rounds = g.beats.get("Sexton.rounds");
    expect(rounds?.structural).toBe("owned");
    expect(rounds?.owner).toBe("Sexton");
    expect(rounds?.uri).toBe("file:///cast.loom");
    expect(rounds?.span).not.toBeNull();
  });

  it("carries cast / setting / counts / preview on the node card", () => {
    const opening = g.beats.get("opening")!;
    expect(opening.cast).toEqual(["Wren", "Sexton"]);
    expect(opening.setting).toBe("Lighthouse");
    expect(opening.counts.choices).toBe(3);
    expect(opening.preview.length).toBeGreaterThan(0);
    expect(opening.preview[0]).toContain("bell rope");
  });

  it("lists entities with their file grouping", () => {
    expect(g.entities.get("character:Wren")).toBeDefined();
    expect(g.entities.get("trait:Ringer")?.uri).toBe("file:///cast.loom");
    expect(g.entities.get("location:Lighthouse")).toBeDefined();
    const castFile = g.files.find((f) => f.uri === "file:///cast.loom")!;
    expect(castFile.entities).toContain("character:Sexton");
    expect(castFile.beats).toContain("Sexton.rounds");
    expect(castFile.beats).toContain("belfry");
  });
});

describe("buildStoryGraph — narrative edges", () => {
  const g = graphOf([
    ["file:///main.loom", MAIN],
    ["file:///cast.loom", CAST],
  ]);

  it("resolves cross-file choice diverts with labels + stickiness", () => {
    const toBelfry = between(g, "opening", "belfry").find((e) => e.kind === "choice");
    expect(toBelfry).toBeDefined();
    expect(toBelfry!.label).toBe("Climb the tower");
    expect(toBelfry!.sticky).toBe(false);
    const loop = between(g, "opening", "opening").find((e) => e.kind === "choice");
    expect(loop?.sticky).toBe(true);
  });

  it("carries conditional guard context on edges", () => {
    const guarded = between(g, "waiting", "belfry")[0];
    expect(guarded?.condition).toBe("if courage > 0");
    const end = narrative(g).find((e) => e.from === "waiting" && e.kind === "end");
    expect(end?.condition).toBe("else");
  });

  it("emits END edges and flags hasEnd", () => {
    expect(g.hasEnd).toBe(true);
    expect(narrative(g).some((e) => e.from === "belfry" && e.kind === "end")).toBe(true);
  });

  it("resolves -> self.beat inside an owned beat and hooks", () => {
    // Sexton's authored hook routes to its own owned beat.
    const hook = between(g, "character:Sexton", "Sexton.rounds").find((e) => e.kind === "hook");
    expect(hook).toBeDefined();
    expect(hook!.label).toBe("on scan guest");
    expect(hook!.uri).toBe("file:///cast.loom");
    expect(hook!.span).not.toBeNull();
  });

  it("routes a trait-inherited hook through param substitution", () => {
    // `Wren is Ringer(belfry)` — the merged hook body is `-> belfry`.
    const hook = between(g, "character:Wren", "belfry").find((e) => e.kind === "hook");
    expect(hook).toBeDefined();
    expect(hook!.label).toBe("on scan guest");
  });

  it("emits tunnel edges from the (name) -> form", () => {
    const tunnel = between(g, "Sexton.rounds", "belfry").find((e) => e.kind === "tunnel");
    expect(tunnel).toBeDefined();
  });

  it("marks unresolved diverts with the raw target", () => {
    const dangling = narrative(g).find((e) => e.from === "Sexton.rounds" && e.to === null && e.kind === "divert");
    expect(dangling).toBeDefined();
    expect(dangling!.unresolved).toBe("missing_beat");
  });

  it("anchors owned-beat edges onto the authored raw lines", () => {
    const dangling = narrative(g).find((e) => e.from === "Sexton.rounds" && e.unresolved === "missing_beat")!;
    expect(dangling.uri).toBe("file:///cast.loom");
    expect(dangling.span).not.toBeNull();
    // The anchored line in the real document is the divert line itself.
    const line = CAST.slice(dangling.span!.start.offset, dangling.span!.end.offset);
    expect(line).toContain("-> missing_beat");
  });
});

describe("buildStoryGraph — targetRange (rewiring anchor)", () => {
  const g = graphOf([
    ["file:///main.loom", MAIN],
    ["file:///cast.loom", CAST],
  ]);

  it("locates the exact target text of a top-level divert", () => {
    const e = between(g, "opening", "belfry").find((x) => x.kind === "choice")!;
    expect(e.targetRange).not.toBeNull();
    expect(MAIN.slice(e.targetRange!.start, e.targetRange!.end)).toBe("cast/belfry");
  });

  it("locates the target text inside an owned beat via line remap", () => {
    const e = narrative(g).find((x) => x.from === "Sexton.rounds" && x.unresolved === "missing_beat")!;
    expect(e.targetRange).not.toBeNull();
    expect(CAST.slice(e.targetRange!.start, e.targetRange!.end)).toBe("missing_beat");
  });

  it("locates a self-qualified target as written", () => {
    const hook = between(g, "character:Sexton", "Sexton.rounds").find((e) => e.kind === "hook")!;
    expect(hook.targetRange).not.toBeNull();
    expect(CAST.slice(hook.targetRange!.start, hook.targetRange!.end)).toBe("self.rounds");
  });
});

describe("buildStoryGraph — relationship overlay edges", () => {
  const g = graphOf([
    ["file:///main.loom", MAIN],
    ["file:///cast.loom", CAST],
  ]);

  it("emits cast / setting / owns / is / contains edges as non-narrative", () => {
    expect(between(g, "opening", "character:Wren").some((e) => e.kind === "cast")).toBe(true);
    expect(between(g, "opening", "location:Lighthouse").some((e) => e.kind === "setting")).toBe(true);
    expect(between(g, "character:Sexton", "Sexton.rounds").some((e) => e.kind === "owns")).toBe(true);
    expect(between(g, "character:Wren", "trait:Ringer").some((e) => e.kind === "is")).toBe(true);
    expect(
      between(g, "location:Lighthouse", "location:Belfry").length,
    ).toBe(0); // Belfry never declared — contains edge only links known entities…
  });
});

describe("buildStoryGraph — shadowed duplicates", () => {
  it("re-keys the earlier duplicate; the last declaration wins the bare name", () => {
    const g = graphOf([
      ["file:///a.loom", "== greet\n  Hello from A.\n"],
      ["file:///b.loom", "== greet\n  Hello from B.\n  -> greet\n"],
    ]);
    const bare = g.beats.get("greet")!;
    expect(bare.uri).toBe("file:///b.loom");
    expect(bare.shadowed).toBe(false);
    const shadow = g.beats.get("greet~1")!;
    expect(shadow.uri).toBe("file:///a.loom");
    expect(shadow.shadowed).toBe(true);
    // The self-divert in b resolves to the winning bare key.
    expect(between(g, "greet", "greet").some((e) => e.kind === "divert")).toBe(true);
  });
});

describe("workspace caching", () => {
  it("rebuilds the graph after a document change", () => {
    const ws = new Workspace();
    ws.updateMany([["file:///m.loom", "== a\n  -> b\n\n== b\n"]]);
    const g1 = ws.storyGraph();
    expect(g1.beats.has("c")).toBe(false);
    expect(ws.storyGraph()).toBe(g1); // cached until an edit
    ws.update("file:///m.loom", "== a\n  -> b\n\n== b\n\n== c\n");
    const g2 = ws.storyGraph();
    expect(g2).not.toBe(g1);
    expect(g2.beats.has("c")).toBe(true);
  });
});

describe("escape-the-internet corpus", () => {
  const files: Array<[string, string]> = scenarioFiles("escape-the-internet").map((f) => [
    `file:///${f.path}`,
    f.source,
  ]);
  const g = graphOf(files);

  it("indexes the full project", () => {
    expect(g.beats.size).toBeGreaterThan(20);
    expect(g.entry).toBe("doors_open");
    expect(g.files.length).toBe(files.length);
  });

  it("resolves cross-file diverts (arrival → algorithm lockdown)", () => {
    const e = between(g, "captcha_gate", "lockdown").find((x) => x.kind === "choice");
    expect(e).toBeDefined();
    expect(e!.uri).toBe("file:///beats/arrival.loom");
    expect(g.beats.get("lockdown")!.uri).toBe("file:///beats/algorithm.loom");
  });

  it("routes owned beats + their hooks (Sysadmin interrogation)", () => {
    expect(g.beats.get("Sysadmin.interrogation")?.structural).toBe("owned");
    const hook = between(g, "character:Sysadmin", "Sysadmin.interrogation").find(
      (e) => e.kind === "hook",
    );
    expect(hook).toBeDefined();
  });

  it("routes one-liner scanner props through trait substitution", () => {
    const hook = between(g, "character:Crawler", "crawler_report").find((e) => e.kind === "hook");
    expect(hook).toBeDefined();
    expect(hook!.label).toBe("on scan guest");
  });

  it("escapes the owned firewall beat into the global lockdown", () => {
    const e = between(g, "Firewall_Terminal.firewall", "lockdown");
    expect(e.some((x) => x.narrative)).toBe(true);
  });

  it("has no unresolved narrative edges in a green corpus", () => {
    const dangling = narrative(g).filter((e) => e.to === null && e.kind !== "end");
    expect(dangling.map((e) => `${e.from} -> ${e.unresolved}`)).toEqual([]);
  });
});
