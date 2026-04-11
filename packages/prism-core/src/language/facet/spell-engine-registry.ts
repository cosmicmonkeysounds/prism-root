/**
 * SpellCheckRegistry — central registration point for dictionaries and filters.
 *
 * All spell checking capabilities are contributed through this registry.
 * Nothing is hardcoded — consumers register dictionaries for their languages
 * and token filters for their skip rules.
 */

import type { TokenFilter } from '@prism/core/syntax';
import type {
  DictionaryProvider,
  SpellCheckEvent,
  SpellCheckEventListener,
} from './spell-engine-types';

export class SpellCheckRegistry {
  private readonly _dictionaries = new Map<string, DictionaryProvider>();
  private readonly _filters = new Map<string, TokenFilter>();
  private readonly _listeners = new Set<SpellCheckEventListener>();

  // ── Dictionary providers ─────────────────────────────────────────────────

  /** Register a dictionary provider. Replaces any existing provider with the same ID. */
  registerDictionary(provider: DictionaryProvider): void {
    this._dictionaries.set(provider.id, provider);
    this._emit({ type: 'dictionary-registered', providerId: provider.id });
  }

  /** Register multiple dictionary providers at once. */
  registerDictionaries(providers: DictionaryProvider[]): void {
    for (const p of providers) this.registerDictionary(p);
  }

  /** Unregister a dictionary provider by ID. */
  unregisterDictionary(id: string): void {
    if (this._dictionaries.delete(id)) {
      this._emit({ type: 'dictionary-unregistered', providerId: id });
    }
  }

  /** Get a dictionary provider by ID. */
  getDictionary(id: string): DictionaryProvider | undefined {
    return this._dictionaries.get(id);
  }

  /** Find dictionary providers for a language (BCP-47 tag). */
  getDictionariesForLanguage(language: string): DictionaryProvider[] {
    const norm = language.toLowerCase();
    return [...this._dictionaries.values()].filter(
      (d) =>
        d.language.toLowerCase() === norm ||
        d.language.toLowerCase().startsWith(norm + '-') ||
        norm.startsWith(d.language.toLowerCase() + '-'),
    );
  }

  /** Get all registered dictionary provider IDs. */
  getAvailableLanguages(): string[] {
    const langs = new Set<string>();
    for (const d of this._dictionaries.values()) langs.add(d.language);
    return [...langs];
  }

  /** Get all registered dictionary providers. */
  getAllDictionaries(): DictionaryProvider[] {
    return [...this._dictionaries.values()];
  }

  // ── Token filters ────────────────────────────────────────────────────────

  /** Register a token filter. Replaces any existing filter with the same ID. */
  registerFilter(filter: TokenFilter): void {
    this._filters.set(filter.id, filter);
    this._emit({ type: 'filter-registered', filterId: filter.id });
  }

  /** Register multiple token filters at once. */
  registerFilters(filters: TokenFilter[]): void {
    for (const f of filters) this.registerFilter(f);
  }

  /** Unregister a token filter by ID. */
  unregisterFilter(id: string): void {
    if (this._filters.delete(id)) {
      this._emit({ type: 'filter-unregistered', filterId: id });
    }
  }

  /** Get a token filter by ID. */
  getFilter(id: string): TokenFilter | undefined {
    return this._filters.get(id);
  }

  /** Get all registered token filters. */
  getAllFilters(): TokenFilter[] {
    return [...this._filters.values()];
  }

  // ── Events ───────────────────────────────────────────────────────────────

  /** Subscribe to registry changes. Returns an unsubscribe function. */
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
        /* swallow listener errors */
      }
    }
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────

  /** Clear all registrations. */
  clear(): void {
    this._dictionaries.clear();
    this._filters.clear();
  }
}
