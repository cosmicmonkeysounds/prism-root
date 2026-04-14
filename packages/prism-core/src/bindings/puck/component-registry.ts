/**
 * Puck Component Registry — DI seam for builder components.
 *
 * See ADR-004 for background. The registry is the extensibility hook that
 * lets packages outside `prism-studio` contribute Puck components. It is
 * strictly additive: anything not registered falls through to whatever
 * caller-owned default path exists (e.g. the generic entity→component
 * generator in studio's layout panel).
 *
 * The registry is generic over the host kernel type so core does not have
 * to import from studio. Providers receive the raw `EntityDef` and a
 * strongly-typed kernel reference, and return a ready-to-insert Puck
 * `ComponentConfig`.
 *
 * Naming convention: entity types are kebab-case (`record-list`), but
 * Puck component names are PascalCase (`RecordList`). The registry takes
 * kebab-case in `register()` and emits PascalCase keys in `buildComponents()`
 * so both conventions stay at their own layers.
 */

import type { ComponentConfig } from "@measured/puck";
import type { EntityDef } from "@prism/core/object-model";

/** Context handed to a provider at config-build time. */
export interface ProviderContext<TKernel> {
  readonly def: EntityDef;
  readonly kernel: TKernel;
}

/**
 * A provider turns one entity type into a Puck `ComponentConfig`.
 *
 * Providers are plain data-and-function objects — no base class, no
 * lifecycle. Most providers close over imported renderers and field
 * factories and return a static config.
 */
export interface PuckComponentProvider<TKernel = unknown> {
  /** Entity type this provider handles, in kebab-case (matches `EntityDef.type`). */
  readonly type: string;
  /** Build the Puck config entry for this entity type. */
  buildConfig(ctx: ProviderContext<TKernel>): ComponentConfig;
}

/** Convert `record-list` → `RecordList`. Exported so callers can match Puck keys. */
export function kebabToPascal(s: string): string {
  return s
    .split("-")
    .map((w) => (w.length > 0 ? w.charAt(0).toUpperCase() + w.slice(1) : w))
    .join("");
}

/**
 * Holds providers keyed by entity type and builds Puck component maps on
 * demand. Construction is side-effect free — registration happens via
 * `.register()` chaining. Safe to share across panels.
 */
export class PuckComponentRegistry<TKernel = unknown> {
  private readonly providers = new Map<string, PuckComponentProvider<TKernel>>();

  /** Register a single provider. Later registrations override earlier ones. */
  register(provider: PuckComponentProvider<TKernel>): this {
    this.providers.set(provider.type, provider);
    return this;
  }

  /** Register multiple providers at once. */
  registerAll(
    providers: ReadonlyArray<PuckComponentProvider<TKernel>>,
  ): this {
    for (const p of providers) this.register(p);
    return this;
  }

  /** Remove a provider. Mainly used in tests. */
  unregister(type: string): boolean {
    return this.providers.delete(type);
  }

  has(type: string): boolean {
    return this.providers.has(type);
  }

  get(type: string): PuckComponentProvider<TKernel> | undefined {
    return this.providers.get(type);
  }

  /** Iterate over registered entity types. */
  types(): string[] {
    return Array.from(this.providers.keys());
  }

  /**
   * Build a Puck component map from the supplied entity defs. Only defs
   * that have a registered provider contribute entries — the caller is
   * expected to merge the result with whatever default-path components it
   * builds itself (e.g. from a generic entity→component generator).
   *
   * Keys in the returned record are PascalCase (Puck's component-name
   * convention); values are the `ComponentConfig` returned by each
   * provider.
   */
  buildComponents(opts: {
    defs: ReadonlyArray<EntityDef>;
    kernel: TKernel;
  }): Record<string, ComponentConfig> {
    const out: Record<string, ComponentConfig> = {};
    for (const def of opts.defs) {
      const provider = this.providers.get(def.type);
      if (!provider) continue;
      out[kebabToPascal(def.type)] = provider.buildConfig({
        def,
        kernel: opts.kernel,
      });
    }
    return out;
  }
}

/** Convenience factory — equivalent to `new PuckComponentRegistry<TKernel>()`. */
export function createPuckComponentRegistry<
  TKernel = unknown,
>(): PuckComponentRegistry<TKernel> {
  return new PuckComponentRegistry<TKernel>();
}
