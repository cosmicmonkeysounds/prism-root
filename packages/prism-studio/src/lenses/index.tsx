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
import ReportFacetPanel from "../panels/report-facet-panel.js";
import { LuaFacetPanel } from "../panels/lua-facet-panel.js";
import { FacetDesignerPanel } from "../panels/facet-designer-panel.js";
import { RecordBrowserPanel } from "../panels/record-browser-panel.js";
import { SpatialCanvasPanel } from "../panels/spatial-canvas-panel.js";
import { VisualScriptPanel } from "../panels/visual-script-panel.js";
import { SavedViewPanel } from "../panels/saved-view-panel.js";
import { ValueListPanel } from "../panels/value-list-panel.js";
import { PrivilegeSetPanel } from "../panels/privilege-set-panel.js";
import { WorkPanel } from "../panels/work-panel.js";
import { FinancePanel } from "../panels/finance-panel.js";
import { CrmPanel } from "../panels/crm-panel.js";
import { LifePanel } from "../panels/life-panel.js";
import { AssetsMgmtPanel } from "../panels/assets-mgmt-panel.js";
import { PlatformPanel } from "../panels/platform-panel.js";

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
export const REPORT_FACET_LENS_ID = lensId("report-facet");
export const LUA_FACET_LENS_ID = lensId("lua-facet");
export const FACET_DESIGNER_LENS_ID = lensId("facet-designer");
export const RECORD_BROWSER_LENS_ID = lensId("record-browser");
export const SPATIAL_CANVAS_LENS_ID = lensId("spatial-canvas");
export const VISUAL_SCRIPT_LENS_ID = lensId("visual-script");
export const SAVED_VIEW_LENS_ID = lensId("saved-view");
export const VALUE_LIST_LENS_ID = lensId("value-list");
export const PRIVILEGE_SET_LENS_ID = lensId("privilege-set");
export const WORK_LENS_ID = lensId("work");
export const FINANCE_LENS_ID = lensId("finance");
export const CRM_LENS_ID = lensId("crm");
export const LIFE_LENS_ID = lensId("life");
export const ASSETS_MGMT_LENS_ID = lensId("assets-mgmt");
export const PLATFORM_LENS_ID = lensId("platform");

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
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-form-facet", name: "Switch to Form Facet", shortcut: ["d"], section: "Navigation" }],
  },
};

const tableFacetManifest: LensManifest = {
  id: TABLE_FACET_LENS_ID,
  name: "Table",
  icon: "\uD83D\uDCCA",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-table-facet", name: "Switch to Table Facet", shortcut: ["b"], section: "Navigation" }],
  },
};

const sequencerManifest: LensManifest = {
  id: SEQUENCER_LENS_ID,
  name: "Sequencer",
  icon: "\uD83C\uDFBC",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-sequencer", name: "Switch to Sequencer", shortcut: ["q"], section: "Navigation" }],
  },
};

const luaFacetManifest: LensManifest = {
  id: LUA_FACET_LENS_ID,
  name: "Lua Facet",
  icon: "\uD83C\uDF19",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-lua-facet", name: "Switch to Lua Facet", shortcut: ["u"], section: "Navigation" }],
  },
};

const reportFacetManifest: LensManifest = {
  id: REPORT_FACET_LENS_ID,
  name: "Report",
  icon: "\uD83D\uDCCB",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-report-facet", name: "Switch to Report Facet", shortcut: ["o"], section: "Navigation" }],
  },
};

const facetDesignerManifest: LensManifest = {
  id: FACET_DESIGNER_LENS_ID,
  name: "Facet Designer",
  icon: "\u{1F3A8}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-facet-designer", name: "Switch to Facet Designer", shortcut: ["x"], section: "Navigation" }],
  },
};

const recordBrowserManifest: LensManifest = {
  id: RECORD_BROWSER_LENS_ID,
  name: "Record Browser",
  icon: "\u{1F4D6}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-record-browser", name: "Switch to Record Browser", shortcut: ["z"], section: "Navigation" }],
  },
};

const spatialCanvasManifest: LensManifest = {
  id: SPATIAL_CANVAS_LENS_ID,
  name: "Spatial Canvas",
  icon: "\uD83D\uDCD0",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-spatial-canvas", name: "Switch to Spatial Canvas Editor", shortcut: ["shift+x"], section: "Navigation" }],
  },
};

const visualScriptManifest: LensManifest = {
  id: VISUAL_SCRIPT_LENS_ID,
  name: "Visual Script",
  icon: "\u{1F9E9}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-visual-script", name: "Switch to Visual Script Editor", shortcut: ["shift+s"], section: "Navigation" }],
  },
};

const savedViewManifest: LensManifest = {
  id: SAVED_VIEW_LENS_ID,
  name: "Saved Views",
  icon: "\u{1F516}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-saved-view", name: "Switch to Saved Views", shortcut: ["shift+v"], section: "Navigation" }],
  },
};

const valueListManifest: LensManifest = {
  id: VALUE_LIST_LENS_ID,
  name: "Value Lists",
  icon: "\u{1F4C3}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-value-list", name: "Switch to Value List Editor", shortcut: ["shift+l"], section: "Navigation" }],
  },
};

const privilegeSetManifest: LensManifest = {
  id: PRIVILEGE_SET_LENS_ID,
  name: "Privilege Sets",
  icon: "\u{1F46E}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-privilege-set", name: "Switch to Privilege Set Manager", shortcut: ["shift+p"], section: "Navigation" }],
  },
};

const workManifest: LensManifest = {
  id: WORK_LENS_ID,
  name: "Work",
  icon: "\u{1F4BC}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-work", name: "Switch to Work", shortcut: ["shift+w"], section: "Navigation" }],
  },
};

const financeManifest: LensManifest = {
  id: FINANCE_LENS_ID,
  name: "Finance",
  icon: "\u{1F4B0}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-finance", name: "Switch to Finance", shortcut: ["shift+f"], section: "Navigation" }],
  },
};

const crmManifest: LensManifest = {
  id: CRM_LENS_ID,
  name: "CRM",
  icon: "\u{1F465}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-crm", name: "Switch to CRM", shortcut: ["shift+c"], section: "Navigation" }],
  },
};

const lifeManifest: LensManifest = {
  id: LIFE_LENS_ID,
  name: "Life",
  icon: "\u{1F33F}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-life", name: "Switch to Life", shortcut: ["shift+h"], section: "Navigation" }],
  },
};

const assetsMgmtManifest: LensManifest = {
  id: ASSETS_MGMT_LENS_ID,
  name: "Asset Manager",
  icon: "\u{1F4E6}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-assets-mgmt", name: "Switch to Asset Manager", shortcut: ["shift+m"], section: "Navigation" }],
  },
};

const platformManifest: LensManifest = {
  id: PLATFORM_LENS_ID,
  name: "Platform",
  icon: "\u{1F4E1}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-platform", name: "Switch to Platform", shortcut: ["shift+i"], section: "Navigation" }],
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
  reportFacetManifest,
  luaFacetManifest,
  facetDesignerManifest,
  recordBrowserManifest,
  spatialCanvasManifest,
  visualScriptManifest,
  savedViewManifest,
  valueListManifest,
  privilegeSetManifest,
  workManifest,
  financeManifest,
  crmManifest,
  lifeManifest,
  assetsMgmtManifest,
  platformManifest,
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
  map.set(REPORT_FACET_LENS_ID, ReportFacetPanel);
  map.set(LUA_FACET_LENS_ID, LuaFacetPanel);
  map.set(FACET_DESIGNER_LENS_ID, FacetDesignerPanel);
  map.set(RECORD_BROWSER_LENS_ID, RecordBrowserPanel);
  map.set(SPATIAL_CANVAS_LENS_ID, SpatialCanvasPanel);
  map.set(VISUAL_SCRIPT_LENS_ID, VisualScriptPanel);
  map.set(SAVED_VIEW_LENS_ID, SavedViewPanel);
  map.set(VALUE_LIST_LENS_ID, ValueListPanel);
  map.set(PRIVILEGE_SET_LENS_ID, PrivilegeSetPanel);
  map.set(WORK_LENS_ID, WorkPanel);
  map.set(FINANCE_LENS_ID, FinancePanel);
  map.set(CRM_LENS_ID, CrmPanel);
  map.set(LIFE_LENS_ID, LifePanel);
  map.set(ASSETS_MGMT_LENS_ID, AssetsMgmtPanel);
  map.set(PLATFORM_LENS_ID, PlatformPanel);
  return map;
}
