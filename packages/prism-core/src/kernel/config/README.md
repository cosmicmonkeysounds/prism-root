# config

Layered configuration system. `ConfigRegistry` is the schema catalog (built-in UI/editor/sync settings plus lens/plugin contributions); `ConfigModel` is the live runtime store with layered scope resolution (`default → workspace → user`); `FeatureFlags` evaluates conditional toggles; `validateConfig` checks values against declarative `ConfigSchema`s.

```ts
import { ConfigRegistry, ConfigModel } from "@prism/core/config";
```

## Key exports

- `ConfigRegistry` — catalog of `SettingDefinition`s and `FeatureFlagDefinition`s; seeded with built-in UI/editor/sync settings.
- `ConfigModel` — runtime config with `get`/`set`/`watch`/`load`, scope-aware resolution and change notifications.
- `FeatureFlags` — feature-flag evaluator reading from a `ConfigModel` + `FeatureFlagContext`.
- `MemoryConfigStore` — in-memory `ConfigStore` implementation for testing or ephemeral scopes.
- `validateConfig(schema, value)` / `coerceConfigValue(schema, value)` / `schemaToValidator(schema)` — declarative `ConfigSchema` validation (string/number/boolean/array/object).
- `SETTING_SCOPE_ORDER` — canonical scope resolution order.
- Types: `SettingScope`, `SettingType`, `SettingDefinition`, `SettingChange`, `SettingWatcher`, `ChangeListener`, `ConfigStore`, `FeatureFlagContext`, `FeatureFlagCondition`, `FeatureFlagDefinition`, `ConfigSchema`, `StringSchema`, `NumberSchema`, `BooleanSchema`, `ArraySchema`, `ObjectSchema`, `ValidationError`, `ValidationResult`.

## Usage

```ts
import { ConfigRegistry, ConfigModel } from "@prism/core/config";

const registry = new ConfigRegistry();
const config = new ConfigModel(registry);
config.load("workspace", { "ui.theme": "dark" });
config.load("user", { "ui.density": "compact" });

const theme = config.get<string>("ui.theme"); // "dark"
config.watch("ui.theme", (next) => applyTheme(next));
```
