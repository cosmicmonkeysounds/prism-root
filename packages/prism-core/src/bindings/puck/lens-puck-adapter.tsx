/**
 * Lens → Puck auto-registration.
 *
 * Any `LensBundle` or `ShellWidgetBundle` that declares a `puck` config
 * becomes a Puck component when its host kernel boots, with zero
 * hand-written wrapper code per lens. The bundle's own component is used
 * as the Puck `render()` function, so the same React component that
 * renders the lens in its tab also renders the lens when dropped into a
 * Puck tree (e.g. the Studio shell tree itself).
 *
 * This file is the *only* place the lens system meets Puck. Everything
 * in `@prism/core/lens` stays React/Puck-free; this adapter lives in
 * `bindings/puck` because `bindings/` is the allowed layer for external
 * library imports.
 *
 * ## The `LensPuckConfig` contract
 *
 * Bundles carry `puck` as an opaque generic from core's POV, but this
 * adapter expects the shape below. Studio specialises `TPuckConfig` to
 * this same shape in `lenses/bundle.ts`.
 */

import type { ComponentConfig, ComponentData, Fields } from "@measured/puck";
import { createElement, type ComponentType, type ReactElement } from "react";
import type {
  LensBundle,
  ShellWidgetBundle,
} from "@prism/core/lens";
import type { EntityDef } from "@prism/core/object-model";
import { kebabToPascal } from "./component-registry.js";
import type { PuckComponentRegistry } from "./component-registry.js";

/**
 * Generic Puck config shape a LensBundle, ShellWidgetBundle, or EntityDef
 * can carry. Field types are kept permissive (`Fields<any>`) so authors
 * don't have to re-state their prop shapes in TS — the fields are the
 * schema.
 *
 * Three carrier shapes share this contract, and the adapter reads them
 * through a single code path:
 *
 *   - `LensBundle.puck`        — lens embedding (opt-in via `embeddable`)
 *   - `ShellWidgetBundle.puck` — shell chrome (embeddable by default)
 *   - `EntityDef.puck`         — entity-as-Puck-component (the dominant
 *                                case — content blocks, widgets, shells)
 *
 * For LensBundle / ShellWidgetBundle, `render` is usually omitted and
 * the bundle's `component` is used. For EntityDef, `render` is required
 * — an EntityDef has no separate component slot.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export interface LensPuckConfig<P extends Record<string, any> = Record<string, any>> {
  /** Human label in palette. Defaults to bundle/entity name. */
  label?: string;
  /** Palette category (`"Shell"`, `"Lens"`, `"Chrome"`, …). */
  category?: string;
  /** Puck field schema for inspector-editable props. */
  fields?: Fields<P>;
  /** Default props for new instances. */
  defaultProps?: Partial<P>;
  /**
   * Whether this lens/widget can be placed in any Puck tree. Defaults to
   * `true` for ShellWidgetBundles and `false` for LensBundles — lenses
   * are tab-routed by default and only become embeddable when their
   * author opts in. Ignored for EntityDefs (entities are always
   * embeddable when they declare a `puck` block).
   */
  embeddable?: boolean;
  /**
   * Names of `<LensZone>` drop-zones exposed inside this lens/widget.
   * Informational — documented for the palette UI; Puck's own slot
   * mechanism still owns the runtime wiring.
   */
  zones?: readonly string[];
  /**
   * Render function for Puck. When present, overrides a bundle's
   * `component` — EntityDef consumers use this to attach their render
   * directly to the schema. For LensBundle / ShellWidgetBundle it is
   * optional and rarely set (the bundle's `component` is preferred).
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  render?: NonNullable<ComponentConfig<any>["render"]>;
}

/** Registration result for one adapter call, useful in tests. */
export interface LensPuckRegistration {
  /** PascalCase Puck component name. */
  readonly name: string;
  /** Whether the bundle was embeddable (and actually registered). */
  readonly registered: boolean;
  /** Why a bundle was skipped, if it was. */
  readonly skipReason?: "not-embeddable" | "no-puck" | "no-component";
}

/**
 * The single low-level builder: take a `LensPuckConfig` plus a
 * fallback label and (optionally) a fallback component, and produce
 * a Puck `ComponentConfig`.
 *
 * Every higher-level registration path — LensBundle, ShellWidgetBundle,
 * EntityDef, and the two built-in shell primitives — reduces to a call
 * into this function, so there is exactly one place the shape of a
 * `ComponentConfig` is assembled.
 */
export function puckConfigToComponentConfig(
  puck: LensPuckConfig,
  fallbackLabel: string,
  fallbackComponent?: ComponentType,
): ComponentConfig {
  const render: ComponentConfig["render"] =
    (puck.render as ComponentConfig["render"] | undefined) ??
    (fallbackComponent !== undefined
      ? ((props: Record<string, unknown>): ReactElement => {
          const { puck: _puckCtx, editMode: _editMode, ...rest } = props as {
            puck?: unknown;
            editMode?: unknown;
          } & Record<string, unknown>;
          void _puckCtx;
          void _editMode;
          return createElement(fallbackComponent, rest);
        }) as ComponentConfig["render"]
      : undefined) as ComponentConfig["render"];
  if (!render) {
    throw new Error(
      `puckConfigToComponentConfig: '${fallbackLabel}' has no render and no fallbackComponent`,
    );
  }
  const config: ComponentConfig = {
    label: puck.label ?? fallbackLabel,
    fields: (puck.fields ?? {}) as Fields,
    render,
  };
  if (puck.defaultProps !== undefined) {
    (config as { defaultProps?: unknown }).defaultProps = puck.defaultProps;
  }
  return config;
}

function bundleToComponentConfig(
  bundle: {
    id: string;
    name: string;
    component: ComponentType;
    puck: LensPuckConfig;
  },
): ComponentConfig {
  return puckConfigToComponentConfig(bundle.puck, bundle.name, bundle.component);
}

/**
 * Register any LensBundles with `puck` config + a concrete component as
 * Puck direct components in `registry`. Lenses default to
 * `embeddable: false` — they only become Puck-placeable when the author
 * explicitly opts in.
 *
 * Returns the list of registrations (including skip reasons) so tests
 * and diagnostics can inspect the outcome.
 */
export function registerLensBundlesInPuck<TKernel>(
  bundles: ReadonlyArray<LensBundle<ComponentType, LensPuckConfig>>,
  registry: PuckComponentRegistry<TKernel>,
): LensPuckRegistration[] {
  const out: LensPuckRegistration[] = [];
  for (const bundle of bundles) {
    if (!bundle.puck) {
      out.push({ name: kebabToPascal(bundle.id), registered: false, skipReason: "no-puck" });
      continue;
    }
    if (!bundle.component) {
      out.push({ name: kebabToPascal(bundle.id), registered: false, skipReason: "no-component" });
      continue;
    }
    if (bundle.puck.embeddable !== true) {
      out.push({ name: kebabToPascal(bundle.id), registered: false, skipReason: "not-embeddable" });
      continue;
    }
    const name = kebabToPascal(bundle.id);
    registry.registerDirect(
      name,
      bundleToComponentConfig({
        id: bundle.id,
        name: bundle.name,
        component: bundle.component,
        puck: bundle.puck,
      }),
    );
    out.push({ name, registered: true });
  }
  return out;
}

/**
 * Register ShellWidgetBundles as Puck direct components. Unlike lens
 * bundles, shell widgets are embeddable by default (that's the whole
 * reason they exist) — `embeddable: false` can still disable a widget
 * temporarily.
 */
export function registerShellWidgetBundlesInPuck<TKernel>(
  bundles: ReadonlyArray<ShellWidgetBundle<ComponentType, LensPuckConfig>>,
  registry: PuckComponentRegistry<TKernel>,
): LensPuckRegistration[] {
  const out: LensPuckRegistration[] = [];
  for (const bundle of bundles) {
    if (bundle.puck.embeddable === false) {
      out.push({ name: kebabToPascal(bundle.id), registered: false, skipReason: "not-embeddable" });
      continue;
    }
    const name = kebabToPascal(bundle.id);
    registry.registerDirect(
      name,
      bundleToComponentConfig({
        id: bundle.id,
        name: bundle.name,
        component: bundle.component,
        puck: bundle.puck,
      }),
    );
    out.push({ name, registered: true });
  }
  return out;
}

/**
 * Register every `EntityDef` that carries a `puck` block as a Puck
 * direct component in `registry`. This is the unified pipeline for
 * entity-driven Puck components — the old `PuckComponentProvider`
 * interface has been retired.
 *
 * Entities without a `puck` block are skipped silently (they're schema-
 * only objects like folders, pages, records). Naming follows the same
 * `kebabToPascal(def.type)` convention used for lens/widget bundles so
 * stored Puck trees reference components by a single predictable name.
 */
export function registerEntityDefsInPuck(
  defs: ReadonlyArray<EntityDef<unknown, LensPuckConfig>>,
  registry: PuckComponentRegistry<unknown>,
): LensPuckRegistration[] {
  const out: LensPuckRegistration[] = [];
  for (const def of defs) {
    const name = kebabToPascal(def.type);
    if (!def.puck) {
      out.push({ name, registered: false, skipReason: "no-puck" });
      continue;
    }
    if (!def.puck.render) {
      out.push({ name, registered: false, skipReason: "no-component" });
      continue;
    }
    registry.registerDirect(
      name,
      puckConfigToComponentConfig(def.puck, def.label),
    );
    out.push({ name, registered: true });
  }
  return out;
}

/**
 * Structural type-only reference used by tests to ensure the adapter
 * signature stays compatible with `ComponentData` from Puck. Having this
 * in the file keeps the `import type` above non-dead.
 */
export type _PuckComponentDataRef = ComponentData;
