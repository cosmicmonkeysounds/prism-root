/**
 * Built-in lens bundles for Prism Studio.
 *
 * Each panel file self-registers via `export const xxxLensBundle: LensBundle`,
 * mirroring the PluginBundle pattern in `@prism/core/plugins`. This file is
 * just the aggregator — it imports each bundle and exposes them to the
 * kernel via `createBuiltinLensBundles()`.
 *
 * Adding a new lens:
 *   1. Create `panels/my-panel.tsx` exporting `MyPanel` and `myLensBundle`.
 *   2. Add `myLensBundle` to the list below.
 *
 * No parallel manifest list, no parallel component map — one line per lens.
 */

import type { LensBundle } from "./bundle.js";

import { editorLensBundle, EDITOR_LENS_ID } from "../panels/editor-panel.js";
import { graphLensBundle } from "../panels/graph-panel.js";
import { layoutLensBundle } from "../panels/layout-panel.js";
import { canvasLensBundle } from "../panels/canvas-panel.js";
import { crdtLensBundle } from "../panels/crdt-panel.js";
import { relayLensBundle } from "../panels/relay-panel.js";
import { settingsLensBundle } from "../panels/settings-panel.js";
import { automationLensBundle } from "../panels/automation-panel.js";
import { analysisLensBundle } from "../panels/analysis-panel.js";
import { pluginLensBundle } from "../panels/plugin-panel.js";
import { shortcutsLensBundle } from "../panels/shortcuts-panel.js";
import { vaultLensBundle } from "../panels/vault-panel.js";
import { identityLensBundle } from "../panels/identity-panel.js";
import { assetsLensBundle } from "../panels/assets-panel.js";
import { trustLensBundle } from "../panels/trust-panel.js";
import { formFacetLensBundle } from "../panels/form-facet-panel.js";
import { tableFacetLensBundle } from "../panels/table-facet-panel.js";
import { sequencerLensBundle } from "../panels/sequencer-panel.js";
import { reportFacetLensBundle } from "../panels/report-facet-panel.js";
import { luauFacetLensBundle } from "../panels/luau-facet-panel.js";
import { facetDesignerLensBundle } from "../panels/facet-designer-panel.js";
import { spatialCanvasLensBundle } from "../panels/spatial-canvas-panel.js";
import { visualScriptLensBundle } from "../panels/visual-script-panel.js";
import { savedViewLensBundle } from "../panels/saved-view-panel.js";
import { valueListLensBundle } from "../panels/value-list-panel.js";
import { privilegeSetLensBundle } from "../panels/privilege-set-panel.js";
import { workLensBundle } from "../panels/work-panel.js";
import { financeLensBundle } from "../panels/finance-panel.js";
import { crmLensBundle } from "../panels/crm-panel.js";
import { lifeLensBundle } from "../panels/life-panel.js";
import { assetsMgmtLensBundle } from "../panels/assets-mgmt-panel.js";
import { platformLensBundle } from "../panels/platform-panel.js";
import { appBuilderLensBundle } from "../panels/app-builder-panel.js";
import { importLensBundle } from "../panels/import-panel.js";
import { publishLensBundle } from "../panels/publish-panel.js";
import { designTokensLensBundle } from "../panels/design-tokens-panel.js";
import { formBuilderLensBundle } from "../panels/form-builder-panel.js";
import { siteNavLensBundle } from "../panels/site-nav-panel.js";
import { sitemapLensBundle } from "../panels/sitemap-panel.js";
import { behaviorLensBundle } from "../panels/behavior-panel.js";
import { entityBuilderLensBundle } from "../panels/entity-builder-panel.js";
import { relationshipBuilderLensBundle } from "../panels/relationship-builder-panel.js";
import { schemaDesignerLensBundle } from "../panels/schema-designer-panel.js";
import { adminLensBundle } from "../panels/admin-panel.js";

// Re-export the default-tab lens id for App-level bootstrap. All other
// lens ids live next to their panel component.
export { EDITOR_LENS_ID };

export type { LensBundle, LensInstallContext } from "./bundle.js";
export { defineLensBundle } from "./bundle.js";

/** The canonical list of built-in Studio lens bundles. */
export function createBuiltinLensBundles(): LensBundle[] {
  return [
    editorLensBundle,
    graphLensBundle,
    layoutLensBundle,
    canvasLensBundle,
    crdtLensBundle,
    relayLensBundle,
    settingsLensBundle,
    automationLensBundle,
    analysisLensBundle,
    pluginLensBundle,
    shortcutsLensBundle,
    vaultLensBundle,
    identityLensBundle,
    assetsLensBundle,
    trustLensBundle,
    formFacetLensBundle,
    tableFacetLensBundle,
    sequencerLensBundle,
    reportFacetLensBundle,
    luauFacetLensBundle,
    facetDesignerLensBundle,
    spatialCanvasLensBundle,
    visualScriptLensBundle,
    savedViewLensBundle,
    valueListLensBundle,
    privilegeSetLensBundle,
    workLensBundle,
    financeLensBundle,
    crmLensBundle,
    lifeLensBundle,
    assetsMgmtLensBundle,
    platformLensBundle,
    appBuilderLensBundle,
    importLensBundle,
    publishLensBundle,
    designTokensLensBundle,
    formBuilderLensBundle,
    siteNavLensBundle,
    sitemapLensBundle,
    behaviorLensBundle,
    entityBuilderLensBundle,
    relationshipBuilderLensBundle,
    schemaDesignerLensBundle,
    adminLensBundle,
  ];
}
