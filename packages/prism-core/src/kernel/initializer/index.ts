/**
 * @prism/core/initializer — generic post-boot initializer pattern.
 *
 * `KernelInitializer<TKernel>` is a self-registering hook that runs
 * AFTER a kernel's construction so it can freely call any kernel method
 * (register templates, seed demo data, wire action handlers, …).
 *
 * Studio, Flux, Lattice, etc. each specialise `TKernel` to their own
 * kernel shape and keep app-specific initializers (demo content,
 * templates, bundles) next to the kernel.
 */

export type {
  KernelInitializer,
  KernelInitializerContext,
} from "./kernel-initializer.js";
export { installInitializers } from "./kernel-initializer.js";
