/**
 * @prism/core — Config System Types
 *
 * Scopes cascade from least to most specific (most specific wins):
 *
 *   default   — hardcoded registry defaults
 *   workspace — PrismManifest.settings
 *   user      — user profile preferences
 *
 * Prism is local-first: no 'app' or 'team' scopes.
 * All stores are synchronous (Loro CRDT is sync).
 */

// ── Scopes ──────────────────────────────────────────────────────────────────────

export type SettingScope = "default" | "workspace" | "user";

/** Ordered from least to most specific. */
export const SETTING_SCOPE_ORDER: SettingScope[] = [
  "default",
  "workspace",
  "user",
];

// ── Setting types ───────────────────────────────────────────────────────────────

export type SettingType =
  | "string"
  | "number"
  | "boolean"
  | "select"
  | "object"
  | "array";

// ── Setting definition ──────────────────────────────────────────────────────────

/**
 * Full definition of a setting.
 * Register with ConfigRegistry.register() at startup.
 */
export interface SettingDefinition<T = unknown> {
  /** Dot-notation key: 'ui.theme', 'editor.fontSize', etc. */
  key: string;
  type: SettingType;
  /** Hardcoded fallback value. */
  default: T;
  label: string;
  description?: string;
  /**
   * Which scopes may override this setting.
   * Scopes not listed are ignored during resolution.
   * @default all scopes
   */
  scopes?: SettingScope[];
  /** For 'select' type: valid options. */
  options?: T[];
  /**
   * Validation function. Return human-readable error string, or null if valid.
   * Called on set(); invalid values are rejected.
   */
  validate?: (value: T) => string | null;
  /** When true: masked (replaced with '***') in toJSON() output. */
  secret?: boolean;
  /** When true: changing this value requires a restart. */
  requiresRestart?: boolean;
  /** Grouping tags for settings UI. */
  tags?: string[];
}

// ── Change events ───────────────────────────────────────────────────────────────

export interface SettingChange {
  key: string;
  previousValue: unknown;
  newValue: unknown;
  scope: SettingScope;
}

export type SettingWatcher<T = unknown> = (
  value: T,
  change: SettingChange,
) => void;
export type ChangeListener = (change: SettingChange) => void;

// ── ConfigStore ─────────────────────────────────────────────────────────────────

/**
 * Synchronous persistence interface for a single scope's settings.
 *
 * Prism uses Loro CRDT — all operations are synchronous.
 *
 * Implementations:
 *   MemoryConfigStore — in-process only (tests, ephemeral state)
 *   FileConfigStore   — desktop/Tauri (workspace, user scopes)
 */
export interface ConfigStore {
  /** Load all values for this store. */
  load(): Record<string, unknown>;
  /** Persist the given values (full snapshot). */
  save(values: Record<string, unknown>): void;
  /**
   * Subscribe to external changes (other processes, file watchers).
   * Returns an unsubscribe function.
   */
  subscribe(callback: (values: Record<string, unknown>) => void): () => void;
}

// ── Feature flags ───────────────────────────────────────────────────────────────

/**
 * Context for feature flag condition evaluation.
 * All fields optional — missing fields mean "condition not applicable".
 */
export interface FeatureFlagContext {
  /** Resolved config values for condition evaluation. */
  config?: Record<string, unknown>;
}

/**
 * A condition that contributes to a feature flag's value.
 * Conditions are evaluated in order; first matching condition wins.
 */
export type FeatureFlagCondition =
  | { type: "always"; value: boolean }
  | { type: "config"; key: string; equals: unknown; value: boolean };

export interface FeatureFlagDefinition {
  id: string;
  label: string;
  description?: string;
  /** Default value when no condition matches. */
  default: boolean;
  /** Conditions evaluated in order. First match wins. */
  conditions?: FeatureFlagCondition[];
  /**
   * Config key that can override this flag.
   * When set, ConfigModel.get(settingKey) takes precedence.
   */
  settingKey?: string;
}
