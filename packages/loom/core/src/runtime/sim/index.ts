//! Loom ecosystem-simulation runtime — the minimal first-principles
//! engine: entities + relationships + a reactive event loop. See
//! `DESIGN.md`.

export * from "./event.ts";
export * from "./model.ts";
export { lowerRawBody, splitDirective, parseSet, splitKeyword } from "./effects.ts";
export { Sim, compileFromSource, type Person } from "./sim.ts";
