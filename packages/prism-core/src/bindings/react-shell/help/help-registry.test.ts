import { describe, it, expect, beforeEach } from "vitest";
import { HelpRegistry } from "./help-registry.js";
import type { HelpEntry } from "./types.js";

const ENTRY_A: HelpEntry = {
  id: "puck.components.record-list",
  title: "Record List",
  summary:
    "Queries kernel records by type and renders them as cards or rows.",
  docPath: "help/puck/record-list.md",
};

const ENTRY_B: HelpEntry = {
  id: "puck.components.page-shell",
  title: "Page Shell",
  summary: "Outer frame for a page — header, sidebar, main, footer slots.",
};

const ENTRY_C: HelpEntry = {
  id: "puck.fields.border-radius",
  title: "Border Radius",
  summary: "Corner rounding expressed in pixels.",
};

describe("HelpRegistry", () => {
  beforeEach(() => {
    HelpRegistry.clear();
  });

  describe("register", () => {
    it("stores an entry by id", () => {
      HelpRegistry.register(ENTRY_A);
      expect(HelpRegistry.get(ENTRY_A.id)).toEqual(ENTRY_A);
    });

    it("overwrites an existing entry with the same id", () => {
      HelpRegistry.register(ENTRY_A);
      const updated: HelpEntry = { ...ENTRY_A, title: "Record List (v2)" };
      HelpRegistry.register(updated);
      expect(HelpRegistry.get(ENTRY_A.id)?.title).toBe("Record List (v2)");
    });
  });

  describe("registerMany", () => {
    it("stores every entry in the array", () => {
      HelpRegistry.registerMany([ENTRY_A, ENTRY_B, ENTRY_C]);
      expect(HelpRegistry.get(ENTRY_A.id)).toBeDefined();
      expect(HelpRegistry.get(ENTRY_B.id)).toBeDefined();
      expect(HelpRegistry.get(ENTRY_C.id)).toBeDefined();
    });

    it("accepts a readonly array", () => {
      const entries: readonly HelpEntry[] = [ENTRY_A, ENTRY_B];
      HelpRegistry.registerMany(entries);
      expect(HelpRegistry.getAll()).toHaveLength(2);
    });
  });

  describe("get", () => {
    it("returns undefined for a missing id", () => {
      expect(HelpRegistry.get("does.not.exist")).toBeUndefined();
    });
  });

  describe("getAll", () => {
    it("returns an array of every registered entry", () => {
      HelpRegistry.registerMany([ENTRY_A, ENTRY_B]);
      const all = HelpRegistry.getAll();
      expect(all).toHaveLength(2);
      expect(all).toEqual(expect.arrayContaining([ENTRY_A, ENTRY_B]));
    });

    it("returns a fresh array (callers cannot mutate the registry)", () => {
      HelpRegistry.register(ENTRY_A);
      const snapshot = HelpRegistry.getAll();
      snapshot.pop();
      expect(HelpRegistry.get(ENTRY_A.id)).toBeDefined();
    });
  });

  describe("search", () => {
    beforeEach(() => {
      HelpRegistry.registerMany([ENTRY_A, ENTRY_B, ENTRY_C]);
    });

    it("returns entries whose title matches a single word", () => {
      const results = HelpRegistry.search("record");
      expect(results).toHaveLength(1);
      expect(results[0]?.id).toBe(ENTRY_A.id);
    });

    it("returns entries whose summary matches a single word", () => {
      const results = HelpRegistry.search("footer");
      expect(results).toHaveLength(1);
      expect(results[0]?.id).toBe(ENTRY_B.id);
    });

    it("is case-insensitive", () => {
      expect(HelpRegistry.search("RECORD")).toHaveLength(1);
      expect(HelpRegistry.search("Record")).toHaveLength(1);
      expect(HelpRegistry.search("record")).toHaveLength(1);
    });

    it("requires every whitespace-separated word to match (AND-logic)", () => {
      expect(HelpRegistry.search("record kernel")).toHaveLength(1);
      expect(HelpRegistry.search("record footer")).toHaveLength(0);
    });

    it("matches across title and summary together", () => {
      expect(HelpRegistry.search("page sidebar")).toHaveLength(1);
    });

    it("returns an empty array for an empty query", () => {
      expect(HelpRegistry.search("")).toEqual([]);
      expect(HelpRegistry.search("   ")).toEqual([]);
    });

    it("returns an empty array when nothing matches", () => {
      expect(HelpRegistry.search("zzz-nope")).toEqual([]);
    });
  });

  describe("clear", () => {
    it("removes every entry", () => {
      HelpRegistry.registerMany([ENTRY_A, ENTRY_B, ENTRY_C]);
      HelpRegistry.clear();
      expect(HelpRegistry.getAll()).toEqual([]);
      expect(HelpRegistry.get(ENTRY_A.id)).toBeUndefined();
    });
  });
});
