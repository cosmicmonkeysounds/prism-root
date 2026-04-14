import { describe, it, expect } from "vitest";
import { HelpRegistry } from "@prism/core/help";
import { PUCK_HELP_ENTRIES } from "./puck-help-entries.js";
import { BUNDLED_HELP_DOCS } from "./help-docs/index.js";

describe("puck-help-entries", () => {
  it("registers every entry with HelpRegistry as an import side-effect", () => {
    expect(PUCK_HELP_ENTRIES.length).toBeGreaterThan(0);
    for (const entry of PUCK_HELP_ENTRIES) {
      expect(HelpRegistry.get(entry.id)).toEqual(entry);
    }
  });

  it("assigns a unique id to every entry", () => {
    const ids = PUCK_HELP_ENTRIES.map((e) => e.id);
    const deduped = new Set(ids);
    expect(deduped.size).toBe(ids.length);
  });

  it("uses the documented id conventions", () => {
    const prefixes = [
      "puck.categories.",
      "puck.components.",
      "puck.fields.",
      "puck.regions.",
    ];
    for (const entry of PUCK_HELP_ENTRIES) {
      const ok = prefixes.some((p) => entry.id.startsWith(p));
      expect(ok, `id "${entry.id}" does not match a known prefix`).toBe(true);
    }
  });

  it("gives every entry a human title and a non-empty summary", () => {
    for (const entry of PUCK_HELP_ENTRIES) {
      expect(entry.title.trim().length).toBeGreaterThan(0);
      expect(entry.summary.trim().length).toBeGreaterThan(20);
    }
  });

  it("ensures every docPath resolves to bundled markdown", () => {
    for (const entry of PUCK_HELP_ENTRIES) {
      if (!entry.docPath) continue;
      expect(
        BUNDLED_HELP_DOCS[entry.docPath],
        `docPath "${entry.docPath}" on entry "${entry.id}" has no bundled markdown`,
      ).toBeDefined();
    }
  });

  it("covers the twelve flagship components with full docs", () => {
    const flagshipIds = [
      "puck.components.page-shell",
      "puck.components.app-shell",
      "puck.components.site-header",
      "puck.components.site-footer",
      "puck.components.section",
      "puck.components.columns",
      "puck.components.heading",
      "puck.components.image",
      "puck.components.facet-view",
      "puck.components.luau-block",
      "puck.components.record-list",
      "puck.components.spatial-canvas",
    ];
    for (const id of flagshipIds) {
      const entry = HelpRegistry.get(id);
      expect(entry, `missing flagship entry: ${id}`).toBeDefined();
      expect(entry?.docPath).toBeDefined();
    }
  });

  it("exposes search over the registered entries", () => {
    const results = HelpRegistry.search("record list");
    const ids = results.map((r) => r.id);
    expect(ids).toContain("puck.components.record-list");
  });

  it("registers exactly four shell-region entries", () => {
    const regionEntries = PUCK_HELP_ENTRIES.filter((e) =>
      e.id.startsWith("puck.regions."),
    );
    expect(regionEntries.map((e) => e.id).sort()).toEqual([
      "puck.regions.footer",
      "puck.regions.header",
      "puck.regions.main",
      "puck.regions.sidebar",
    ]);
  });
});
