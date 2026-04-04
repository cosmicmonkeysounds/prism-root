import { describe, it, expect, beforeEach } from "vitest";
import { createToolActor, getToolMode } from "./index.js";
describe("toolMachine", () => {
  let actor: ReturnType<typeof createToolActor>;

  beforeEach(() => {
    actor = createToolActor();
  });

  it("starts in select mode", () => {
    expect(getToolMode(actor)).toBe("select");
  });

  it("transitions from select to hand", () => {
    actor.send({ type: "SWITCH_HAND" });
    expect(getToolMode(actor)).toBe("hand");
  });

  it("transitions from select to edit on DOUBLE_CLICK_NODE", () => {
    actor.send({ type: "DOUBLE_CLICK_NODE" });
    expect(getToolMode(actor)).toBe("edit");
  });

  it("transitions from select to edit on SWITCH_EDIT", () => {
    actor.send({ type: "SWITCH_EDIT" });
    expect(getToolMode(actor)).toBe("edit");
  });

  it("transitions from edit to select on PRESS_ESCAPE", () => {
    actor.send({ type: "SWITCH_EDIT" });
    expect(getToolMode(actor)).toBe("edit");
    actor.send({ type: "PRESS_ESCAPE" });
    expect(getToolMode(actor)).toBe("select");
  });

  it("transitions from edit to select on CLICK_CANVAS", () => {
    actor.send({ type: "SWITCH_EDIT" });
    actor.send({ type: "CLICK_CANVAS" });
    expect(getToolMode(actor)).toBe("select");
  });

  it("transitions from hand to select on PRESS_ESCAPE", () => {
    actor.send({ type: "SWITCH_HAND" });
    actor.send({ type: "PRESS_ESCAPE" });
    expect(getToolMode(actor)).toBe("select");
  });

  it("transitions from hand to edit", () => {
    actor.send({ type: "SWITCH_HAND" });
    actor.send({ type: "SWITCH_EDIT" });
    expect(getToolMode(actor)).toBe("edit");
  });

  it("transitions from edit to hand", () => {
    actor.send({ type: "SWITCH_EDIT" });
    actor.send({ type: "SWITCH_HAND" });
    expect(getToolMode(actor)).toBe("hand");
  });

  it("ignores invalid events in select mode", () => {
    actor.send({ type: "CLICK_CANVAS" });
    expect(getToolMode(actor)).toBe("select");
    actor.send({ type: "PRESS_ESCAPE" });
    expect(getToolMode(actor)).toBe("select");
  });

  it("ignores DOUBLE_CLICK_NODE in hand mode", () => {
    actor.send({ type: "SWITCH_HAND" });
    actor.send({ type: "DOUBLE_CLICK_NODE" });
    expect(getToolMode(actor)).toBe("hand");
  });
});
