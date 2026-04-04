import { describe, it, expect } from "vitest";
import { executeLua } from "./lua-runtime.js";

describe("executeLua (wasmoon)", () => {
  it("should execute simple arithmetic", async () => {
    const result = await executeLua("return 2 + 2");
    expect(result.success).toBe(true);
    expect(result.value).toBe(4);
  });

  it("should inject and use arguments", async () => {
    const result = await executeLua("return x + y", { x: 10, y: 20 });
    expect(result.success).toBe(true);
    expect(result.value).toBe(30);
  });

  it("should handle string operations", async () => {
    const result = await executeLua('return "Hello, " .. name', {
      name: "Prism",
    });
    expect(result.success).toBe(true);
    expect(result.value).toBe("Hello, Prism");
  });

  it("should return tables as objects", async () => {
    const result = await executeLua("return {a = 1, b = 2}");
    expect(result.success).toBe(true);
    expect(result.value).toEqual({ a: 1, b: 2 });
  });

  it("should handle errors gracefully", async () => {
    const result = await executeLua("error('test error')");
    expect(result.success).toBe(false);
    expect(result.error).toContain("test error");
  });

  it("should run the same factorial script as mlua daemon", async () => {
    // This exact script also runs in Rust mlua tests —
    // validates browser/daemon Lua parity.
    const script = `
      local function factorial(n)
        if n <= 1 then return 1 end
        return n * factorial(n - 1)
      end
      return factorial(input)
    `;
    const result = await executeLua(script, { input: 5 });
    expect(result.success).toBe(true);
    expect(result.value).toBe(120);
  });

  it("should handle nil return", async () => {
    const result = await executeLua("return nil");
    expect(result.success).toBe(true);
    // wasmoon returns null for nil
    expect(result.value).toBeNull();
  });

  it("should handle boolean return", async () => {
    const result = await executeLua("return true");
    expect(result.success).toBe(true);
    expect(result.value).toBe(true);
  });
});
