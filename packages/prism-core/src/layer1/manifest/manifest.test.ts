import { describe, it, expect } from "vitest";
import {
  defaultManifest,
  parseManifest,
  serialiseManifest,
  validateManifest,
  addCollection,
  removeCollection,
  updateCollection,
  getCollection,
} from "./manifest.js";
import { MANIFEST_VERSION } from "./manifest-types.js";
import type { PrismManifest, CollectionRef } from "./manifest-types.js";

// ── defaultManifest ─────────────────────────────────────────────────────────

describe("defaultManifest", () => {
  it("creates a manifest with required fields", () => {
    const m = defaultManifest("My Project", "m-1");
    expect(m.id).toBe("m-1");
    expect(m.name).toBe("My Project");
    expect(m.version).toBe(MANIFEST_VERSION);
    expect(m.storage.backend).toBe("loro");
    expect(m.schema.modules).toEqual(["@prism/core"]);
    expect(m.visibility).toBe("private");
    expect(m.createdAt).toBeDefined();
  });
});

// ── parseManifest ───────────────────────────────────────────────────────────

describe("parseManifest", () => {
  it("parses a minimal manifest", () => {
    const json = JSON.stringify({
      id: "m-1",
      name: "Test",
      storage: { backend: "loro", path: "./data/vault.loro" },
    });
    const m = parseManifest(json);
    expect(m.id).toBe("m-1");
    expect(m.name).toBe("Test");
    expect(m.version).toBe(MANIFEST_VERSION);
    expect(m.schema.modules).toEqual(["@prism/core"]);
    expect(m.visibility).toBe("private");
  });

  it("preserves all optional fields", () => {
    const full: PrismManifest = {
      id: "m-2",
      name: "Full",
      version: "1",
      storage: { backend: "memory" },
      schema: { modules: ["@prism/core", "./custom.yaml"] },
      sync: { mode: "auto", intervalSeconds: 30, peers: ["peer1"] },
      collections: [{ id: "c1", name: "Tasks" }],
      createdAt: "2026-01-01T00:00:00Z",
      lastOpenedAt: "2026-04-01T00:00:00Z",
      modules: { editor: true, graph: false },
      settings: { "ui.theme": "dark" },
      ownerId: "user-1",
      visibility: "team",
      description: "A test manifest",
    };
    const m = parseManifest(JSON.stringify(full));
    expect(m.sync?.mode).toBe("auto");
    expect(m.sync?.peers).toEqual(["peer1"]);
    expect(m.collections).toHaveLength(1);
    expect(m.modules?.editor).toBe(true);
    expect(m.settings?.["ui.theme"]).toBe("dark");
    expect(m.ownerId).toBe("user-1");
    expect(m.visibility).toBe("team");
    expect(m.description).toBe("A test manifest");
  });

  it("throws on missing id", () => {
    expect(() =>
      parseManifest(JSON.stringify({ name: "X", storage: { backend: "memory" } })),
    ).toThrow("missing required field \"id\"");
  });

  it("throws on missing name", () => {
    expect(() =>
      parseManifest(JSON.stringify({ id: "x", storage: { backend: "memory" } })),
    ).toThrow("missing required field \"name\"");
  });

  it("throws on missing storage", () => {
    expect(() =>
      parseManifest(JSON.stringify({ id: "x", name: "X" })),
    ).toThrow("missing required field \"storage\"");
  });
});

// ── serialiseManifest ───────────────────────────────────────────────────────

describe("serialiseManifest", () => {
  it("round-trips through parse", () => {
    const m = defaultManifest("RoundTrip", "rt-1");
    const json = serialiseManifest(m);
    const parsed = parseManifest(json);
    expect(parsed.id).toBe("rt-1");
    expect(parsed.name).toBe("RoundTrip");
    expect(parsed.storage).toEqual(m.storage);
  });

  it("produces formatted JSON", () => {
    const m = defaultManifest("Formatted", "fmt-1");
    const json = serialiseManifest(m);
    expect(json).toContain("\n");
    expect(json).toContain("  ");
  });
});

// ── validateManifest ────────────────────────────────────────────────────────

describe("validateManifest", () => {
  it("returns no errors for a valid manifest", () => {
    const m = defaultManifest("Valid", "v-1");
    expect(validateManifest(m)).toEqual([]);
  });

  it("reports missing id", () => {
    const m = { ...defaultManifest("X", "x"), id: "" };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.field === "id")).toBe(true);
  });

  it("reports missing name", () => {
    const m = { ...defaultManifest("", "x") };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.field === "name")).toBe(true);
  });

  it("reports unknown storage backend", () => {
    const m = {
      ...defaultManifest("X", "x"),
      storage: { backend: "postgres" as "loro" },
    };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.field === "storage.backend")).toBe(true);
  });

  it("reports unsupported version", () => {
    const m = { ...defaultManifest("X", "x"), version: "99" };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.field === "version")).toBe(true);
  });

  it("reports unknown visibility", () => {
    const m = {
      ...defaultManifest("X", "x"),
      visibility: "secret" as "private",
    };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.field === "visibility")).toBe(true);
  });

  it("reports duplicate collection ref ids", () => {
    const m = {
      ...defaultManifest("X", "x"),
      collections: [
        { id: "c1", name: "A" },
        { id: "c1", name: "B" },
      ],
    };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.message.includes("duplicate"))).toBe(true);
  });

  it("reports collection ref missing id", () => {
    const m = {
      ...defaultManifest("X", "x"),
      collections: [{ id: "", name: "A" }],
    };
    const errors = validateManifest(m);
    expect(errors.some((e) => e.message.includes("missing id"))).toBe(true);
  });
});

// ── Collection ref helpers ──────────────────────────────────────────────────

describe("collection ref helpers", () => {
  const base = defaultManifest("Test", "t-1");
  const col1: CollectionRef = { id: "tasks", name: "Tasks", objectTypes: ["task"] };
  const col2: CollectionRef = { id: "goals", name: "Goals", objectTypes: ["goal"] };

  it("addCollection adds to manifest", () => {
    const m = addCollection(base, col1);
    expect(m.collections).toHaveLength(1);
    expect(m.collections![0]!.id).toBe("tasks");
  });

  it("addCollection throws on duplicate", () => {
    const m = addCollection(base, col1);
    expect(() => addCollection(m, col1)).toThrow("already exists");
  });

  it("removeCollection removes by id", () => {
    let m = addCollection(base, col1);
    m = addCollection(m, col2);
    m = removeCollection(m, "tasks");
    expect(m.collections).toHaveLength(1);
    expect(m.collections![0]!.id).toBe("goals");
  });

  it("removeCollection is safe for missing id", () => {
    const m = addCollection(base, col1);
    const m2 = removeCollection(m, "nonexistent");
    expect(m2.collections).toHaveLength(1);
  });

  it("updateCollection patches fields", () => {
    let m = addCollection(base, col1);
    m = updateCollection(m, "tasks", { name: "All Tasks", sortBy: "date" });
    const c = m.collections![0]!;
    expect(c.name).toBe("All Tasks");
    expect(c.sortBy).toBe("date");
    expect(c.id).toBe("tasks");
  });

  it("updateCollection throws for missing collection", () => {
    expect(() => updateCollection(base, "missing", { name: "X" })).toThrow(
      "not found",
    );
  });

  it("getCollection returns collection ref by id", () => {
    const m = addCollection(base, col1);
    expect(getCollection(m, "tasks")).toEqual(col1);
  });

  it("getCollection returns undefined for missing", () => {
    expect(getCollection(base, "missing")).toBeUndefined();
  });
});
