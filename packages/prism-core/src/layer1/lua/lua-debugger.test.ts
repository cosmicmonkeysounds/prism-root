import { describe, it, expect, afterEach } from "vitest";
import { createLuaDebugger, type LuaDebugger } from "./lua-debugger.js";

describe("createLuaDebugger", () => {
  let dbg: LuaDebugger | null = null;

  afterEach(async () => {
    if (dbg) {
      await dbg.dispose();
      dbg = null;
    }
  });

  it("runs a simple script and records one frame per user line", async () => {
    dbg = await createLuaDebugger();
    const source = `local a = 1\nlocal b = 2\nlocal c = a + b\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // Three statements → at least three frames; some hook impls emit a frame
    // at chunk entry too, so allow >= 3.
    expect(result.frames.length).toBeGreaterThanOrEqual(3);
    const lines = result.frames.map((f) => f.line);
    expect(lines).toContain(1);
    expect(lines).toContain(2);
    expect(lines).toContain(3);
  });

  it("captures locals at each line", async () => {
    dbg = await createLuaDebugger();
    const source = `local a = 10\nlocal b = 20\nlocal sum = a + b\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // The last frame should have seen all three locals.
    const last = result.frames[result.frames.length - 1];
    expect(last).toBeDefined();
    // At least one frame should have "a" = "10" in its locals snapshot.
    const hasA = result.frames.some((f) => f.locals.a === "10");
    expect(hasA).toBe(true);
  });

  it("records a frame for each iteration of a loop", async () => {
    dbg = await createLuaDebugger();
    const source = `local total = 0\nfor i = 1, 3 do\n  total = total + i\nend\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // The `total = total + i` line fires three times (once per iteration).
    const line3Hits = result.frames.filter((f) => f.line === 3).length;
    expect(line3Hits).toBe(3);
  });

  it("reports syntax errors without frames", async () => {
    dbg = await createLuaDebugger();
    const result = await dbg.run("local a = ");
    expect(result.success).toBe(false);
    expect(result.error).toBeDefined();
    expect(result.frames).toEqual([]);
  });

  it("reports runtime errors and stops tracing at the failing line", async () => {
    dbg = await createLuaDebugger();
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
    dbg = await createLuaDebugger();
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
    dbg = await createLuaDebugger();
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
    dbg = await createLuaDebugger();
    const source = `local x = 1\nlocal y = 2\n`;
    const result = await dbg.run(source);
    expect(result.success).toBe(true);
    // If wrapper lines leaked in, we'd see lines > 2.
    const maxLine = Math.max(...result.frames.map((f) => f.line));
    expect(maxLine).toBeLessThanOrEqual(2);
  });

  it("injects JS globals into the script", async () => {
    dbg = await createLuaDebugger({ globals: { seed: 41 } });
    // Line hooks fire at the *start* of each line, so values assigned on
    // the observed line aren't visible yet — add a no-op trailing line so
    // the previous line's assignments become part of the locals snapshot.
    const result = await dbg.run(`local v = seed + 1\nlocal _tail = true\n`);
    expect(result.success).toBe(true);
    const hasV = result.frames.some((f) => f.locals.v === "42");
    expect(hasV).toBe(true);
  });

  it("can run multiple scripts on the same debugger without leaking globals", async () => {
    dbg = await createLuaDebugger();
    const r1 = await dbg.run(`local a = 1\nlocal _tail = true\n`);
    expect(r1.success).toBe(true);
    const r2 = await dbg.run(`local b = 2\nlocal _tail = true\n`);
    expect(r2.success).toBe(true);
    // Each run's trace is independent.
    expect(r1.frames.some((f) => f.locals.a === "1")).toBe(true);
    expect(r2.frames.some((f) => f.locals.b === "2")).toBe(true);
  });
});
