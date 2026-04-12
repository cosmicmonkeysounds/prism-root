/**
 * KernelInitializer — self-registering startup hooks (generic).
 *
 * Mirrors PluginBundle / LensBundle: each initializer knows how to install
 * itself onto a kernel (seed data, register templates, register action
 * handlers, etc) without the host having to call N specific functions in
 * the right order.
 *
 * The kernel runs initializers AFTER all core state has been constructed
 * and its return object exists, so initializers can call any kernel
 * method freely.
 *
 * Generic over `TKernel` so each app can specialise the context with its
 * own kernel type (Studio's `StudioKernel`, Flux's `FluxKernel`, …).
 */

export interface KernelInitializerContext<TKernel> {
  readonly kernel: TKernel;
}

export interface KernelInitializer<TKernel> {
  /** Unique id. Primarily for debugging and deduplication. */
  readonly id: string;
  /** Human-readable name. */
  readonly name: string;
  /**
   * Install this initializer onto the kernel. Returns a disposer that
   * is run from `kernel.dispose()`. Most initializers are one-shot and
   * can return a no-op.
   */
  install(ctx: KernelInitializerContext<TKernel>): () => void;
}

/**
 * Install multiple initializers in order. Returns a single disposer that
 * runs all individual disposers in reverse order.
 */
export function installInitializers<TKernel>(
  initializers: KernelInitializer<TKernel>[],
  ctx: KernelInitializerContext<TKernel>,
): () => void {
  const disposers = initializers.map((init) => init.install(ctx));
  return () => {
    for (let i = disposers.length - 1; i >= 0; i--) {
      const dispose = disposers[i];
      if (dispose) dispose();
    }
  };
}
