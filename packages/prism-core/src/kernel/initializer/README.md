# initializer

Generic post-boot initializer pattern. `KernelInitializer<TKernel>` is a self-registering hook that runs AFTER a kernel's construction so it can freely call kernel methods (seed demo data, register templates, wire action handlers, etc.). Symmetric with `PluginBundle` and `LensBundle`, but scoped to side-effects rather than contribution registration. Each app specialises `TKernel` with its own kernel type (Studio → `StudioKernel`, Flux → `FluxKernel`, …).

```ts
import { installInitializers } from "@prism/core/initializer";
```

## Key exports

- `installInitializers(initializers, ctx)` — installs a list of `KernelInitializer<TKernel>`s against a `KernelInitializerContext<TKernel>` and returns a composite disposer that runs each uninstall in reverse order.
- Types: `KernelInitializer<TKernel>` (`{ id, name, install(ctx) => () => void }`), `KernelInitializerContext<TKernel>` (`{ kernel }`).

## Usage

```ts
import type { KernelInitializer } from "@prism/core/initializer";
import { installInitializers } from "@prism/core/initializer";

interface StudioKernel {
  templates: { register(t: unknown): void };
}

const seedTemplates: KernelInitializer<StudioKernel> = {
  id: "studio.seed.templates",
  name: "Seed Templates",
  install({ kernel }) {
    kernel.templates.register({ id: "blank" });
    return () => {};
  },
};

const dispose = installInitializers([seedTemplates], { kernel });
```
