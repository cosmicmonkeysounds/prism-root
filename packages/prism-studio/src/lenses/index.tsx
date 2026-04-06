/**
 * Built-in lens definitions for Prism Studio.
 *
 * Each lens has a manifest (pure TS) and a component wrapper.
 * Components access data via the kernel context (useKernel).
 */

import type { ComponentType } from "react";
import type { LensManifest, LensRegistry, LensId } from "@prism/core/lens";
import { lensId } from "@prism/core/lens";
import { EditorPanel } from "../panels/editor-panel.js";
import { GraphPanel } from "../panels/graph-panel.js";
import { LayoutPanel } from "../panels/layout-panel.js";
import { CrdtPanel } from "../panels/crdt-panel.js";
import { CanvasPanel } from "../panels/canvas-panel.js";
import { RelayPanel } from "../panels/relay-panel.js";

export const EDITOR_LENS_ID = lensId("editor");
export const GRAPH_LENS_ID = lensId("graph");
export const LAYOUT_LENS_ID = lensId("layout");
export const CANVAS_LENS_ID = lensId("canvas");
export const CRDT_LENS_ID = lensId("crdt");
export const RELAY_LENS_ID = lensId("relay");

const editorManifest: LensManifest = {
  id: EDITOR_LENS_ID,
  name: "Editor",
  icon: "\u270E",
  category: "editor",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-editor", name: "Switch to Editor", shortcut: ["e"], section: "Navigation" }],
  },
};

const graphManifest: LensManifest = {
  id: GRAPH_LENS_ID,
  name: "Graph",
  icon: "\u2B21",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-graph", name: "Switch to Graph", shortcut: ["g"], section: "Navigation" }],
  },
};

const layoutManifest: LensManifest = {
  id: LAYOUT_LENS_ID,
  name: "Layout",
  icon: "\u25A6",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-layout", name: "Switch to Layout Builder", shortcut: ["l"], section: "Navigation" }],
  },
};

const canvasManifest: LensManifest = {
  id: CANVAS_LENS_ID,
  name: "Canvas",
  icon: "\uD83D\uDDBC",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-canvas", name: "Switch to Canvas Preview", shortcut: ["v"], section: "Navigation" }],
  },
};

const crdtManifest: LensManifest = {
  id: CRDT_LENS_ID,
  name: "CRDT",
  icon: "\u29C9",
  category: "debug",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-crdt", name: "Switch to CRDT Inspector", shortcut: ["c"], section: "Navigation" }],
  },
};

const relayManifest: LensManifest = {
  id: RELAY_LENS_ID,
  name: "Relay",
  icon: "\u21C6",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-relay", name: "Switch to Relay Manager", shortcut: ["r"], section: "Navigation" }],
  },
};

export const ALL_MANIFESTS: LensManifest[] = [
  editorManifest,
  graphManifest,
  layoutManifest,
  canvasManifest,
  crdtManifest,
  relayManifest,
];

export function registerBuiltinLenses(registry: LensRegistry): () => void {
  const unsubs = ALL_MANIFESTS.map((m) => registry.register(m));
  return () => unsubs.forEach((fn) => fn());
}

export function createLensComponentMap(): Map<LensId, ComponentType> {
  const map = new Map<LensId, ComponentType>();
  map.set(EDITOR_LENS_ID, EditorPanel);
  map.set(GRAPH_LENS_ID, GraphPanel);
  map.set(LAYOUT_LENS_ID, LayoutPanel);
  map.set(CANVAS_LENS_ID, CanvasPanel);
  map.set(CRDT_LENS_ID, CrdtPanel);
  map.set(RELAY_LENS_ID, RelayPanel);
  return map;
}
