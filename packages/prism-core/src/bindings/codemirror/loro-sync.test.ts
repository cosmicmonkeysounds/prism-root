/**
 * @vitest-environment jsdom
 */
import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { loroSync, createLoroTextDoc } from "./loro-sync.js";

describe("loroSync extension", () => {
  it("should initialize CM with LoroText content", () => {
    const { doc, text } = createLoroTextDoc("test", "Hello World");

    const state = EditorState.create({
      doc: text.toString(),
      extensions: [loroSync({ doc, text })],
    });

    expect(state.doc.toString()).toBe("Hello World");
  });

  it("should propagate CM edits to LoroText", () => {
    const { doc, text } = createLoroTextDoc("test", "Hello");

    const state = EditorState.create({
      doc: text.toString(),
      extensions: [loroSync({ doc, text })],
    });

    const container = document.createElement("div");
    const view = new EditorView({ state, parent: container });

    // Simulate a user typing " World"
    view.dispatch({
      changes: { from: 5, insert: " World" },
    });

    expect(text.toString()).toBe("Hello World");
    view.destroy();
  });

  it("should propagate CM deletions to LoroText", () => {
    const { doc, text } = createLoroTextDoc("test", "Hello World");

    const state = EditorState.create({
      doc: text.toString(),
      extensions: [loroSync({ doc, text })],
    });

    const container = document.createElement("div");
    const view = new EditorView({ state, parent: container });

    // Delete " World"
    view.dispatch({
      changes: { from: 5, to: 11 },
    });

    expect(text.toString()).toBe("Hello");
    view.destroy();
  });

  it("should propagate CM replacements to LoroText", () => {
    const { doc, text } = createLoroTextDoc("test", "Hello World");

    const state = EditorState.create({
      doc: text.toString(),
      extensions: [loroSync({ doc, text })],
    });

    const container = document.createElement("div");
    const view = new EditorView({ state, parent: container });

    // Replace "World" with "Prism"
    view.dispatch({
      changes: { from: 6, to: 11, insert: "Prism" },
    });

    expect(text.toString()).toBe("Hello Prism");
    view.destroy();
  });
});

describe("createLoroTextDoc", () => {
  it("should create a doc with a named text and initial content", () => {
    const { doc, text } = createLoroTextDoc("content", "Initial");
    expect(text.toString()).toBe("Initial");
    // Verify it's in the doc
    expect(doc.getText("content").toString()).toBe("Initial");
  });

  it("should create empty text when no initial content", () => {
    const { text } = createLoroTextDoc("empty");
    expect(text.toString()).toBe("");
  });
});
