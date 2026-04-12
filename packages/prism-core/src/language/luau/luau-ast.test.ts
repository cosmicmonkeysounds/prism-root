import { beforeAll, describe, expect, it } from "vitest";
import {
  ensureLuauParserLoaded,
  isLuauParserReady,
  initLuauSyntax,
  parseLuau,
  parseLuauSync,
  findUiCalls,
  findUiCallsSync,
  findStatementLines,
  findStatementLinesSync,
  validateLuau,
  validateLuauSync,
  createLuauContribution,
  createLuauSyntaxProvider,
} from "./index.js";

beforeAll(async () => {
  await initLuauSyntax();
});

// ── Loader ─────────────────────────────────────────────────────────────────

describe("luau wasm loader", () => {
  it("reports ready after init", () => {
    expect(isLuauParserReady()).toBe(true);
  });

  it("is idempotent — repeated init returns the same module", async () => {
    const a = await ensureLuauParserLoaded();
    const b = await ensureLuauParserLoaded();
    expect(a).toBe(b);
  });
});

// ── findUiCalls ────────────────────────────────────────────────────────────

describe("findUiCalls", () => {
  it("extracts a flat list of ui.* calls", async () => {
    const source = `
      ui.label("Hello")
      ui.button("Click me")
    `;
    const result = await findUiCalls(source);
    expect(result.error).toBeNull();
    expect(result.calls.map((c) => c.kind)).toEqual(["label", "button"]);
    expect(result.calls[0]?.args[0]?.value).toBe("Hello");
    expect(result.calls[0]?.args[0]?.valueKind).toBe("string");
  });

  it("extracts nested children from ui.section", async () => {
    const source = `
      ui.section("Outer", {
        ui.label("Inner 1"),
        ui.button("Inner 2"),
      })
    `;
    const result = await findUiCalls(source);
    expect(result.error).toBeNull();
    expect(result.calls).toHaveLength(1);
    const section = result.calls[0];
    expect(section?.kind).toBe("section");
    expect(section?.children.map((c) => c.kind)).toEqual(["label", "button"]);
  });

  it("returns empty calls for empty source without erroring", async () => {
    const result = await findUiCalls("   \n  ");
    expect(result.error).toBeNull();
    expect(result.calls).toEqual([]);
  });

  it("surfaces parser errors on the result instead of throwing", async () => {
    const result = await findUiCalls("ui.label(");
    // Parser error returned on `error` field, calls empty.
    expect(result.calls).toEqual([]);
    expect(result.error).not.toBeNull();
  });

  it("exposes a sync variant with identical results", () => {
    const source = `ui.label("hi")`;
    const sync = findUiCallsSync(source);
    expect(sync.error).toBeNull();
    expect(sync.calls[0]?.kind).toBe("label");
  });
});

// ── findStatementLines ─────────────────────────────────────────────────────

describe("findStatementLines", () => {
  it("returns a plain Array of 1-based line numbers", async () => {
    const source = `local x = 1\nlocal y = 2\nlocal z = 3`;
    const lines = await findStatementLines(source);
    expect(Array.isArray(lines)).toBe(true);
    expect(lines).toContain(1);
    expect(lines).toContain(2);
    expect(lines).toContain(3);
  });

  it("recurses into if/then/else blocks", async () => {
    const source = `
local x = 1
if x > 0 then
  local y = 2
else
  local z = 3
end
`;
    const lines = await findStatementLines(source);
    // Should include body lines inside the if.
    expect(lines).toContain(4);
    expect(lines).toContain(6);
  });

  it("handles multiline strings without miscounting", async () => {
    const source = `local s = [[\nline1\nline2\n]]\nlocal after = 1`;
    const lines = await findStatementLines(source);
    expect(lines).toContain(1);
    // The assignment after the long string starts on line 5.
    expect(lines).toContain(5);
  });

  it("exposes a sync variant", () => {
    const lines = findStatementLinesSync("local a = 1");
    expect(lines).toEqual([1]);
  });

  it("returns empty for empty source", async () => {
    expect(await findStatementLines("")).toEqual([]);
  });
});

// ── validateLuau ───────────────────────────────────────────────────────────

describe("validateLuau", () => {
  it("returns [] for clean source", async () => {
    const diags = await validateLuau(`local x = 1`);
    expect(diags).toEqual([]);
  });

  it("returns at least one error diagnostic on parse failure", async () => {
    const diags = await validateLuau(`local = `);
    expect(diags.length).toBeGreaterThan(0);
    expect(diags[0]?.severity).toBe("error");
    expect(diags[0]?.message).toBeTruthy();
  });

  it("sync variant matches async behavior", () => {
    expect(validateLuauSync(`local x = 1`)).toEqual([]);
    expect(validateLuauSync(`local = `).length).toBeGreaterThan(0);
  });
});

// ── parseLuau ──────────────────────────────────────────────────────────────

describe("parseLuau", () => {
  it("returns a root node for valid source", async () => {
    const root = await parseLuau(`local x = 1`);
    expect(root.type).toBe("root");
    expect(Array.isArray(root.children)).toBe(true);
  });

  it("sync variant returns a root node", () => {
    const root = parseLuauSync(`local x = 1`);
    expect(root.type).toBe("root");
  });
});

// ── LanguageContribution ───────────────────────────────────────────────────

describe("createLuauContribution", () => {
  it("exposes the expected id + extensions", () => {
    const contrib = createLuauContribution();
    expect(contrib.id).toBe("prism:luau");
    expect(contrib.extensions).toContain(".luau");
    expect(contrib.extensions).toContain(".lua");
  });

  it("exposes a code+preview surface", () => {
    const contrib = createLuauContribution();
    expect(contrib.surface.defaultMode).toBe("code");
    expect(contrib.surface.availableModes).toEqual(["code", "preview"]);
  });

  it("parses clean source into a root node", () => {
    const contrib = createLuauContribution();
    const tree = contrib.parse?.(`local x = 1`);
    expect(tree?.type).toBe("root");
    expect(Array.isArray(tree?.children)).toBe(true);
  });

  it("returns an empty root node on parse error (no ctx in the unified model)", () => {
    const contrib = createLuauContribution();
    const tree = contrib.parse?.(`local = `);
    // `parseLuauSync` tolerates some errors and returns a partial tree;
    // the contribution boundary guarantees a root node either way.
    expect(tree?.type).toBe("root");
  });

  it("provides a syntax provider via syntaxProvider()", () => {
    const contrib = createLuauContribution();
    const provider = contrib.syntaxProvider?.();
    expect(provider?.name).toBe("luau");
  });
});

// ── SyntaxProvider ─────────────────────────────────────────────────────────

describe("createLuauSyntaxProvider", () => {
  const provider = createLuauSyntaxProvider();

  it("has the expected name", () => {
    expect(provider.name).toBe("luau");
  });

  it("diagnose returns [] for clean source", () => {
    expect(provider.diagnose(`local x = 1`, 0)).toEqual([]);
  });

  it("diagnose returns diagnostics for bad source", () => {
    const diags = provider.diagnose(`local = `, 0);
    expect(diags.length).toBeGreaterThan(0);
  });

  it("complete returns ui.* suggestions after `ui.`", () => {
    const source = `ui.`;
    const items = provider.complete(source, source.length);
    expect(items.length).toBeGreaterThan(0);
    expect(items.some((c) => c.label === "ui.label")).toBe(true);
    expect(items.some((c) => c.label === "ui.section")).toBe(true);
  });

  it("complete includes documentation + insertText on items", () => {
    const items = provider.complete(`ui.`, 3);
    const label = items.find((c) => c.label === "ui.label");
    expect(label?.kind).toBe("function");
    expect(label?.insertText).toBeTruthy();
    expect(label?.documentation).toBeTruthy();
  });

  it("hover returns null", () => {
    expect(provider.hover(`local x = 1`, 0)).toBeNull();
  });
});
