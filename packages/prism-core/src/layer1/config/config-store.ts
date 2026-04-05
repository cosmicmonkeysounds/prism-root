/**
 * @prism/core — ConfigStore implementations
 *
 * MemoryConfigStore — in-process only, lost on restart.
 *                     Use in tests and for ephemeral runtime overrides.
 *
 * For production, implement ConfigStore in your app layer:
 *   FileConfigStore — desktop/Tauri (workspace, user scopes)
 */

import type { ConfigStore } from "./config-types.js";

export class MemoryConfigStore implements ConfigStore {
  private _values: Record<string, unknown>;
  private _listeners = new Set<(values: Record<string, unknown>) => void>();

  constructor(initial: Record<string, unknown> = {}) {
    this._values = { ...initial };
  }

  load(): Record<string, unknown> {
    return { ...this._values };
  }

  save(values: Record<string, unknown>): void {
    this._values = { ...values };
  }

  subscribe(
    callback: (values: Record<string, unknown>) => void,
  ): () => void {
    this._listeners.add(callback);
    return () => this._listeners.delete(callback);
  }

  /**
   * Simulate an external change (e.g. another process updated the store).
   * Useful in tests to trigger hot-reload behaviour.
   */
  simulateExternalChange(values: Record<string, unknown>): void {
    this._values = { ...values };
    for (const l of this._listeners) l({ ...this._values });
  }

  get snapshot(): Record<string, unknown> {
    return { ...this._values };
  }
}
