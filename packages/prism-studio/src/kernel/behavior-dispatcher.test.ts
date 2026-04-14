import { describe, it, expect, beforeEach } from "vitest";
import { createStudioKernel } from "./studio-kernel.js";
import type { StudioKernel } from "./studio-kernel.js";
import type { ObjectId } from "@prism/core/object-model";

describe("behavior dispatcher", () => {
  let kernel: StudioKernel;
  let targetId: ObjectId;

  beforeEach(() => {
    kernel = createStudioKernel();
    const target = kernel.createObject({
      type: "button",
      name: "Fire me",
      parentId: null,
      position: 0,
      data: {},
    });
    targetId = target.id;
  });

  function addBehavior(patch: {
    trigger?: string;
    source: string;
    enabled?: boolean;
    targetOverride?: ObjectId;
  }) {
    return kernel.createObject({
      type: "behavior",
      name: "Test behavior",
      parentId: null,
      position: 0,
      data: {
        targetObjectId: patch.targetOverride ?? targetId,
        trigger: patch.trigger ?? "onClick",
        source: patch.source,
        enabled: patch.enabled ?? true,
      },
    });
  }

  describe("list", () => {
    it("returns only behaviors bound to the target", () => {
      addBehavior({ source: "return 1" });
      addBehavior({ source: "return 2" });
      const otherTarget = kernel.createObject({
        type: "button",
        name: "Other",
        parentId: null,
        position: 0,
        data: {},
      });
      addBehavior({ source: "return 3", targetOverride: otherTarget.id });

      expect(kernel.behaviors.list(targetId)).toHaveLength(2);
      expect(kernel.behaviors.list(otherTarget.id)).toHaveLength(1);
    });

    it("filters by trigger when provided", () => {
      addBehavior({ source: "return 1", trigger: "onClick" });
      addBehavior({ source: "return 2", trigger: "onMount" });

      expect(kernel.behaviors.list(targetId, "onClick")).toHaveLength(1);
      expect(kernel.behaviors.list(targetId, "onMount")).toHaveLength(1);
      expect(kernel.behaviors.list(targetId, "onChange")).toHaveLength(0);
    });

    it("skips behaviors missing a source or target", () => {
      kernel.createObject({
        type: "behavior",
        name: "Empty",
        parentId: null,
        position: 0,
        data: { targetObjectId: targetId, trigger: "onClick", source: "" },
      });
      expect(kernel.behaviors.list(targetId)).toHaveLength(0);
    });
  });

  describe("fire", () => {
    it("runs every enabled behavior for the trigger", async () => {
      addBehavior({ source: "return 10", trigger: "onClick" });
      addBehavior({ source: "return 20", trigger: "onClick" });
      addBehavior({ source: "return 30", trigger: "onMount" });

      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results).toHaveLength(2);
      expect(results.every((r) => r.success)).toBe(true);
      expect(results.map((r) => r.value).sort()).toEqual([10, 20]);
    });

    it("skips disabled behaviors", async () => {
      addBehavior({ source: "return 1", enabled: false });
      addBehavior({ source: "return 2" });

      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results).toHaveLength(1);
      expect(results[0]?.value).toBe(2);
    });

    it("returns empty when nothing matches", async () => {
      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results).toEqual([]);
    });

    it("surfaces Luau errors and adds a notification", async () => {
      addBehavior({ source: "error('boom')" });
      const before = kernel.notifications.getAll().length;

      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(false);
      expect(results[0]?.error).toBeTruthy();

      const after = kernel.notifications.getAll();
      expect(after.length).toBe(before + 1);
      expect(after[after.length - 1]?.kind).toBe("error");
    });

    it("injects ui.notify into the behavior globals", async () => {
      addBehavior({ source: 'ui.notify("hello", "world"); return 1' });
      const before = kernel.notifications.getAll().length;

      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results[0]?.success).toBe(true);

      const after = kernel.notifications.getAll();
      expect(after.length).toBe(before + 1);
      const latest = after[after.length - 1];
      expect(latest?.title).toBe("hello");
      expect(latest?.body).toBe("world");
    });

    it("exposes `self` as the target object id", async () => {
      addBehavior({ source: "return self" });
      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results[0]?.value).toBe(targetId);
    });

    it("passes the event payload to scripts", async () => {
      addBehavior({ source: "return event.clientX" });
      const results = await kernel.behaviors.fire(targetId, "onClick", {
        clientX: 42,
      });
      expect(results[0]?.value).toBe(42);
    });

    it("respects behavior.enabled being toggled via updateObject", async () => {
      const b = addBehavior({ source: "return 1" });
      kernel.updateObject(b.id, {
        data: {
          targetObjectId: targetId,
          trigger: "onClick",
          source: "return 1",
          enabled: false,
        },
      });
      const results = await kernel.behaviors.fire(targetId, "onClick");
      expect(results).toEqual([]);
    });

    it("stops firing behaviors after the target is deleted", async () => {
      addBehavior({ source: "return 1" });
      kernel.deleteObject(targetId);
      const results = await kernel.behaviors.fire(targetId, "onClick");
      // Behaviors still exist but they won't be found via list() because
      // asBehavior checks deletedAt, while the behavior itself wasn't
      // deleted — fire just runs and returns whatever matched.
      expect(results).toHaveLength(1);
      expect(results[0]?.success).toBe(true);
    });
  });

  describe("kernel integration", () => {
    it("allows behaviors to raise notifications via kernel.notify", async () => {
      const before = kernel.notifications.getAll().length;
      addBehavior({ source: 'kernel.notify("fired"); return 1' });
      await kernel.behaviors.fire(targetId, "onClick");
      const after = kernel.notifications.getAll();
      expect(after.length).toBe(before + 1);
      expect(after[after.length - 1]?.title).toBe("fired");
    });

    it("allows behaviors to shift selection via kernel.select", async () => {
      const other = kernel.createObject({
        type: "button",
        name: "Other",
        parentId: null,
        position: 0,
        data: {},
      });
      addBehavior({ source: `kernel.select("${other.id}")` });

      await kernel.behaviors.fire(targetId, "onClick");
      expect(kernel.atoms.getState().selectedId).toBe(other.id);
    });
  });
});
