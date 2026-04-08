/**
 * Studio-specific lens bundle types.
 *
 * Specializes the generic `@prism/core/lens` bundle types with React's
 * ComponentType so panels can export bundles without repeating the
 * generic parameter at every call site.
 */

import type { ComponentType } from "react";
import type {
  LensBundle as LensBundleBase,
  LensInstallContext as LensInstallContextBase,
} from "@prism/core/lens";
import { defineLensBundle as defineLensBundleBase } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";

export type LensBundle = LensBundleBase<ComponentType>;
export type LensInstallContext = LensInstallContextBase<ComponentType>;

/** Convenience wrapper that specializes defineLensBundle to React components. */
export function defineLensBundle(
  manifest: LensManifest,
  component: ComponentType,
): LensBundle {
  return defineLensBundleBase<ComponentType>(manifest, component);
}
