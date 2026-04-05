/**
 * @prism/core — ConfigModel
 *
 * Live runtime config with layered scope resolution.
 * Resolution walks scopes from most specific (user) to least (default/registry).
 *
 * Usage:
 *   const config = new ConfigModel(registry);
 *   config.load('workspace', manifest.settings ?? {});
 *   config.load('user', userProfile.preferences);
 *
 *   config.get<string>('ui.theme');           // 'dark'
 *   config.set('ui.theme', 'light', 'user'); // save to user scope
 *   config.watch('sync.intervalSeconds', v => restartSync(v));
 *
 * Hot reload: call config.load(scope, newValues) on external changes.
 * All watchers for affected keys are notified.
 */

import type {
  SettingScope,
  SettingChange,
  SettingWatcher,
  ChangeListener,
  ConfigStore,
} from "./config-types.js";
import { SETTING_SCOPE_ORDER } from "./config-types.js";
import type { ConfigRegistry } from "./config-registry.js";

export class ConfigModel {
  /** scope -> key -> value */
  private _layers = new Map<SettingScope, Map<string, unknown>>();
  /** key -> Set of watchers */
  private _watchers = new Map<string, Set<SettingWatcher>>();
  /** Wildcard change listeners */
  private _changeListeners = new Set<ChangeListener>();
  /** scope -> ConfigStore (for persistence) */
  private _stores = new Map<SettingScope, ConfigStore>();
  /** scope -> store unsubscribe fn */
  private _storeUnsubs = new Map<SettingScope, () => void>();

  constructor(private _registry: ConfigRegistry) {
    for (const scope of SETTING_SCOPE_ORDER) {
      this._layers.set(scope, new Map());
    }
  }

  /** Safe layer access — returns the scope's map or a fresh empty map. */
  private _layer(scope: SettingScope): Map<string, unknown> {
    return this._layers.get(scope) ?? new Map();
  }

  // ── Loading ─────────────────────────────────────────────────────────────────

  /**
   * Load (or replace) all values for a scope.
   * Notifies watchers for any keys whose resolved value changes.
   */
  load(scope: SettingScope, values: Record<string, unknown>): void {
    const layer = this._layer(scope);
    const previous = new Map(layer);

    layer.clear();
    for (const [k, v] of Object.entries(values)) {
      layer.set(k, v);
    }

    const affectedKeys = new Set([...previous.keys(), ...layer.keys()]);
    for (const key of affectedKeys) {
      const wasResolved = this._resolveWithPrevious(key, scope, previous);
      const nowResolved = this.get(key);
      if (!deepEqual(wasResolved, nowResolved)) {
        this._notifyChange(key, wasResolved, nowResolved, scope);
      }
    }
  }

  /**
   * Attach a persistent store to a scope.
   * Calls store.load() immediately and store.save() on mutations.
   * External changes arrive via store.subscribe().
   */
  attachStore(scope: SettingScope, store: ConfigStore): void {
    this._stores.set(scope, store);
    const values = store.load();
    this.load(scope, values);

    const unsub = store.subscribe((newValues) => {
      this.load(scope, newValues);
    });
    this._storeUnsubs.set(scope, unsub);
  }

  detachStore(scope: SettingScope): void {
    this._storeUnsubs.get(scope)?.();
    this._storeUnsubs.delete(scope);
    this._stores.delete(scope);
  }

  // ── Reading ─────────────────────────────────────────────────────────────────

  /**
   * Get the resolved value for a key (most specific scope wins).
   * Falls back to registry default if no scope has the key.
   */
  get<T = unknown>(key: string, fallback?: T): T {
    for (let i = SETTING_SCOPE_ORDER.length - 1; i >= 0; i--) {
      const scope = SETTING_SCOPE_ORDER[i] as SettingScope;
      const def = this._registry.get(key);
      if (def?.scopes && !def.scopes.includes(scope)) continue;
      const val = this._layer(scope).get(key);
      if (val !== undefined) return val as T;
    }
    const registryDefault = this._registry.getDefault(key);
    if (registryDefault !== undefined) return registryDefault as T;
    return fallback as T;
  }

  /**
   * Get the value at a specific scope (not cascaded).
   */
  getAtScope<T = unknown>(key: string, scope: SettingScope): T | undefined {
    return this._layer(scope).get(key) as T | undefined;
  }

  /**
   * Get all key/value pairs for a specific scope (not cascaded).
   */
  getScope(scope: SettingScope): Record<string, unknown> {
    return Object.fromEntries(this._layer(scope));
  }

  /**
   * Check whether a key has been explicitly set in any non-default scope.
   */
  isOverridden(key: string): boolean {
    for (let i = SETTING_SCOPE_ORDER.length - 1; i >= 1; i--) {
      const scope = SETTING_SCOPE_ORDER[i] as SettingScope;
      if (this._layer(scope).has(key)) return true;
    }
    return false;
  }

  // ── Writing ─────────────────────────────────────────────────────────────────

  /**
   * Set a value in the given scope.
   * Throws if the key has a validator that rejects the value,
   * or if the scope is not allowed.
   */
  set(key: string, value: unknown, scope: SettingScope): void {
    const def = this._registry.get(key);
    if (def?.scopes && !def.scopes.includes(scope)) {
      throw new Error(`Setting '${key}' does not allow scope '${scope}'`);
    }
    if (def?.validate) {
      const err = def.validate(value);
      if (err) throw new Error(`Invalid value for '${key}': ${err}`);
    }

    const previous = this.get(key);
    this._layer(scope).set(key, value);
    const resolved = this.get(key);

    if (!deepEqual(previous, resolved)) {
      this._notifyChange(key, previous, resolved, scope);
    }

    const store = this._stores.get(scope);
    if (store) {
      store.save(this.getScope(scope));
    }
  }

  /**
   * Remove a value from a specific scope (falls back to parent scopes).
   */
  reset(key: string, scope: SettingScope): void {
    const previous = this.get(key);
    this._layer(scope).delete(key);
    const resolved = this.get(key);

    if (!deepEqual(previous, resolved)) {
      this._notifyChange(key, previous, resolved, scope);
    }

    const store = this._stores.get(scope);
    if (store) {
      store.save(this.getScope(scope));
    }
  }

  // ── Watching ────────────────────────────────────────────────────────────────

  /**
   * Watch a specific key for resolved value changes.
   * The callback is called immediately with the current resolved value,
   * and again whenever it changes. Returns an unsubscribe function.
   */
  watch<T = unknown>(key: string, callback: SettingWatcher<T>): () => void {
    if (!this._watchers.has(key)) {
      this._watchers.set(key, new Set());
    }
    const watcher = callback as SettingWatcher;
    const watcherSet = this._watchers.get(key);
    if (watcherSet) watcherSet.add(watcher);

    const current = this.get<T>(key);
    const change: SettingChange = {
      key,
      previousValue: current,
      newValue: current,
      scope: "default",
    };
    callback(current, change);

    return () => this._watchers.get(key)?.delete(watcher);
  }

  /**
   * Listen for any config change across all keys and scopes.
   * Returns an unsubscribe function.
   */
  on(event: "change", listener: ChangeListener): () => void {
    this._changeListeners.add(listener);
    return () => this._changeListeners.delete(listener);
  }

  // ── Serialization ──────────────────────────────────────────────────────────

  /**
   * Serialize a scope's values as a plain object.
   * Secret values are replaced with '***'.
   */
  toJSON(scope: SettingScope): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const [key, value] of this._layer(scope)) {
      const def = this._registry.get(key);
      out[key] = def?.secret ? "***" : value;
    }
    return out;
  }

  // ── Internal ───────────────────────────────────────────────────────────────

  private _notifyChange(
    key: string,
    previousValue: unknown,
    newValue: unknown,
    scope: SettingScope,
  ): void {
    const change: SettingChange = { key, previousValue, newValue, scope };
    for (const l of this._changeListeners) l(change);
    for (const w of this._watchers.get(key) ?? []) w(newValue, change);
  }

  private _resolveWithPrevious(
    key: string,
    modifiedScope: SettingScope,
    previousLayer: Map<string, unknown>,
  ): unknown {
    for (let i = SETTING_SCOPE_ORDER.length - 1; i >= 0; i--) {
      const scope = SETTING_SCOPE_ORDER[i] as SettingScope;
      const def = this._registry.get(key);
      if (def?.scopes && !def.scopes.includes(scope)) continue;
      const layer =
        scope === modifiedScope ? previousLayer : this._layer(scope);
      const val = layer.get(key);
      if (val !== undefined) return val;
    }
    return this._registry.getDefault(key);
  }
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (typeof a !== "object" || a === null || b === null) return false;
  const aKeys = Object.keys(a as object);
  const bKeys = Object.keys(b as object);
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((k) =>
    deepEqual(
      (a as Record<string, unknown>)[k],
      (b as Record<string, unknown>)[k],
    ),
  );
}
