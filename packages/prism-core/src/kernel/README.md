# kernel

Runtime, execution, and orchestration primitives for Prism apps. This category owns the moving parts that turn static data (foundation, language, identity) into a live, reactive, extensible runtime: the actor/process queue, automation engine, plugin system, config registry, kernel initializers, state machines, and the self-replicating app builder.

`kernel` may import from `network` (and vice versa) but must never reach down into `interaction` or `bindings`.

## Subcategories

- [`actor/`](./actor/README.md) — `@prism/core/actor`. Process queue, pluggable actor runtimes (Luau, sidecar, test), capability scoping, and the AI provider registry / context builder.
- [`automation/`](./automation/README.md) — `@prism/core/automation`. AutomationEngine with triggers (object/cron/manual), conditions, action dispatch, template interpolation.
- [`builder/`](./builder/README.md) — `@prism/core/builder`. Self-replicating app builder — AppProfile, BuildTarget, BuildPlan, and BuilderManager for composing focused Prism apps from a single Studio codebase.
- [`config/`](./config/README.md) — `@prism/core/config`. ConfigRegistry, ConfigModel, FeatureFlags, and schema validation with layered scope resolution (default → workspace → user).
- [`initializer/`](./initializer/README.md) — `@prism/core/initializer`. Generic `KernelInitializer<TKernel>` post-boot hook pattern, symmetric with PluginBundle / LensBundle.
- [`plugin/`](./plugin/README.md) — `@prism/core/plugin`. PluginRegistry + ContributionRegistry (views, commands, keybindings, menus, settings, toolbars, status bar, weak-ref providers) and the `PrismPlugin` interface.
- [`plugin-bundles/`](./plugin-bundles/README.md) — `@prism/core/plugin-bundles`. Canonical built-in plugin bundles (work, finance, crm, life, assets, platform) consumed by Studio via `createBuiltinBundles()`.
- [`state-machine/`](./state-machine/README.md) — `@prism/core/automaton` (and aliases `@prism/core/machines`, `@prism/core/state-machine`). Flat FSM primitives (`Machine`, `createMachine`) with guards, actions, and lifecycle hooks.
