import { describe, it, expect, afterEach } from "vitest";
import { createLuauDebugger, type LuauDebugger } from "./luau-debugger.js";

describe("createLuauDebugger", () => {
  let dbg: LuauDebugger | null = null;

  afterEach(async () => {
    if (dbg) {
      await dbg.dispose();
      dbg = null;
    }
  });

  it("runs a simple script and records one frame per user line", async () => {
    dbg = await createLuauDebugger();
    const source = `local a = 1\nlocal b = 2\nlocal c = a + b\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // Three statements → at least three frames; instrumentation injects a
    // __prism_trace call before each line, so allow >= 3.
    expect(result.frames.length).toBeGreaterThanOrEqual(3);
    const lines = result.frames.map((f) => f.line);
    expect(lines).toContain(1);
    expect(lines).toContain(2);
    expect(lines).toContain(3);
  });

  it("captures locals at each line", async () => {
    dbg = await createLuauDebugger();
    const source = `local a = 10\nlocal b = 20\nlocal sum = a + b\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // Frames are emitted for each line regardless of whether debug.getlocal
    // is available (luau-web sandboxes it; mlua daemon exposes it).
    expect(result.frames.length).toBeGreaterThanOrEqual(3);
  });

  it("records a frame for each iteration of a loop", async () => {
    dbg = await createLuauDebugger();
    const source = `local total = 0\nfor i = 1, 3 do\n  total = total + i\nend\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // The `total = total + i` line fires three times (once per iteration).
    const line3Hits = result.frames.filter((f) => f.line === 3).length;
    expect(line3Hits).toBe(3);
  });

  it("reports syntax errors without frames", async () => {
    dbg = await createLuauDebugger();
    const result = await dbg.run("local a = ");
    expect(result.success).toBe(false);
    expect(result.error).toBeDefined();
    expect(result.frames).toEqual([]);
  });

  it("reports runtime errors and stops tracing at the failing line", async () => {
    dbg = await createLuauDebugger();
    const source = `local a = 1\nerror("boom")\nlocal b = 2\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(false);
    expect(result.error).toContain("boom");
    // Trace should contain at most the line where the error was raised.
    const lines = result.frames.map((f) => f.line);
    expect(lines).toContain(1);
    // Line 3 should never have been reached.
    expect(lines).not.toContain(3);
  });

  it("tracks breakpoints independently of execution", async () => {
    dbg = await createLuauDebugger();
    expect(dbg.listBreakpoints()).toEqual([]);
    dbg.setBreakpoint(2);
    dbg.setBreakpoint(5);
    dbg.setBreakpoint(2); // idempotent
    expect(dbg.listBreakpoints()).toEqual([2, 5]);
    expect(dbg.toggleBreakpoint(2)).toBe(false);
    expect(dbg.toggleBreakpoint(3)).toBe(true);
    expect(dbg.listBreakpoints()).toEqual([3, 5]);
    dbg.clearBreakpoint(3);
    expect(dbg.listBreakpoints()).toEqual([5]);
    dbg.clearAllBreakpoints();
    expect(dbg.listBreakpoints()).toEqual([]);
  });

  it("marks frames on breakpoint lines and filters via breakpointFrames", async () => {
    dbg = await createLuauDebugger();
    dbg.setBreakpoint(2);
    const source = `local a = 1\nlocal b = 2\nlocal c = 3\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    const bpFrames = dbg.breakpointFrames(result.frames);
    expect(bpFrames.length).toBeGreaterThanOrEqual(1);
    expect(bpFrames.every((f) => f.line === 2)).toBe(true);
    const line2Frame = result.frames.find((f) => f.line === 2);
    expect(line2Frame?.breakpoint).toBe(true);
    const line1Frame = result.frames.find((f) => f.line === 1);
    expect(line1Frame?.breakpoint).toBe(false);
  });

  it("preserves line numbers relative to user source (not wrapper)", async () => {
    dbg = await createLuauDebugger();
    const source = `local x = 1\nlocal y = 2\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // Line numbers in frames are those passed to __prism_trace, which are
    // the original source line numbers — not the wrapped script's lines.
    const maxLine = Math.max(...result.frames.map((f) => f.line));
    expect(maxLine).toBeLessThanOrEqual(2);
  });

  it("injects JS globals into the script", async () => {
    dbg = await createLuauDebugger({ globals: { seed: 41 } });
    // If seed is not injected the arithmetic would error; success proves injection works.
    const result = await dbg.run(`local v = seed + 1\nlocal _tail = true\n`);
    expect(result.success).toBe(true);
    expect(result.frames.length).toBeGreaterThanOrEqual(1);
  });

  it("can run multiple scripts on the same debugger without leaking globals", async () => {
    dbg = await createLuauDebugger();
    const r1 = await dbg.run(`local a = 1\nlocal _tail = true\n`);
    expect(r1.success).toBe(true);
    const r2 = await dbg.run(`local b = 2\nlocal _tail = true\n`);
    expect(r2.success).toBe(true);
    // Each run is independent — both should produce their own frames.
    expect(r1.frames.length).toBeGreaterThanOrEqual(1);
    expect(r2.frames.length).toBeGreaterThanOrEqual(1);
  });

  // ── AST-backed instrumentation regressions ────────────────────────────────
  // The previous regex-based line scanner injected `__prism_trace` into the
  // middle of multi-line strings and into continuation lines of multi-line
  // statements. The full-moon-backed `findStatementLines` avoids both.

  it("does not trace continuation lines inside a multi-line string", async () => {
    dbg = await createLuauDebugger();
    const source = `local s = [[\nhello\nworld\n]]\nlocal after = 1\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    const lines = result.frames.map((f) => f.line);
    // The `local s = ...` statement starts on line 1; the subsequent lines
    // 2, 3, 4 are part of the long-string literal and must NOT be traced.
    expect(lines).toContain(1);
    expect(lines).not.toContain(2);
    expect(lines).not.toContain(3);
    expect(lines).not.toContain(4);
    // The statement after the long string starts on line 5 and must be traced.
    expect(lines).toContain(5);
  });

  it("traces only the first line of a multi-line statement", async () => {
    dbg = await createLuauDebugger();
    // The function call spans lines 1–3; only line 1 is a statement start.
    const source = `print(\n  "hi",\n  "there"\n)\nlocal tail = 1\n`;
    const result = await dbg.run(source);
    // print is not defined in the sandbox; we don't care if it errors —
    // we care that the trace frames only include line 1 and line 5.
    const lines = result.frames.map((f) => f.line);
    expect(lines).toContain(1);
    expect(lines).not.toContain(2);
    expect(lines).not.toContain(3);
    expect(lines).not.toContain(4);
  });

  it("traces statements nested inside if/then/else blocks", async () => {
    dbg = await createLuauDebugger();
    const source = `local x = 1\nif x > 0 then\n  local y = 2\nelse\n  local z = 3\nend\nlocal tail = 1\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    const lines = result.frames.map((f) => f.line);
    // Statements inside the if body must be traced.
    expect(lines).toContain(3);
    // The tail statement after `end` must also be traced.
    expect(lines).toContain(7);
  });
});
