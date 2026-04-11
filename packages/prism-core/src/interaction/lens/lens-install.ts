/**
 * LensBundle — self-registering lens interface.
 *
 * Mirrors PluginBundle from kernel/plugin/plugin-install.ts. Each bundle
 * knows how to register its own manifest into a LensRegistry and its own
 * component into a component map. The host (e.g. studio-kernel) simply
 * calls install() — no manual wiring, no hardcoded lens lists.
 *
 * `@prism/core/lens` stays React-free: the component type is a type parameter
 * so Studio can specialize with React's ComponentType while core remains
 * pure TypeScript.
 */

import type { LensId, LensManifest } from "./lens-types.js";
import type { LensRegistry } from "./lens-registry.js";

/**
 * Context passed to a LensBundle's install() call.
 *
 * @typeParam TComponent - The component type used by the host. Layer 1
 * is React-agnostic; Studio specializes with React's ComponentType.
 */
export interface LensInstallContext<TComponent = unknown> {
  readonly lensRegistry: LensRegistry;
  readonly componentMap: Map<LensId, TComponent>;
}

export interface LensBundle<TComponent = unknown> {
  /** Unique bundle identifier (typically matches the lens id). */
  readonly id: string;
  /** Human-readable name. */
  readonly name: string;
  /**
   * Install this lens bundle: register the manifest into the lens
   * registry and the component into the component map. Returns an
   * uninstall function that removes both.
   */
  install(ctx: LensInstallContext<TComponent>): () => void;
}

/**
 * Install multiple lens bundles into the given context.
 * Returns a single uninstall function that tears all of them down
 * in reverse order of installation.
 */
export function installLensBundles<TComponent>(
  bundles: LensBundle<TComponent>[],
  ctx: LensInstallContext<TComponent>,
): () => void {
  const uninstalls = bundles.map((b) => b.install(ctx));
  return () => {
    for (let i = uninstalls.length - 1; i >= 0; i--) {
      const fn = uninstalls[i];
      if (fn) fn();
    }
  };
}

/**
 * Convenience helper: build a LensBundle from a manifest + component
 * pair. Handles install/uninstall wiring so panel files can stay terse.
 */
export function defineLensBundle<TComponent>(
  manifest: LensManifest,
  component: TComponent,
): LensBundle<TComponent> {
  return {
    id: manifest.id,
    name: manifest.name,
    install(ctx) {
      const unregister = ctx.lensRegistry.register(manifest);
      ctx.componentMap.set(manifest.id, component);
      return () => {
        unregister();
        ctx.componentMap.delete(manifest.id);
      };
    },
  };
}
