/**
 * @prism/core — ConfigRegistry
 *
 * Central catalog of all known SettingDefinitions and FeatureFlagDefinitions.
 * Instance-based (not singleton) — each Prism workspace gets its own registry.
 *
 * Built-in settings cover UI, editor, and sync categories.
 * Lenses extend the registry with their own settings.
 */

import type {
  SettingDefinition,
  FeatureFlagDefinition,
  SettingScope,
} from "./config-types.js";

// ── Built-in Prism settings ─────────────────────────────────────────────────────

const BUILT_IN_SETTINGS: SettingDefinition[] = [
  // ── UI ────────────────────────────────────────────────────────────────────
  {
    key: "ui.theme",
    type: "select",
    default: "system",
    label: "Theme",
    tags: ["ui"],
    options: ["light", "dark", "system"],
    scopes: ["workspace", "user"],
  },
  {
    key: "ui.density",
    type: "select",
    default: "comfortable",
    label: "Density",
    tags: ["ui"],
    options: ["compact", "comfortable", "spacious"],
    scopes: ["workspace", "user"],
  },
  {
    key: "ui.language",
    type: "string",
    default: "en",
    label: "Language",
    description: "BCP 47 locale code (e.g. en-US, fr-FR)",
    tags: ["ui"],
    scopes: ["user"],
  },
  {
    key: "ui.sidebarWidth",
    type: "number",
    default: 260,
    label: "Sidebar width (px)",
    tags: ["ui"],
    scopes: ["user"],
    validate: (v) =>
      typeof v === "number" && v >= 180 && v <= 600
        ? null
        : "Must be between 180 and 600",
  },
  {
    key: "ui.showActivityBar",
    type: "boolean",
    default: true,
    label: "Show activity bar",
    tags: ["ui"],
    scopes: ["user"],
  },

  // ── Editor ────────────────────────────────────────────────────────────────
  {
    key: "editor.fontSize",
    type: "number",
    default: 14,
    label: "Editor font size",
    tags: ["editor"],
    scopes: ["workspace", "user"],
    validate: (v) =>
      typeof v === "number" && v >= 8 && v <= 32
        ? null
        : "Must be between 8 and 32",
  },
  {
    key: "editor.lineNumbers",
    type: "boolean",
    default: true,
    label: "Show line numbers",
    tags: ["editor"],
    scopes: ["user"],
  },
  {
    key: "editor.spellCheck",
    type: "boolean",
    default: false,
    label: "Spell check",
    tags: ["editor"],
    scopes: ["user"],
  },
  {
    key: "editor.indentSize",
    type: "number",
    default: 2,
    label: "Indent size (spaces)",
    description: "Number of spaces per indent level in code editors.",
    tags: ["editor"],
    scopes: ["workspace", "user"],
    validate: (v) =>
      typeof v === "number" && v >= 1 && v <= 8 && Number.isInteger(v)
        ? null
        : "Must be an integer between 1 and 8",
  },
  {
    key: "editor.autosaveMs",
    type: "number",
    default: 1500,
    label: "Autosave delay (ms)",
    tags: ["editor"],
    scopes: ["workspace", "user"],
    validate: (v) =>
      typeof v === "number" && v >= 0 ? null : "Must be >= 0",
  },

  // ── Sync ──────────────────────────────────────────────────────────────────
  {
    key: "sync.enabled",
    type: "boolean",
    default: false,
    label: "Enable sync",
    tags: ["sync"],
    scopes: ["workspace"],
  },
  {
    key: "sync.intervalSeconds",
    type: "number",
    default: 300,
    label: "Sync interval (seconds)",
    tags: ["sync"],
    scopes: ["workspace"],
    validate: (v) =>
      typeof v === "number" && v >= 0
        ? null
        : "Must be >= 0 (0 = manual only)",
  },

  // ── AI ────────────────────────────────────────────────────────────────────
  {
    key: "ai.enabled",
    type: "boolean",
    default: true,
    label: "Enable AI features",
    tags: ["ai"],
    scopes: ["workspace"],
  },
  {
    key: "ai.provider",
    type: "select",
    default: "anthropic",
    label: "AI provider",
    tags: ["ai"],
    options: ["anthropic", "openai", "ollama", "custom"],
    scopes: ["workspace"],
  },
  {
    key: "ai.modelId",
    type: "string",
    default: "claude-sonnet-4-6",
    label: "AI model ID",
    tags: ["ai"],
    scopes: ["workspace", "user"],
  },
  {
    key: "ai.apiKey",
    type: "string",
    default: "",
    label: "AI API key",
    tags: ["ai"],
    secret: true,
    scopes: ["workspace"],
  },

  // ── Notifications ─────────────────────────────────────────────────────────
  {
    key: "notifications.inApp",
    type: "boolean",
    default: true,
    label: "In-app notifications",
    tags: ["notifications"],
    scopes: ["user"],
  },
];

// ── Built-in feature flags ──────────────────────────────────────────────────────

const BUILT_IN_FLAGS: FeatureFlagDefinition[] = [
  {
    id: "ai-features",
    label: "AI Features",
    description: "AI chat, suggestions, and knowledge base",
    default: true,
    settingKey: "ai.enabled",
  },
  {
    id: "sync",
    label: "CRDT Sync",
    description: "Sync workspace data to peer nodes",
    default: false,
    settingKey: "sync.enabled",
  },
];

// ── ConfigRegistry ──────────────────────────────────────────────────────────────

export class ConfigRegistry {
  private _settings = new Map<string, SettingDefinition>();
  private _flags = new Map<string, FeatureFlagDefinition>();

  constructor() {
    this.reset();
  }

  // ── Settings ────────────────────────────────────────────────────────────

  register<T>(def: SettingDefinition<T>): void {
    this._settings.set(def.key, def as SettingDefinition);
  }

  registerAll(defs: SettingDefinition[]): void {
    for (const def of defs) this.register(def);
  }

  get(key: string): SettingDefinition | undefined {
    return this._settings.get(key);
  }

  all(): SettingDefinition[] {
    return [...this._settings.values()];
  }

  byTag(tag: string): SettingDefinition[] {
    return [...this._settings.values()].filter((d) => d.tags?.includes(tag));
  }

  byScope(scope: SettingScope): SettingDefinition[] {
    return [...this._settings.values()].filter(
      (d) => !d.scopes || d.scopes.includes(scope),
    );
  }

  getDefault(key: string): unknown {
    return this._settings.get(key)?.default;
  }

  // ── Feature flags ─────────────────────────────────────────────────────────

  registerFlag(def: FeatureFlagDefinition): void {
    this._flags.set(def.id, def);
  }

  registerAllFlags(defs: FeatureFlagDefinition[]): void {
    for (const def of defs) this.registerFlag(def);
  }

  getFlag(id: string): FeatureFlagDefinition | undefined {
    return this._flags.get(id);
  }

  allFlags(): FeatureFlagDefinition[] {
    return [...this._flags.values()];
  }

  // ── Reset ─────────────────────────────────────────────────────────────────

  /** Reset to built-in definitions only (useful in tests). */
  reset(): void {
    this._settings.clear();
    this._flags.clear();
    for (const s of BUILT_IN_SETTINGS) this._settings.set(s.key, s);
    for (const f of BUILT_IN_FLAGS) this._flags.set(f.id, f);
  }
}
