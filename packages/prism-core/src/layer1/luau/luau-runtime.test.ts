import { describe, it, expect } from "vitest";
import { executeLuau } from "./luau-runtime.js";

describe("executeLuau (luau-web)", () => {
  it("should execute simple arithmetic", async () => {
    const result = await executeLuau("return 2 + 2");
    expect(result.success).toBe(true);
    expect(result.value).toBe(4);
  });

  it("should inject and use arguments", async () => {
    const result = await executeLuau("return x + y", { x: 10, y: 20 });
    expect(result.success).toBe(true);
    expect(result.value).toBe(30);
  });

  it("should handle string operations", async () => {
    const result = await executeLuau('return "Hello, " .. name', {
      name: "Prism",
    });
    expect(result.success).toBe(true);
    expect(result.value).toBe("Hello, Prism");
  });

  it("should return tables as objects", async () => {
    const result = await executeLuau("return {a = 1, b = 2}");
    expect(result.success).toBe(true);
    expect(result.value).toEqual({ a: 1, b: 2 });
  });

  it("should handle errors gracefully", async () => {
    const result = await executeLuau("error('test error')");
    expect(result.success).toBe(false);
    expect(result.error).toContain("test error");
  });

  it("should run the same factorial script as mlua daemon", async () => {
    // This exact script also runs in Rust mlua/luau tests —
    // validates browser/daemon Luau parity.
    const script = `
      local function factorial(n)
        if n <= 1 then return 1 end
        return n * factorial(n - 1)
      end
      return factorial(input)
    `;
    const result = await executeLuau(script, { input: 5 });
    expect(result.success).toBe(true);
    expect(result.value).toBe(120);
  });

  it("should handle nil return", async () => {
    const result = await executeLuau("return nil");
    expect(result.success).toBe(true);
    expect(result.value).toBeNull();
  });

  it("should handle boolean return", async () => {
    const result = await executeLuau("return true");
    expect(result.success).toBe(true);
    expect(result.value).toBe(true);
  });
});
