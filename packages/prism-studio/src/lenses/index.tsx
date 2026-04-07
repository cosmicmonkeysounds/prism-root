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
import { SettingsPanel } from "../panels/settings-panel.js";
import { AutomationPanel } from "../panels/automation-panel.js";
import { AnalysisPanel } from "../panels/analysis-panel.js";
import { PluginPanel } from "../panels/plugin-panel.js";
import { ShortcutsPanel } from "../panels/shortcuts-panel.js";
import { VaultPanel } from "../panels/vault-panel.js";
import { IdentityPanel } from "../panels/identity-panel.js";
import { AssetsPanel } from "../panels/assets-panel.js";
import { TrustPanel } from "../panels/trust-panel.js";
import { FormFacetPanel } from "../panels/form-facet-panel.js";
import { TableFacetPanel } from "../panels/table-facet-panel.js";
import { SequencerPanel } from "../panels/sequencer-panel.js";

export const EDITOR_LENS_ID = lensId("editor");
export const GRAPH_LENS_ID = lensId("graph");
export const LAYOUT_LENS_ID = lensId("layout");
export const CANVAS_LENS_ID = lensId("canvas");
export const CRDT_LENS_ID = lensId("crdt");
export const RELAY_LENS_ID = lensId("relay");
export const SETTINGS_LENS_ID = lensId("settings");
export const AUTOMATION_LENS_ID = lensId("automation");
export const ANALYSIS_LENS_ID = lensId("analysis");
export const PLUGIN_LENS_ID = lensId("plugin");
export const SHORTCUTS_LENS_ID = lensId("shortcuts");
export const VAULT_LENS_ID = lensId("vault");
export const IDENTITY_LENS_ID = lensId("identity");
export const ASSETS_LENS_ID = lensId("assets");
export const TRUST_LENS_ID = lensId("trust");
export const FORM_FACET_LENS_ID = lensId("form-facet");
export const TABLE_FACET_LENS_ID = lensId("table-facet");
export const SEQUENCER_LENS_ID = lensId("sequencer");

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

const settingsManifest: LensManifest = {
  id: SETTINGS_LENS_ID,
  name: "Settings",
  icon: "\u2699",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-settings", name: "Switch to Settings", shortcut: [","], section: "Navigation" }],
  },
};

const automationManifest: LensManifest = {
  id: AUTOMATION_LENS_ID,
  name: "Automation",
  icon: "\u26A1",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-automation", name: "Switch to Automation", shortcut: ["a"], section: "Navigation" }],
  },
};

const analysisManifest: LensManifest = {
  id: ANALYSIS_LENS_ID,
  name: "Analysis",
  icon: "\uD83D\uDCC8",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-analysis", name: "Switch to Analysis", shortcut: ["n"], section: "Navigation" }],
  },
};

const pluginManifest: LensManifest = {
  id: PLUGIN_LENS_ID,
  name: "Plugins",
  icon: "\uD83E\uDDE9",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-plugins", name: "Switch to Plugins", shortcut: ["p"], section: "Navigation" }],
  },
};

const shortcutsManifest: LensManifest = {
  id: SHORTCUTS_LENS_ID,
  name: "Shortcuts",
  icon: "\u2328",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-shortcuts", name: "Switch to Shortcuts", shortcut: ["k"], section: "Navigation" }],
  },
};

const vaultManifest: LensManifest = {
  id: VAULT_LENS_ID,
  name: "Vaults",
  icon: "\uD83D\uDD12",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-vaults", name: "Switch to Vaults", shortcut: ["w"], section: "Navigation" }],
  },
};

const identityManifest: LensManifest = {
  id: IDENTITY_LENS_ID,
  name: "Identity",
  icon: "\uD83D\uDD11",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-identity", name: "Switch to Identity", shortcut: ["i"], section: "Navigation" }],
  },
};

const assetsManifest: LensManifest = {
  id: ASSETS_LENS_ID,
  name: "Assets",
  icon: "\uD83D\uDCC1",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-assets", name: "Switch to Assets", shortcut: ["f"], section: "Navigation" }],
  },
};

const trustManifest: LensManifest = {
  id: TRUST_LENS_ID,
  name: "Trust",
  icon: "\uD83D\uDEE1",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-trust", name: "Switch to Trust", shortcut: ["t"], section: "Navigation" }],
  },
};

const formFacetManifest: LensManifest = {
  id: FORM_FACET_LENS_ID,
  name: "Form",
  icon: "\uD83D\uDCDD",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-form-facet", name: "Switch to Form Facet", shortcut: ["d"], section: "Navigation" }],
  },
};

const tableFacetManifest: LensManifest = {
  id: TABLE_FACET_LENS_ID,
  name: "Table",
  icon: "\uD83D\uDCCA",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-table-facet", name: "Switch to Table Facet", shortcut: ["b"], section: "Navigation" }],
  },
};

const sequencerManifest: LensManifest = {
  id: SEQUENCER_LENS_ID,
  name: "Sequencer",
  icon: "\uD83C\uDFBC",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-sequencer", name: "Switch to Sequencer", shortcut: ["q"], section: "Navigation" }],
  },
};

export const ALL_MANIFESTS: LensManifest[] = [
  editorManifest,
  graphManifest,
  layoutManifest,
  canvasManifest,
  crdtManifest,
  relayManifest,
  settingsManifest,
  automationManifest,
  analysisManifest,
  pluginManifest,
  shortcutsManifest,
  vaultManifest,
  identityManifest,
  assetsManifest,
  trustManifest,
  formFacetManifest,
  tableFacetManifest,
  sequencerManifest,
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
  map.set(SETTINGS_LENS_ID, SettingsPanel);
  map.set(AUTOMATION_LENS_ID, AutomationPanel);
  map.set(ANALYSIS_LENS_ID, AnalysisPanel);
  map.set(PLUGIN_LENS_ID, PluginPanel);
  map.set(SHORTCUTS_LENS_ID, ShortcutsPanel);
  map.set(VAULT_LENS_ID, VaultPanel);
  map.set(IDENTITY_LENS_ID, IdentityPanel);
  map.set(ASSETS_LENS_ID, AssetsPanel);
  map.set(TRUST_LENS_ID, TrustPanel);
  map.set(FORM_FACET_LENS_ID, FormFacetPanel);
  map.set(TABLE_FACET_LENS_ID, TableFacetPanel);
  map.set(SEQUENCER_LENS_ID, SequencerPanel);
  return map;
}
