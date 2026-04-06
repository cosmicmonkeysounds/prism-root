/**
 * Built-in lens definitions for Prism Studio.
 *
 * Each lens has a manifest (pure TS) and a component wrapper.
 * Components access data via the kernel context (useKernel).
 */

import type { ComponentType } from "react";
import type { LensManifest, LensRegistry, LensId } from "@prism/core/workspace";
import { lensId } from "@prism/core/workspace";
import { EditorPanel } from "../panels/editor-panel.js";
import { GraphPanel } from "../panels/graph-panel.js";
import { LayoutPanel } from "../panels/layout-panel.js";
import { CrdtPanel } from "../panels/crdt-panel.js";

export const EDITOR_LENS_ID = lensId("editor");
export const GRAPH_LENS_ID = lensId("graph");
export const LAYOUT_LENS_ID = lensId("layout");
export const CRDT_LENS_ID = lensId("crdt");

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

export const ALL_MANIFESTS: LensManifest[] = [
  editorManifest,
  graphManifest,
  layoutManifest,
  crdtManifest,
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
  map.set(CRDT_LENS_ID, CrdtPanel);
  return map;
}
