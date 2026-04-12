/**
 * Studio-specialised view of the generic `KernelInitializer` pattern
 * from `@prism/core/initializer`. Studio initializers receive a
 * `StudioKernel` in their install context; otherwise the shape and
 * install/disposer contract is identical.
 */

import type {
  KernelInitializer,
  KernelInitializerContext,
} from "@prism/core/initializer";
import type { StudioKernel } from "./studio-kernel.js";

export type StudioInitializer = KernelInitializer<StudioKernel>;
export type StudioInitializerContext = KernelInitializerContext<StudioKernel>;

export { installInitializers } from "@prism/core/initializer";
