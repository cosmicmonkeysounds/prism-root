/**
 * Luau step-through debugger — unified for raw Luau source and
 * visual-script-generated Luau.
 *
 * Approach: source-instrumentation-based. The debugger preprocesses the
 * user's Luau source by injecting `__prism_trace(n)` calls before each
 * non-empty, non-comment line. A wrapper script defines `__prism_trace`
 * using `debug.getlocal` to snapshot locals at the caller's level, then
 * runs the instrumented user code inside a `pcall`. The returned frame
 * list drives step / continue / inspect interactions.
 *
 * Why source instrumentation instead of debug.sethook:
 *   - Luau does not expose `debug.sethook` (removed from the Lua 5.x API
 *     in favour of Luau's own profiling/coverage infrastructure).
 *   - Source instrumentation is deterministic, testable, and works with
 *     any Luau WASM runtime.
 *   - `debug.getlocal` IS available in Luau and gives us per-frame locals.
 *
 * Unified debugging for visual scripts:
 *   - Visual scripts compile to Luau via emitStepsLuauWithMap(), which
 *     returns a Map<ScriptStepId, lineNumber>. Breakpoints set on a step
 *     translate to Luau line breakpoints. When the debugger pauses on a
 *     line, the owning step is found via the reverse map and the visual
 *     step card is highlighted.
 */

import { LuauState } from "luau-web";
import { fromLuauValue } from "./luau-runtime.js";
import { findStatementLines } from "./luau-ast.js";

/** One recorded point in execution: line + locals snapshot. */
export interface TraceFrame {
  /** 1-based line number inside the user's source chunk. */
  line: number;
  /** tostring() rendering of each non-internal local, keyed by name. */
  locals: Record<string, string>;
  /** True if this line is currently a breakpoint. */
  breakpoint: boolean;
}

/** Result of running a script under the debugger. */
export interface DebugRunResult {
  /** True if the user chunk executed without error. */
  success: boolean;
  /** Error message if `success` is false. */
  error?: string;
  /** Full execution trace — one entry per line hit, in order. */
  frames: TraceFrame[];
}

/** A Luau step-through debugger for a single script. */
export interface LuauDebugger {
  /** Set a breakpoint on a 1-based line. */
  setBreakpoint(line: number): void;
  /** Clear a breakpoint. */
  clearBreakpoint(line: number): void;
  /** Toggle a breakpoint on the given line, returning its new state. */
  toggleBreakpoint(line: number): boolean;
  /** Clear every breakpoint. */
  clearAllBreakpoints(): void;
  /** Return a snapshot of the current breakpoints, sorted ascending. */
  listBreakpoints(): number[];
  /**
   * Run the given source under trace instrumentation. Optionally inject
   * JS globals before execution. Returns the full frame list and any error.
   */
  run(source: string, args?: Record<string, unknown>): Promise<DebugRunResult>;
  /**
   * Filter a previously-returned frame list to only those frames whose line
   * has a breakpoint set. This is what drives "continue to next breakpoint".
   */
  breakpointFrames(frames: TraceFrame[]): TraceFrame[];
  /** Dispose the underlying Luau state. */
  dispose(): Promise<void>;
}

// ── Source instrumentation ────────────────────────────────────────────────────
// Inject `__prism_trace(n)` before each line that begins a Luau statement,
// as determined by the full-moon AST (`findStatementLines`). Line numbers in
// trace calls refer to the ORIGINAL source, not the instrumented output.
//
// This replaces the earlier regex line scanner, which naively traced every
// non-empty, non-comment line and so injected bogus trace calls into the
// middle of multi-line strings and multi-line statements. The AST gives us
// the canonical set of statement-start lines — nothing more, nothing less.

async function instrumentSource(source: string): Promise<string> {
  const statementLines = new Set(await findStatementLines(source));
  const lines = source.split("\n");
  const out: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const lineNumber = i + 1;
    if (statementLines.has(lineNumber)) {
      const trimmed = line.trimStart();
      const indent = line.slice(0, line.length - trimmed.length);
      out.push(`${indent}__prism_trace(${lineNumber})`);
    }
    out.push(line);
  }
  return out.join("\n");
}

// ── Wrapper template ──────────────────────────────────────────────────────────
// __prism_trace is a Luau function (not a JS callback) so it can call
// debug.getlocal at level 2 (= the pcall anonymous function = user code scope).

const WRAPPER_PREFIX = `
local __prism_frames = {}
local __has_getlocal = type(debug) == "table" and type(debug.getlocal) == "function"

local function __prism_trace(line)
  local locals = {}
  if __has_getlocal then
    local i = 1
    while true do
      local name, value = debug.getlocal(2, i)
      if name == nil then break end
      if string.sub(name, 1, 1) ~= "(" then
        local ok, rendered = pcall(tostring, value)
        locals[name] = ok and rendered or "<?>"
      end
      i = i + 1
    end
  end
  table.insert(__prism_frames, {line = line, locals = locals})
end

local __ok, __err = pcall(function()
`;

const WRAPPER_SUFFIX = `
end)

return {ok = __ok, err = __ok and "" or tostring(__err), frames = __prism_frames}
`;

async function buildScript(source: string): Promise<string> {
  const instrumented = await instrumentSource(source);
  return WRAPPER_PREFIX + instrumented + WRAPPER_SUFFIX;
}

// ── Implementation ──────────────────────────────────────────────────────────

/**
 * Create a Luau step debugger. Seed globals (e.g. Prism.* APIs) are merged
 * into every run. Call `dispose()` when done.
 */
export async function createLuauDebugger(options?: {
  /** Seed globals installed on the engine before every run. */
  globals?: Record<string, unknown>;
}): Promise<LuauDebugger> {
  const seedGlobals: Record<string, unknown> = options?.globals ?? {};
  const breakpoints = new Set<number>();

  return {
    setBreakpoint(line) {
      breakpoints.add(line);
    },
    clearBreakpoint(line) {
      breakpoints.delete(line);
    },
    toggleBreakpoint(line) {
      if (breakpoints.has(line)) {
        breakpoints.delete(line);
        return false;
      }
      breakpoints.add(line);
      return true;
    },
    clearAllBreakpoints() {
      breakpoints.clear();
    },
    listBreakpoints() {
      return [...breakpoints].sort((a, b) => a - b);
    },

    async run(source, args) {
      const allGlobals = { ...seedGlobals, ...(args ?? {}) };
      try {
        const state = await LuauState.createAsync(allGlobals);
        const fullScript = await buildScript(source);
        const fn = state.loadstring(fullScript, "debugger", true);
        const results = await fn();
        // luau-web returns an array of multi-return values; take the first
        const rawResult = Array.isArray(results) ? results[0] : results;
        // Convert LuauTable proxies into plain JS objects
        const raw = fromLuauValue(rawResult);
        const result = normalizeRunResult(raw);
        result.frames = result.frames.map((frame) => ({
          ...frame,
          breakpoint: breakpoints.has(frame.line),
        }));
        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return { success: false, error: message, frames: [] };
      }
    },

    breakpointFrames(frames) {
      return frames.filter((frame) => breakpoints.has(frame.line));
    },

    async dispose() {
      // luau-web states are GC'd; no explicit teardown needed.
    },
  };
}

// ── Result normalization ────────────────────────────────────────────────────

function normalizeRunResult(raw: unknown): DebugRunResult {
  if (!raw || typeof raw !== "object") {
    return { success: false, error: "debugger returned no result", frames: [] };
  }
  const obj = raw as Record<string, unknown>;
  const ok = obj.ok === true;
  const err = typeof obj.err === "string" ? obj.err : "";
  const frames = normalizeFrames(obj.frames);
  const result: DebugRunResult = { success: ok, frames };
  if (!ok && err) result.error = err;
  return result;
}

function normalizeFrames(raw: unknown): TraceFrame[] {
  if (!raw || typeof raw !== "object") return [];
  const out: TraceFrame[] = [];
  const entries: unknown[] = Array.isArray(raw)
    ? raw
    : Object.keys(raw as Record<string, unknown>)
        .filter((k) => /^\d+$/.test(k))
        .sort((a, b) => Number(a) - Number(b))
        .map((k) => (raw as Record<string, unknown>)[k]);
  for (const entry of entries) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const line = typeof e.line === "number" ? e.line : 0;
    const locals = normalizeLocals(e.locals);
    if (line > 0) {
      out.push({ line, locals, breakpoint: false });
    }
  }
  return out;
}

function normalizeLocals(raw: unknown): Record<string, string> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    out[k] = typeof v === "string" ? v : String(v);
  }
  return out;
}
