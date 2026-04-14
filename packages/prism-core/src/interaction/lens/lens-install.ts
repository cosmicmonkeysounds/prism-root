/**
 * LensBundle — self-registering lens interface.
 *
 * Mirrors PluginBundle from kernel/plugin/plugin-install.ts. Each bundle
 * knows how to register its own manifest into a LensRegistry and its own
 * component into a component map. The host (e.g. studio-kernel) simply
 * calls install() — no manual wiring, no hardcoded lens lists.
 *
 * `@prism/core/lens` stays React-free: component and puck-config types are
 * type parameters so Studio can specialize with React's ComponentType and
 * `@measured/puck`'s field schemas while core remains pure TypeScript.
 *
 * ## Puck extension
 *
 * LensBundles may optionally declare a `puck` config. When present, the
 * `bindings/puck/lens-puck-adapter` reads it at kernel-boot time and
 * registers the lens as a Puck component, so the same lens can appear in
 * any Puck tree — including the Studio shell tree itself. Core carries
 * `puck` as an opaque generic so `@measured/puck` types never leak into
 * this layer; Studio specializes `TPuckConfig` to the real shape in its
 * own `lenses/bundle.ts`.
 *
 * Shell-only chrome (ActivityBar, Inspector, TabBar, …) that isn't
 * tab-switchable uses the sibling `ShellWidgetBundle` type below — same
 * Puck auto-registration, no LensManifest.
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

/**
 * A self-registering lens. Carries its manifest + component as plain data
 * so auxiliary registries (Puck, kbar, keybindings) can see them without
 * re-implementing install(); the `install()` method remains the one
 * authoritative path that wires the lens into `LensRegistry` +
 * `componentMap`.
 */
export interface LensBundle<TComponent = unknown, TPuckConfig = unknown> {
  /** Unique bundle identifier (typically matches the lens id). */
  readonly id: string;
  /** Human-readable name. */
  readonly name: string;
  /**
   * The lens's static manifest. Optional so hand-rolled test bundles can
   * skip it; every `defineLensBundle` output populates it.
   */
  readonly manifest?: LensManifest;
  /**
   * The React-like component that renders the lens. Optional for the same
   * reason as `manifest`. The Puck adapter skips bundles that don't carry
   * both.
   */
  readonly component?: TComponent;
  /**
   * Optional Puck authorability. When present, auxiliary adapters (see
   * `bindings/puck/lens-puck-adapter`) auto-register the lens as a Puck
   * component so it can be placed in any Puck tree. Opaque here to keep
   * core React/Puck-free; Studio specializes `TPuckConfig` to the real
   * Puck field shape.
   */
  readonly puck?: TPuckConfig;
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
  bundles: ReadonlyArray<LensBundle<TComponent>>,
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
 * An optional `puck` config marks the lens as Puck-authorable — it is
 * passed through opaquely so the adapter layer can read it.
 */
export function defineLensBundle<TComponent, TPuckConfig = unknown>(
  manifest: LensManifest,
  component: TComponent,
  puck?: TPuckConfig,
): LensBundle<TComponent, TPuckConfig> {
  return {
    id: manifest.id,
    name: manifest.name,
    manifest,
    component,
    ...(puck !== undefined ? { puck } : {}),
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

// ── ShellWidgetBundle ──────────────────────────────────────────────────────
//
// Chrome-only Puck component: has no LensManifest, no tab routing, no
// activity-bar icon — it just wants to appear in Puck trees as a drop
// target for the Studio shell (ActivityBar, InspectorPanel, TabBar, …).
//
// Symmetric with LensBundle so the same install flow handles both. `puck`
// is required here because a ShellWidgetBundle with no Puck config has no
// reason to exist.

export interface ShellWidgetInstallContext<TComponent = unknown> {
  readonly componentMap: Map<string, TComponent>;
}

export interface ShellWidgetBundle<TComponent = unknown, TPuckConfig = unknown> {
  readonly id: string;
  readonly name: string;
  readonly component: TComponent;
  readonly puck: TPuckConfig;
  install(ctx: ShellWidgetInstallContext<TComponent>): () => void;
}

export function installShellWidgetBundles<TComponent>(
  bundles: ReadonlyArray<ShellWidgetBundle<TComponent>>,
  ctx: ShellWidgetInstallContext<TComponent>,
): () => void {
  const uninstalls = bundles.map((b) => b.install(ctx));
  return () => {
    for (let i = uninstalls.length - 1; i >= 0; i--) {
      const fn = uninstalls[i];
      if (fn) fn();
    }
  };
}

/** Convenience constructor — mirrors `defineLensBundle`. */
export function defineShellWidgetBundle<TComponent, TPuckConfig>(opts: {
  id: string;
  name: string;
  component: TComponent;
  puck: TPuckConfig;
}): ShellWidgetBundle<TComponent, TPuckConfig> {
  return {
    id: opts.id,
    name: opts.name,
    component: opts.component,
    puck: opts.puck,
    install(ctx) {
      ctx.componentMap.set(opts.id, opts.component);
      return () => {
        ctx.componentMap.delete(opts.id);
      };
    },
  };
}
