/**
 * Tests for the `parseLuauUi` adapter that maps full-moon AST `ui.*` calls
 * onto the `UINode` shape consumed by the canvas, layout, and luau-facet
 * panels. Covers the positional-arg → named-props conversion for every
 * supported element kind and the nested-children wiring.
 */

import { beforeAll, describe, expect, it } from "vitest";
import { initLuauSyntax } from "@prism/core/syntax";
import { parseLuauUi } from "./luau-facet-panel.js";

beforeAll(async () => {
  await initLuauSyntax();
});

describe("parseLuauUi", () => {
  it("returns an empty result for whitespace-only source", () => {
    expect(parseLuauUi("")).toEqual({ nodes: [], error: null });
    expect(parseLuauUi("   \n  ")).toEqual({ nodes: [], error: null });
  });

  it("extracts ui.label with positional text arg", () => {
    const result = parseLuauUi(`ui.label("Hello")`);
    expect(result.error).toBeNull();
    expect(result.nodes).toEqual([
      { type: "label", props: { text: "Hello" }, children: [] },
    ]);
  });

  it("extracts ui.button with text prop", () => {
    const result = parseLuauUi(`ui.button("Click me")`);
    expect(result.nodes[0]?.type).toBe("button");
    expect(result.nodes[0]?.props["text"]).toBe("Click me");
  });

  it("extracts ui.badge with text + color props", () => {
    const result = parseLuauUi(`ui.badge("Online", "green")`);
    expect(result.nodes[0]).toEqual({
      type: "badge",
      props: { text: "Online", color: "green" },
      children: [],
    });
  });

  it("treats a single arg to ui.input as placeholder", () => {
    const result = parseLuauUi(`ui.input("Search...")`);
    expect(result.nodes[0]?.props).toEqual({ placeholder: "Search..." });
  });

  it("maps second arg to ui.input as value", () => {
    const result = parseLuauUi(`ui.input("Search...", "query")`);
    expect(result.nodes[0]?.props).toEqual({
      placeholder: "Search...",
      value: "query",
    });
  });

  it("extracts ui.section title + nested children", () => {
    const source = `
      ui.section("Status", {
        ui.label("Inner"),
        ui.button("Go"),
      })
    `;
    const result = parseLuauUi(source);
    expect(result.error).toBeNull();
    const section = result.nodes[0];
    expect(section?.type).toBe("section");
    expect(section?.props["title"]).toBe("Status");
    expect(section?.children.map((c) => c.type)).toEqual(["label", "button"]);
  });

  it("extracts ui.row / ui.column with children", () => {
    const source = `
      ui.row({
        ui.column({
          ui.label("A"),
          ui.label("B"),
        }),
      })
    `;
    const result = parseLuauUi(source);
    expect(result.error).toBeNull();
    const row = result.nodes[0];
    expect(row?.type).toBe("row");
    expect(row?.children[0]?.type).toBe("column");
    expect(row?.children[0]?.children.map((c) => c.type)).toEqual([
      "label",
      "label",
    ]);
  });

  it("extracts ui.spacer / ui.divider with no props", () => {
    const result = parseLuauUi(`
      ui.spacer()
      ui.divider()
    `);
    expect(result.nodes.map((n) => n.type)).toEqual(["spacer", "divider"]);
    expect(result.nodes[0]?.props).toEqual({});
    expect(result.nodes[1]?.props).toEqual({});
  });

  it("extracts multiple top-level calls in order", () => {
    const result = parseLuauUi(`
      ui.label("one")
      ui.label("two")
      ui.label("three")
    `);
    expect(result.nodes.map((n) => n.props["text"])).toEqual([
      "one",
      "two",
      "three",
    ]);
  });

  it("surfaces parser errors on the result instead of throwing", () => {
    // Unterminated call — full-moon reports a syntax error.
    const result = parseLuauUi(`ui.label(`);
    expect(result.nodes).toEqual([]);
    expect(result.error).not.toBeNull();
  });

  it("handles Luau comments without miscounting elements", () => {
    const source = `
      -- this is a comment
      ui.label("visible")
      -- another comment
    `;
    const result = parseLuauUi(source);
    expect(result.error).toBeNull();
    expect(result.nodes).toHaveLength(1);
    expect(result.nodes[0]?.props["text"]).toBe("visible");
  });
});
