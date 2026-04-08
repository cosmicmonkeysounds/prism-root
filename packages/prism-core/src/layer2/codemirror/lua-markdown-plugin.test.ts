/**
 * Tests for lua-markdown-plugin — fenced block scanning and execution.
 */

import { describe, it, expect } from "vitest";
import {
  findLuaBlocks,
  formatBlockResult,
  processLuaBlocks,
  type LuaRunner,
} from "./lua-markdown-plugin.js";

describe("findLuaBlocks", () => {
  it("finds a single lua block", () => {
    const md = '# Doc\n\n```lua\nprint("hi")\n```\n\nEnd.';
    const blocks = findLuaBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.source).toBe('print("hi")');
  });

  it("finds multiple blocks", () => {
    const md = '```lua\na = 1\n```\n\nmid\n\n```lua\nb = 2\n```';
    const blocks = findLuaBlocks(md);
    expect(blocks).toHaveLength(2);
    expect(blocks[0]?.source).toBe("a = 1");
    expect(blocks[1]?.source).toBe("b = 2");
  });

  it("ignores non-lua fences", () => {
    const md = '```js\nconsole.log(1)\n```\n\n```python\nprint(1)\n```';
    expect(findLuaBlocks(md)).toEqual([]);
  });

  it("is case-insensitive on the info string", () => {
    const md = '```LUA\nx = 1\n```';
    expect(findLuaBlocks(md)).toHaveLength(1);
  });

  it("skips unterminated blocks", () => {
    const md = '```lua\nprint(1)\n\nno close fence';
    expect(findLuaBlocks(md)).toEqual([]);
  });

  it("produces offsets that slice back to the original fence", () => {
    const md = 'prefix\n```lua\nx = 1\n```\nsuffix';
    const [block] = findLuaBlocks(md);
    if (!block) throw new Error("expected a block");
    const sliced = md.slice(block.start, block.end);
    expect(sliced).toContain("```lua");
    expect(sliced).toContain("x = 1");
    expect(sliced).toContain("```");
  });
});

describe("formatBlockResult", () => {
  it("formats a successful run with a return value", () => {
    const out = formatBlockResult("return 42", { success: true, value: 42 });
    expect(out).toContain("```lua\nreturn 42\n```");
    expect(out).toContain("=> 42");
  });

  it("formats stdout without a return value", () => {
    const out = formatBlockResult('print("hi")', {
      success: true,
      value: null,
      stdout: "hi\n",
    });
    expect(out).toContain("hi");
  });

  it("formats an error", () => {
    const out = formatBlockResult("bad", { success: false, value: null, error: "boom" });
    expect(out).toContain("error: boom");
  });

  it("omits output block when nothing to show", () => {
    const out = formatBlockResult("x = 1", { success: true, value: null });
    // Only the source fence should be present
    const fences = out.match(/```/g);
    expect(fences?.length).toBe(2);
  });
});

describe("processLuaBlocks", () => {
  const echoRunner: LuaRunner = async (script) => ({
    success: true,
    value: script.trim().split("\n").length, // return line count
  });

  it("returns input unchanged when no blocks", async () => {
    const md = "# plain\n\ntext";
    expect(await processLuaBlocks(md, echoRunner)).toBe(md);
  });

  it("replaces lua blocks with source+output fences", async () => {
    const md = 'a\n\n```lua\nprint(1)\nprint(2)\n```\n\nb';
    const out = await processLuaBlocks(md, echoRunner);
    expect(out).toContain("=> 2"); // two lines echoed back
    expect(out).toContain("a");
    expect(out).toContain("b");
  });

  it("replaces every block independently", async () => {
    const md = '```lua\nx\n```\n\n```lua\nx\ny\n```';
    const out = await processLuaBlocks(md, echoRunner);
    expect(out.match(/=> 1/g)?.length).toBe(1);
    expect(out.match(/=> 2/g)?.length).toBe(1);
  });

  it("surfaces runner errors in the output", async () => {
    const erroring: LuaRunner = async () => ({
      success: false,
      value: null,
      error: "syntax error",
    });
    const md = '```lua\n???\n```';
    const out = await processLuaBlocks(md, erroring);
    expect(out).toContain("error: syntax error");
  });
});
