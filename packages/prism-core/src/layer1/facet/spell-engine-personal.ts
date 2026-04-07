/**
 * PersonalDictionary — user's custom word list with pluggable persistence.
 *
 * Two levels:
 *  - **Permanent** words: persisted via the injected storage backend.
 *  - **Ignored** words: session-only, cleared on dispose.
 *
 * Satisfies the PersonalDictionary interface from spell-check-types.ts
 * (which only requires `isKnown(word): boolean`).
 */

import type {
  PersonalDictionaryStorage,
  SpellCheckEvent,
  SpellCheckEventListener,
} from './spell-engine-types';

/** In-memory storage — words are lost when the instance is disposed. */
export class MemoryDictionaryStorage implements PersonalDictionaryStorage {
  private _words: string[] = [];

  async load(): Promise<string[]> {
    return [...this._words];
  }

  async save(words: string[]): Promise<void> {
    this._words = [...words];
  }
}

export class PersonalDictionary {
  private readonly _words = new Set<string>();
  private readonly _ignored = new Set<string>();
  private readonly _storage: PersonalDictionaryStorage;
  private readonly _listeners = new Set<SpellCheckEventListener>();
  private _loaded = false;

  constructor(storage?: PersonalDictionaryStorage) {
    this._storage = storage ?? new MemoryDictionaryStorage();
  }

  /** Load words from storage. Safe to call multiple times (no-op after first). */
  async load(): Promise<void> {
    if (this._loaded) return;
    const words = await this._storage.load();
    for (const w of words) this._words.add(w.toLowerCase());
    this._loaded = true;
  }

  /** Add a word permanently (persisted to storage). */
  async add(word: string): Promise<void> {
    const normalized = word.toLowerCase();
    if (this._words.has(normalized)) return;
    this._words.add(normalized);
    this._ignored.delete(normalized);
    await this._storage.save([...this._words]);
    this._emit({ type: 'personal-word-added', word: normalized });
  }

  /** Remove a word from the permanent dictionary. */
  async remove(word: string): Promise<void> {
    const normalized = word.toLowerCase();
    if (!this._words.delete(normalized)) return;
    await this._storage.save([...this._words]);
    this._emit({ type: 'personal-word-removed', word: normalized });
  }

  /** Check if a word is in the permanent dictionary. */
  has(word: string): boolean {
    return this._words.has(word.toLowerCase());
  }

  /** Ignore a word for this session only (not persisted). */
  ignore(word: string): void {
    this._ignored.add(word.toLowerCase());
  }

  /** Check if a word is ignored (session-only). */
  isIgnored(word: string): boolean {
    return this._ignored.has(word.toLowerCase());
  }

  /** Check if a word is known (permanent or ignored). */
  isKnown(word: string): boolean {
    const normalized = word.toLowerCase();
    return this._words.has(normalized) || this._ignored.has(normalized);
  }

  /** Get all permanent words. */
  getWords(): string[] {
    return [...this._words];
  }

  /** Subscribe to dictionary changes. */
  on(listener: SpellCheckEventListener): () => void {
    this._listeners.add(listener);
    return () => {
      this._listeners.delete(listener);
    };
  }

  private _emit(event: SpellCheckEvent): void {
    for (const listener of this._listeners) {
      try {
        listener(event);
      } catch {
        /* swallow */
      }
    }
  }

  /** Clear session-ignored words. */
  clearIgnored(): void {
    this._ignored.clear();
  }

  /** Dispose — clear all state. */
  dispose(): void {
    this._words.clear();
    this._ignored.clear();
    this._listeners.clear();
    this._loaded = false;
  }
}
