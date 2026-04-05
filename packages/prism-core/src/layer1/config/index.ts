export type {
  SettingScope,
  SettingType,
  SettingDefinition,
  SettingChange,
  SettingWatcher,
  ChangeListener,
  ConfigStore,
  FeatureFlagContext,
  FeatureFlagCondition,
  FeatureFlagDefinition,
} from "./config-types.js";

export { SETTING_SCOPE_ORDER } from "./config-types.js";

export { ConfigRegistry } from "./config-registry.js";

export { ConfigModel } from "./config-model.js";

export type {
  ConfigSchema,
  StringSchema,
  NumberSchema,
  BooleanSchema,
  ArraySchema,
  ObjectSchema,
  ValidationError,
  ValidationResult,
} from "./config-schema.js";

export {
  validateConfig,
  coerceConfigValue,
  schemaToValidator,
} from "./config-schema.js";

export { FeatureFlags } from "./feature-flags.js";

export { MemoryConfigStore } from "./config-store.js";
