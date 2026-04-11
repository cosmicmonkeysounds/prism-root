/**
 * MockSpellCheckBackend — deterministic mock for unit tests.
 *
 * Usage:
 *   const backend = new MockSpellCheckBackend({
 *     correct: new Set(['hello', 'world']),
 *     suggestions: { 'wrold': ['world'], 'helo': ['hello', 'help'] },
 *   });
 *
 * For production, inject a real SpellCheckBackend implementation
 * (e.g. nspell, hunspell.js, or any WASM-based engine).
 */

import type { SpellCheckBackend, DictionaryData } from './spell-engine-types';

export interface MockSpellCheckConfig {
  /** Set of correctly-spelled words. */
  correct?: Set<string>;
  /** Map of misspelled word -> suggestions. */
  suggestions?: Record<string, string[]>;
}

export class MockSpellCheckBackend implements SpellCheckBackend {
  private _correct: Set<string>;
  private _suggestions: Record<string, string[]>;
  private _added = new Set<string>();

  constructor(config?: MockSpellCheckConfig) {
    this._correct = new Set(config?.correct ?? []);
    this._suggestions = { ...config?.suggestions };
  }

  load(_data: DictionaryData): void {
    // No-op — mock uses the injected word lists
  }

  correct(word: string): boolean {
    return (
      this._correct.has(word.toLowerCase()) ||
      this._added.has(word.toLowerCase())
    );
  }

  suggest(word: string): string[] {
    return this._suggestions[word.toLowerCase()] ?? [];
  }

  add(word: string): void {
    this._added.add(word.toLowerCase());
    this._correct.add(word.toLowerCase());
  }

  remove(word: string): void {
    this._added.delete(word.toLowerCase());
    this._correct.delete(word.toLowerCase());
  }

  dispose(): void {
    this._correct.clear();
    this._added.clear();
    this._suggestions = {};
  }
}
