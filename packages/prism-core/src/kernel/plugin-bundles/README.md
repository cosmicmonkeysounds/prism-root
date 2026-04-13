# plugin-bundles

Canonical built-in `PluginBundle`s shipped with `@prism/core`. Each bundle extends the Flux domain with new entity types, edge relations, automation presets, and a `PrismPlugin` contribution set (views, commands, keybindings, activity bar). Bundles are self-registering: they install themselves into an `ObjectRegistry` + `PluginRegistry` via `install(ctx)`. Studio calls `createBuiltinBundles()` + `installPluginBundles(bundles, ctx)` during kernel boot; other apps (Flux, Lattice, …) can cherry-pick individual bundles.

```ts
import {
  createBuiltinBundles,
  installPluginBundles,
} from "@prism/core/plugin-bundles";
```

## Subcategories

- [`work/`](./work/README.md) — freelance gigs, time entries, focus blocks.
- [`finance/`](./finance/README.md) — loans, grants, budgets (builds on Flux Transaction/Account/Invoice).
- [`crm/`](./crm/README.md) — CRM views/commands/pipeline over Flux Contact/Organization (no new entity types).
- [`life/`](./life/README.md) — habits, fitness, sleep, journal, meals, cycle tracking.
- [`assets/`](./assets/README.md) — media assets, content items, scanned documents, collections.
- [`platform/`](./platform/README.md) — calendar events, messages, reminders, feeds.

## Top-level exports

- `createBuiltinBundles()` — returns all six built-in bundles in canonical order (work, finance, crm, life, assets, platform).
- `installPluginBundles(bundles, ctx)` — installs each bundle against a `PluginInstallContext` and returns a single uninstall function.
- Types: `PluginBundle`, `PluginInstallContext`.
- Re-exports all entity/edge/status constants and registry factories from each subcategory.
