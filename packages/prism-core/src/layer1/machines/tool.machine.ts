/**
 * XState tool machine for canvas interaction modes.
 *
 * Prevents click event chaos in multi-layer canvases by enforcing
 * a strict FSM for tool mode management.
 *
 * States: Hand → Select → Edit (and back)
 * - Hand: pan/zoom the canvas, no node interaction
 * - Select: click to select nodes, drag to multi-select
 * - Edit: click into node content (CodeMirror, Markdown)
 */

import { createMachine, createActor, type ActorRefFrom } from "xstate";

export type ToolMode = "hand" | "select" | "edit";

export type ToolEvent =
  | { type: "SWITCH_HAND" }
  | { type: "SWITCH_SELECT" }
  | { type: "SWITCH_EDIT" }
  | { type: "DOUBLE_CLICK_NODE" }
  | { type: "CLICK_CANVAS" }
  | { type: "PRESS_ESCAPE" };

export const toolMachine = createMachine({
  id: "tool",
  initial: "select",
  states: {
    hand: {
      on: {
        SWITCH_SELECT: { target: "select" },
        SWITCH_EDIT: { target: "edit" },
        PRESS_ESCAPE: { target: "select" },
      },
    },
    select: {
      on: {
        SWITCH_HAND: { target: "hand" },
        SWITCH_EDIT: { target: "edit" },
        DOUBLE_CLICK_NODE: { target: "edit" },
      },
    },
    edit: {
      on: {
        SWITCH_HAND: { target: "hand" },
        SWITCH_SELECT: { target: "select" },
        PRESS_ESCAPE: { target: "select" },
        CLICK_CANVAS: { target: "select" },
      },
    },
  },
});

/** Create a running tool machine actor. */
export function createToolActor() {
  return createActor(toolMachine).start();
}

/** Get the current tool mode from an actor snapshot. */
export function getToolMode(
  actor: ActorRefFrom<typeof toolMachine>,
): ToolMode {
  return actor.getSnapshot().value as ToolMode;
}
