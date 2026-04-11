/**
 * PluginBundle — self-registering plugin interface.
 *
 * Each plugin bundle knows how to install itself into the ObjectRegistry
 * and PluginRegistry. The kernel just calls install() — the plugin handles
 * registering its own entity types, edge types, category rules, and plugin
 * contributions. No manual wiring needed.
 */

import type { ObjectRegistry } from "@prism/core/object-model";
import type { PluginRegistry } from "@prism/core/plugin";

export interface PluginInstallContext {
  readonly objectRegistry: ObjectRegistry;
  readonly pluginRegistry: PluginRegistry;
}

export interface PluginBundle {
  /** Unique bundle identifier. */
  readonly id: string;
  /** Human-readable name. */
  readonly name: string;
  /**
   * Install this plugin bundle: register entity defs, edge defs,
   * category rules, and plugin contributions into the given registries.
   * Returns an uninstall function.
   */
  install(ctx: PluginInstallContext): () => void;
}

/**
 * Install multiple plugin bundles into the registries.
 * Returns a single uninstall function that removes all.
 */
export function installPluginBundles(
  bundles: PluginBundle[],
  ctx: PluginInstallContext,
): () => void {
  const uninstalls = bundles.map((b) => b.install(ctx));
  return () => {
    for (const fn of uninstalls) fn();
  };
}
