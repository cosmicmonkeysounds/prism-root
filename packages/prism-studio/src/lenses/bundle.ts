/**
 * Studio-specific lens bundle types.
 *
 * Specializes the generic `@prism/core/lens` bundle types with React's
 * `ComponentType` + the concrete `LensPuckConfig` so panels can export
 * bundles without repeating the generic parameters at every call site.
 */

import type { ComponentType } from "react";
import type {
  LensBundle as LensBundleBase,
  LensInstallContext as LensInstallContextBase,
  ShellWidgetBundle as ShellWidgetBundleBase,
  ShellWidgetInstallContext as ShellWidgetInstallContextBase,
} from "@prism/core/lens";
import {
  defineLensBundle as defineLensBundleBase,
  defineShellWidgetBundle as defineShellWidgetBundleBase,
} from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import type { LensPuckConfig } from "@prism/core/puck";

export type LensBundle = LensBundleBase<ComponentType, LensPuckConfig>;
export type LensInstallContext = LensInstallContextBase<ComponentType>;
export type ShellWidgetBundle = ShellWidgetBundleBase<ComponentType, LensPuckConfig>;
export type ShellWidgetInstallContext = ShellWidgetInstallContextBase<ComponentType>;

/**
 * Studio-specialized `defineLensBundle`. Accepts an optional `puck`
 * config so panels can opt into Puck embedding with a single line.
 */
export function defineLensBundle(
  manifest: LensManifest,
  component: ComponentType,
  puck?: LensPuckConfig,
): LensBundle {
  return defineLensBundleBase<ComponentType, LensPuckConfig>(
    manifest,
    component,
    puck,
  );
}

/** Studio-specialized `defineShellWidgetBundle`. */
export function defineShellWidgetBundle(opts: {
  id: string;
  name: string;
  component: ComponentType;
  puck: LensPuckConfig;
}): ShellWidgetBundle {
  return defineShellWidgetBundleBase<ComponentType, LensPuckConfig>(opts);
}
