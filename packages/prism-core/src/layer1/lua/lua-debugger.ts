/**
 * Lua step-through debugger — unified for raw Lua source and
 * visual-script-generated Lua.
 *
 * Approach: trace-based. The debugger wraps the user's Lua in a chunk
 * loaded separately from the instrumentation wrapper, installs a Lua
 * line hook (`debug.sethook(..., "l")`), and records every line hit
 * together with a tostring()-snapshot of the local variables into a
 * frame list. The debugger then exposes that frame list to the UI,
 * which drives step / continue / inspect interactions over the recorded
 * trace.
 *
 * Why trace-based rather than live pause:
 *   - wasmoon runs Lua synchronously; pausing inside a hook to wait
 *     for a JS click would require coroutine yielding from inside a
 *     hook, which is fragile across wasmoon releases.
 *   - Every Prism script already goes through Prism.* JS bindings, so
 *     the observable state at each line is a snapshot — replaying a
 *     recorded trace is indistinguishable from live stepping for the
 *     user.
 *   - Trace-based is deterministic and test-friendly.
 *
 * Unified debugging for visual scripts:
 *   - Visual scripts compile to Lua via emitStepsLuaWithMap(), which
 *     returns a Map<ScriptStepId, lineNumber>. Breakpoints set on a
 *     step translate to Lua line breakpoints. When the debugger pauses
 *     on a line, the owning step is found via the reverse map and the
 *     visual step card is highlighted.
 */

import { LuaFactory } from "wasmoon";
import type { LuaEngine } from "wasmoon";

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

/** A Lua step-through debugger for a single script. */
export interface LuaDebugger {
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
  /** Dispose the underlying Lua engine. */
  dispose(): Promise<void>;
}

// ── Wrapper Lua ─────────────────────────────────────────────────────────────
// The instrumentation wrapper. `__prism_user_source` is injected via
// engine.global.set; this keeps the user source out of the wrapper's own
// line numbering.

const WRAPPER_LUA = `
local __prism_frames = {}

local __chunk, __load_err = load(__prism_user_source, "user", "t", _ENV)
if not __chunk then
  return { ok = false, err = tostring(__load_err), frames = __prism_frames }
end

-- Closure captures __chunk so the hook can match by function identity —
-- this is how we distinguish wrapper line events from user-chunk line
-- events without relying on source-name string matching (which varies
-- across Lua versions and chunkname prefix conventions).
local function __prism_hook(event, line)
  local info = debug.getinfo(2, "f")
  if info == nil or info.func ~= __chunk then return end
  local locals = {}
  local i = 1
  while true do
    local name, value = debug.getlocal(2, i)
    if name == nil then break end
    -- skip internal ("(*temporary)") locals
    if name:sub(1, 1) ~= "(" then
      local ok, rendered = pcall(tostring, value)
      locals[name] = ok and rendered or "<?>"
    end
    i = i + 1
  end
  table.insert(__prism_frames, { line = line, locals = locals })
end

debug.sethook(__prism_hook, "l")
local __ok, __err = pcall(__chunk)
debug.sethook()

return { ok = __ok, err = __ok and "" or tostring(__err), frames = __prism_frames }
`;

// ── Engine factory ──────────────────────────────────────────────────────────

let factory: LuaFactory | null = null;

async function getFactory(): Promise<LuaFactory> {
  if (!factory) factory = new LuaFactory();
  return factory;
}

// ── Implementation ──────────────────────────────────────────────────────────

/**
 * Create a Lua step debugger. Each debugger owns a persistent LuaEngine
 * so injected JS globals (Prism.*, etc.) remain available between runs.
 * Call `dispose()` when done.
 */
export async function createLuaDebugger(options?: {
  /** Seed globals installed on the engine before every run. */
  globals?: Record<string, unknown>;
}): Promise<LuaDebugger> {
  const f = await getFactory();
  const engine: LuaEngine = await f.createEngine();
  const breakpoints = new Set<number>();

  if (options?.globals) {
    for (const [key, value] of Object.entries(options.globals)) {
      engine.global.set(key, value);
    }
  }

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
      if (args) {
        for (const [key, value] of Object.entries(args)) {
          engine.global.set(key, value);
        }
      }
      engine.global.set("__prism_user_source", source);

      try {
        const raw = await engine.doString(WRAPPER_LUA);
        const result = normalizeRunResult(raw);
        // Mark frames whose line is a current breakpoint.
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
      engine.global.close();
    },
  };
}

// ── Result normalization ────────────────────────────────────────────────────

/**
 * Coerce the wrapper's Lua return table into a typed DebugRunResult.
 * wasmoon hands back Lua tables as plain JS objects, but Lua arrays are
 * 1-indexed — we must iterate both numeric keys and plain keys defensively.
 */
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
  // Lua arrays arrive as objects with 1-based numeric keys.
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
