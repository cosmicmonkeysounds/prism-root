import { describe, it, expect, beforeEach } from "vitest";
import { ObjectRegistry } from "./registry.js";
import { objectId } from "./types.js";
import type { EntityDef, CategoryRule, EdgeTypeDef } from "./types.js";

// ── Shared fixture ────────────────────────────────────────────────────────────
// A small "workspace app" type system: workspaces contain tasks and notes.

const CATEGORY_RULES: CategoryRule[] = [
  {
    category: "workspace",
    canParent: ["workspace", "content", "record"],
    canBeRoot: true,
  },
  { category: "content", canParent: [], canBeRoot: true },
  { category: "record", canParent: ["line-item"], canBeRoot: true },
  { category: "line-item", canParent: [], canBeRoot: false },
];

const TYPE_DEFS: EntityDef<string>[] = [
  {
    type: "workspace",
    category: "workspace",
    label: "Workspace",
    icon: "folder",
    color: "#3b82f6",
  },
  { type: "project", category: "workspace", label: "Project", icon: "folder" },
  { type: "task", category: "content", label: "Task", icon: "check" },
  { type: "note", category: "content", label: "Note", icon: "note" },
  { type: "budget", category: "record", label: "Budget", icon: "money" },
  {
    type: "line-item",
    category: "line-item",
    label: "Line Item",
    icon: "dash",
    childOnly: true,
  },
  {
    type: "account",
    category: "content",
    label: "Account",
    extraChildTypes: ["account"],
  },
  {
    type: "folder",
    category: "content",
    label: "Folder",
    extraParentTypes: ["workspace"],
  },
];

describe("ObjectRegistry", () => {
  let registry: ObjectRegistry<string>;

  beforeEach(() => {
    registry = new ObjectRegistry<string>(CATEGORY_RULES);
    registry.registerAll(TYPE_DEFS);
  });

  // ── Entity lookup ──────────────────────────────────────────────────────────

  it("registers and retrieves entity definitions", () => {
    expect(registry.has("task")).toBe(true);
    expect(registry.get("task")?.label).toBe("Task");
    expect(registry.has("unknown")).toBe(false);
  });

  it("returns all registered types", () => {
    expect(registry.allTypes()).toContain("task");
    expect(registry.allTypes()).toContain("workspace");
    expect(registry.allTypes().length).toBe(TYPE_DEFS.length);
  });

  it("returns label, plural label, color, category", () => {
    expect(registry.getLabel("task")).toBe("Task");
    expect(registry.getPluralLabel("task")).toBe("Task"); // no plural set
    expect(registry.getColor("workspace")).toBe("#3b82f6");
    expect(registry.getColor("unknown")).toBe("#888888"); // default
    expect(registry.getCategory("task")).toBe("content");
  });

  // ── Containment ────────────────────────────────────────────────────────────

  it("validates parent-child by category rule", () => {
    expect(registry.canBeChildOf("task", "workspace")).toBe(true);
    expect(registry.canBeChildOf("task", "task")).toBe(false); // content can't parent content
  });

  it("validates extraChildTypes override", () => {
    expect(registry.canBeChildOf("account", "account")).toBe(true);
  });

  it("validates extraParentTypes override", () => {
    expect(registry.canBeChildOf("folder", "workspace")).toBe(true);
  });

  it("canBeRoot respects childOnly", () => {
    expect(registry.canBeRoot("task")).toBe(true);
    expect(registry.canBeRoot("line-item")).toBe(false);
  });

  it("canBeRoot respects category canBeRoot: false", () => {
    expect(registry.canBeRoot("line-item")).toBe(false);
  });

  it("canHaveChildren", () => {
    expect(registry.canHaveChildren("workspace")).toBe(true);
    expect(registry.canHaveChildren("task")).toBe(false);
    expect(registry.canHaveChildren("account")).toBe(true); // extraChildTypes
  });

  it("getAllowedChildTypes", () => {
    const allowed = registry.getAllowedChildTypes("workspace");
    expect(allowed).toContain("task");
    expect(allowed).toContain("note");
    expect(allowed).toContain("project"); // workspace can parent workspace
    expect(allowed).not.toContain("line-item"); // line-item is not content/workspace/record
  });

  // ── Edge types ─────────────────────────────────────────────────────────────

  it("registers and queries edge types", () => {
    registry.registerEdge({
      relation: "blocks",
      label: "Blocks",
      behavior: "dependency",
      sourceTypes: ["task"],
      targetTypes: ["task"],
    });

    expect(registry.getEdgeType("blocks")?.label).toBe("Blocks");
    expect(registry.allEdgeTypes()).toContain("blocks");
    expect(registry.canConnect("blocks", "task", "task")).toBe(true);
    expect(registry.canConnect("blocks", "note", "task")).toBe(false);
  });

  it("canConnect returns true for unknown relations", () => {
    expect(registry.canConnect("unknown-rel", "task", "note")).toBe(true);
  });

  it("filters edges by source/target type", () => {
    registry.registerEdges([
      {
        relation: "blocks",
        label: "Blocks",
        sourceTypes: ["task"],
        targetTypes: ["task"],
      },
      {
        relation: "references",
        label: "References",
        suggestInline: true,
      },
    ]);

    expect(registry.getEdgesFrom("task").length).toBe(2); // blocks + references (no constraint)
    expect(registry.getEdgesTo("note").length).toBe(1); // references only
    expect(registry.getEdgesBetween("task", "task").length).toBe(2);
  });

  // ── Slots ──────────────────────────────────────────────────────────────────

  it("registers and retrieves slots by type", () => {
    registry.registerSlot({
      slot: {
        id: "kami:brain",
        tabs: [{ id: "brain", label: "Brain" }],
        fields: [{ id: "kami_brain_state", type: "enum" }],
      },
      forTypes: ["task"],
    });

    const slots = registry.getSlots("task");
    expect(slots.length).toBe(1);
    expect(slots[0]!.id).toBe("kami:brain");

    expect(registry.getSlots("note").length).toBe(0);
  });

  it("registers slots by category", () => {
    registry.registerSlot({
      slot: { id: "audit:log", tabs: [{ id: "audit", label: "Audit Log" }] },
      forCategories: ["content"],
    });

    expect(registry.getSlots("task").length).toBe(1); // task is content
    expect(registry.getSlots("note").length).toBe(1); // note is content
    expect(registry.getSlots("workspace").length).toBe(0);
  });

  it("getEffectiveTabs merges base + slot tabs", () => {
    registry.register({
      type: "character",
      category: "content",
      label: "Character",
      tabs: [{ id: "overview", label: "Overview" }],
    });

    registry.registerSlot({
      slot: { id: "kami:brain", tabs: [{ id: "brain", label: "Brain" }] },
      forTypes: ["character"],
    });

    const tabs = registry.getEffectiveTabs("character");
    expect(tabs.map((t) => t.id)).toEqual(["overview", "brain"]);
  });

  it("getEffectiveTabs: base tabs win on collision", () => {
    registry.register({
      type: "character",
      category: "content",
      label: "Character",
      tabs: [{ id: "overview", label: "Overview" }],
    });

    registry.registerSlot({
      slot: {
        id: "test:override",
        tabs: [{ id: "overview", label: "OVERRIDDEN" }],
      },
      forTypes: ["character"],
    });

    const tabs = registry.getEffectiveTabs("character");
    expect(tabs.find((t) => t.id === "overview")?.label).toBe("Overview");
  });

  it("getEntityFields merges base + slot fields", () => {
    registry.register({
      type: "character",
      category: "content",
      label: "Character",
      fields: [{ id: "name", type: "string" }],
    });

    registry.registerSlot({
      slot: {
        id: "kami:brain",
        fields: [{ id: "kami_state", type: "enum" }],
      },
      forTypes: ["character"],
    });

    const fields = registry.getEntityFields("character");
    expect(fields.map((f) => f.id)).toEqual(["name", "kami_state"]);
  });

  // ── Tree building ──────────────────────────────────────────────────────────

  it("builds a tree from flat objects", () => {
    const objects = [
      {
        id: objectId("root"),
        type: "workspace",
        name: "Root",
        parentId: null,
        position: 0,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
        createdAt: "",
        updatedAt: "",
      },
      {
        id: objectId("child"),
        type: "task",
        name: "Task",
        parentId: objectId("root"),
        position: 0,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
        createdAt: "",
        updatedAt: "",
      },
    ];

    const tree = registry.buildTree(objects);
    expect(tree.length).toBe(1);
    expect(tree[0]!.object.name).toBe("Root");
    expect(tree[0]!.children.length).toBe(1);
    expect(tree[0]!.children[0]!.object.name).toBe("Task");
  });
});
