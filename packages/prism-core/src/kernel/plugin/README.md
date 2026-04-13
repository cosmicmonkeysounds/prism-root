# plugin

The Prism plugin system. `PluginRegistry` manages registered `PrismPlugin` instances and routes their declared contributions (views, commands, keybindings, context menus, …) into typed `ContributionRegistry` buckets that the shell queries at runtime. `ContributionRegistry<T>` is the single generalization of every "plugins declare, shell consumes" pattern.

```ts
import { PluginRegistry } from "@prism/core/plugin";
```

## Key exports

- `PluginRegistry` — register/unregister `PrismPlugin`s, auto-fan-out their `contributes` into the per-type `ContributionRegistry`s, emit `registered`/`unregistered` events.
- `ContributionRegistry<T>` — generic typed registry with `register`, `registerAll`, `unregister`, `unregisterByPlugin`, `all`, and `query`.
- `pluginId(str)` — typed `PluginId` constructor.
- Types: `PrismPlugin`, `PluginId`, `PluginContributions`, `ViewContributionDef`, `ViewZone`, `CommandContributionDef`, `ContextMenuContributionDef`, `KeybindingContributionDef`, `ActivityBarContributionDef`, `SettingsContributionDef`, `ToolbarContributionDef`, `StatusBarContributionDef`, `WeakRefProviderContributionDef`, `ContributionEntry<T>`, `PluginRegistryEvent`, `PluginRegistryEventType`, `PluginRegistryListener`.

## Usage

```ts
import { PluginRegistry, pluginId } from "@prism/core/plugin";

const plugins = new PluginRegistry();
plugins.register({
  id: pluginId("my.plugin"),
  name: "My Plugin",
  contributes: {
    commands: [
      { id: "my.hello", label: "Hello", category: "My", action: "my.hello" },
    ],
  },
});

for (const entry of plugins.commands.all()) {
  console.log(entry.pluginId, entry.item.label);
}
```
