import { describe, it, expect } from "vitest";
import {
  listBehaviorsFor,
  newBehaviorDraft,
  mergeBehaviorEdit,
  summariseBehavior,
  type BehaviorRow,
} from "./behavior-data.js";
import { createStudioKernel } from "../kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

function mk(
  id: string,
  type: string,
  data: Record<string, unknown>,
  name: string = id,
): GraphObject {
  return {
    id: id as unknown as ObjectId,
    type,
    name,
    parentId: null,
    position: 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: null,
    color: null,
    image: null,
    pinned: false,
    data,
    deletedAt: null,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
  } as unknown as GraphObject;
}

describe("listBehaviorsFor", () => {
  it("filters behaviors by targetObjectId and ignores other types", () => {
    const objects: GraphObject[] = [
      mk("b1", "behavior", {
        trigger: "onClick",
        enabled: true,
        source: "ui.navigate('/a')",
        targetObjectId: "btn1",
      }),
      mk("b2", "behavior", {
        trigger: "onMount",
        enabled: true,
        source: "",
        targetObjectId: "other",
      }),
      mk("not-a-behavior", "button", {}),
    ];
    const rows = listBehaviorsFor("btn1", objects);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.id).toBe("b1");
    expect(rows[0]?.trigger).toBe("onClick");
  });

  it("defaults unknown triggers to onClick and enabled=true when unset", () => {
    const objects: GraphObject[] = [
      mk("b1", "behavior", { source: "", targetObjectId: "t" }),
      mk("b2", "behavior", {
        trigger: "weird",
        enabled: false,
        source: "",
        targetObjectId: "t",
      }),
    ];
    const rows = listBehaviorsFor("t", objects);
    expect(rows).toHaveLength(2);
    expect(rows.find((r) => r.id === "b1")?.trigger).toBe("onClick");
    expect(rows.find((r) => r.id === "b1")?.enabled).toBe(true);
    expect(rows.find((r) => r.id === "b2")?.trigger).toBe("onClick");
    expect(rows.find((r) => r.id === "b2")?.enabled).toBe(false);
  });

  it("skips soft-deleted behaviors", () => {
    const deleted = mk("bX", "behavior", { source: "", targetObjectId: "t" });
    (deleted as unknown as { deletedAt: string }).deletedAt = "2024-05-01T00:00:00Z";
    const rows = listBehaviorsFor("t", [deleted]);
    expect(rows).toEqual([]);
  });
});

describe("newBehaviorDraft", () => {
  it("creates a sensible default draft", () => {
    const draft = newBehaviorDraft("btn1", "app1", "onClick");
    expect(draft.type).toBe("behavior");
    expect(draft.parentId).toBe("app1");
    expect(draft.data).toMatchObject({
      trigger: "onClick",
      source: "",
      enabled: true,
      targetObjectId: "btn1",
    });
  });

  it("sets trigger-specific name", () => {
    const draft = newBehaviorDraft("btn1", "app1", "onMount");
    expect(draft.name).toBe("onMount behavior");
  });
});

describe("mergeBehaviorEdit", () => {
  const existing = mk("b1", "behavior", {
    trigger: "onClick",
    enabled: true,
    source: "ui.navigate('/a')",
    targetObjectId: "btn1",
    keepMe: "value",
  });

  it("patches only specified keys and preserves others", () => {
    const merged = mergeBehaviorEdit(existing, { source: "ui.navigate('/b')" });
    expect(merged.data["source"]).toBe("ui.navigate('/b')");
    expect(merged.data["trigger"]).toBe("onClick");
    expect(merged.data["keepMe"]).toBe("value");
  });

  it("flips enabled cleanly", () => {
    const merged = mergeBehaviorEdit(existing, { enabled: false });
    expect(merged.data["enabled"]).toBe(false);
  });
});

describe("summariseBehavior", () => {
  const row: BehaviorRow = {
    id: "b1",
    name: "Click",
    trigger: "onClick",
    source: "ui.navigate('/about')",
    enabled: true,
    targetObjectId: "btn1",
  };

  it("includes the first non-empty line of source", () => {
    const summary = summariseBehavior(row);
    expect(summary).toContain("onClick");
    expect(summary).toContain("ui.navigate");
  });

  it("marks disabled rows explicitly", () => {
    expect(summariseBehavior({ ...row, enabled: false })).toContain("(disabled)");
  });

  it("truncates long source lines", () => {
    const longSrc = "ui.navigate('" + "x".repeat(80) + "')";
    const summary = summariseBehavior({ ...row, source: longSrc });
    expect(summary).toContain("…");
  });
});

describe("behavior CRUD against the studio kernel", () => {
  it("creates, edits, and deletes behaviors through the kernel", () => {
    const kernel = createStudioKernel();
    try {
      // Seed an app + button so the behavior has a target to hang off.
      const app = kernel.createObject({
        type: "app",
        name: "Demo",
        parentId: null,
        position: 0,
        data: { name: "Demo", profileId: "studio" },
      });
      const page = kernel.createObject({
        type: "page",
        name: "Home",
        parentId: app.id,
        position: 0,
        data: {},
      });
      const btn = kernel.createObject({
        type: "button",
        name: "Click me",
        parentId: page.id,
        position: 0,
        data: {},
      });

      // Create a behavior through the draft helper.
      const draft = newBehaviorDraft(btn.id as unknown as string, app.id as unknown as string, "onClick");
      const behavior = kernel.createObject({
        type: draft.type,
        name: draft.name,
        parentId: app.id,
        position: draft.position,
        data: draft.data as unknown as Record<string, unknown>,
      });

      let rows = listBehaviorsFor(
        btn.id as unknown as string,
        kernel.store.allObjects(),
      );
      expect(rows).toHaveLength(1);
      expect(rows[0]?.trigger).toBe("onClick");

      // Edit the source via mergeBehaviorEdit.
      const existing = kernel.store.getObject(behavior.id);
      expect(existing).toBeDefined();
      kernel.updateObject(
        behavior.id,
        mergeBehaviorEdit(existing!, { source: "ui.navigate('/about')" }),
      );
      rows = listBehaviorsFor(
        btn.id as unknown as string,
        kernel.store.allObjects(),
      );
      expect(rows[0]?.source).toBe("ui.navigate('/about')");

      // Flip enabled off.
      const existing2 = kernel.store.getObject(behavior.id);
      kernel.updateObject(behavior.id, mergeBehaviorEdit(existing2!, { enabled: false }));
      rows = listBehaviorsFor(
        btn.id as unknown as string,
        kernel.store.allObjects(),
      );
      expect(rows[0]?.enabled).toBe(false);

      // Delete.
      kernel.deleteObject(behavior.id);
      rows = listBehaviorsFor(
        btn.id as unknown as string,
        kernel.store.allObjects(),
      );
      expect(rows).toEqual([]);
    } finally {
      kernel.dispose();
    }
  });
});
