/**
 * Studio Puck providers — DI seam for dynamic-content components.
 *
 * Module-level registry pre-populated with built-in providers. Adding a new
 * parametric component means:
 *   1. Define an entity type in `kernel/entities.ts`.
 *   2. Create a provider file in this folder.
 *   3. Register it in `createStudioPuckRegistry` below.
 *
 * `layout-panel.tsx` calls `createStudioPuckRegistry()` inside its config
 * `useMemo` and merges the resulting component map into its Puck config,
 * overriding any generic fallback for the same entity type.
 */

import {
  createPuckComponentRegistry,
  type PuckComponentRegistry,
} from "@prism/core/puck";
import type { StudioKernel } from "../../kernel/studio-kernel.js";
import { recordListProvider } from "./record-list-provider.js";

export { recordListProvider };

/** Build a fresh registry seeded with the studio's built-in providers. */
export function createStudioPuckRegistry(): PuckComponentRegistry<StudioKernel> {
  return createPuckComponentRegistry<StudioKernel>().register(
    recordListProvider,
  );
}
