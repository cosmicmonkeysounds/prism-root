// Flat FSM (formerly automaton/)
export { Machine, createMachine } from "./machine.js";
export type {
  StateNode,
  Transition,
  MachineDefinition,
  MachineListener,
} from "./machine.js";

// XState tool machine (formerly machines/)
export { toolMachine, createToolActor, getToolMode } from "./tool.machine.js";
export type { ToolMode, ToolEvent } from "./tool.machine.js";
