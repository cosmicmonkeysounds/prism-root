// ── Plugin Bundles ────────────────────────────────────────────────────────
// Each plugin extends the Flux domain with new entity types, edge relations,
// automation presets, and a PrismPlugin definition with views/commands.
// Plugins self-register via the PluginBundle.install() pattern.

export type { PluginBundle, PluginInstallContext } from "./plugin-install.js";
export { installPluginBundles } from "./plugin-install.js";

export * from "./work/index.js";
export * from "./finance/index.js";
export * from "./crm/index.js";
export * from "./life/index.js";
export * from "./assets/index.js";
export * from "./platform/index.js";

// ── Convenience: all built-in bundles ────────────────────────────────────

import { createWorkBundle } from "./work/index.js";
import { createFinanceBundle } from "./finance/index.js";
import { createCrmBundle } from "./crm/index.js";
import { createLifeBundle } from "./life/index.js";
import { createAssetsBundle } from "./assets/index.js";
import { createPlatformBundle } from "./platform/index.js";
import type { PluginBundle } from "./plugin-install.js";

/** Create all built-in plugin bundles. */
export function createBuiltinBundles(): PluginBundle[] {
  return [
    createWorkBundle(),
    createFinanceBundle(),
    createCrmBundle(),
    createLifeBundle(),
    createAssetsBundle(),
    createPlatformBundle(),
  ];
}
