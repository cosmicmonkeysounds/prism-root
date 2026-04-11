/**
 * SpellCheckerBuilder — fluent API for constructing SpellChecker instances.
 *
 * Usage:
 *   const { checker, registry } = spellCheckerBuilder()
 *     .language('en-US')
 *     .dictionary(myDictProvider)
 *     .filter(URL_FILTER)
 *     .filter(CAMEL_CASE_FILTER)
 *     .personal(myPersonalDict)
 *     .maxSuggestions(8)
 *     .backend(myCustomBackend)
 *     .build();
 */

import type { TokenFilter } from '@prism/core/syntax';
import type {
  DictionaryProvider,
  SpellCheckBackend,
  SpellCheckerConfig,
  PersonalDictionaryStorage,
} from './spell-engine-types';
import { SpellCheckRegistry } from './spell-engine-registry';
import { SpellChecker } from './spell-engine-checker';
import { PersonalDictionary } from './spell-engine-personal';

export class SpellCheckerBuilder {
  private _language = 'en';
  private _dictionaries: DictionaryProvider[] = [];
  private _filters: TokenFilter[] = [];
  private _personal: PersonalDictionary | undefined;
  private _personalStorage: PersonalDictionaryStorage | undefined;
  private _backend: SpellCheckBackend | undefined;
  private _registry: SpellCheckRegistry | undefined;
  private _maxSuggestions = 5;
  private _minWordLength = 2;
  private _wordPattern: RegExp | undefined;

  /** Set the language (BCP-47 tag). Default: 'en'. */
  language(lang: string): this {
    this._language = lang;
    return this;
  }

  /** Add a dictionary provider. Registered on the registry at build time. */
  dictionary(provider: DictionaryProvider): this {
    this._dictionaries.push(provider);
    return this;
  }

  /** Add multiple dictionary providers. */
  dictionaries(providers: DictionaryProvider[]): this {
    this._dictionaries.push(...providers);
    return this;
  }

  /** Add a token filter. Registered on the registry at build time. */
  filter(filter: TokenFilter): this {
    this._filters.push(filter);
    return this;
  }

  /** Add multiple token filters. */
  filters(filters: TokenFilter[]): this {
    this._filters.push(...filters);
    return this;
  }

  /** Set the personal dictionary instance. */
  personal(dict: PersonalDictionary): this {
    this._personal = dict;
    return this;
  }

  /** Create a personal dictionary with the given storage backend. */
  personalStorage(storage: PersonalDictionaryStorage): this {
    this._personalStorage = storage;
    return this;
  }

  /**
   * Set a custom spell check backend.
   *
   * NOTE: No default backend is provided (no nspell dependency).
   * You MUST set a backend before the checker can load dictionaries.
   * Use MockSpellCheckBackend for testing, or inject a real backend
   * (nspell, hunspell.js) for production.
   */
  backend(backend: SpellCheckBackend): this {
    this._backend = backend;
    return this;
  }

  /** Use an existing registry instead of creating a new one. */
  registry(registry: SpellCheckRegistry): this {
    this._registry = registry;
    return this;
  }

  /** Maximum suggestions per misspelled word. Default: 5. */
  maxSuggestions(n: number): this {
    this._maxSuggestions = n;
    return this;
  }

  /** Minimum word length to check. Default: 2. */
  minWordLength(n: number): this {
    this._minWordLength = n;
    return this;
  }

  /** Custom regex for word extraction. */
  wordPattern(pattern: RegExp): this {
    this._wordPattern = pattern;
    return this;
  }

  /**
   * Build the SpellChecker. Registers all dictionaries and filters
   * on the registry (creates one if not provided).
   */
  build(): { checker: SpellChecker; registry: SpellCheckRegistry } {
    const registry = this._registry ?? new SpellCheckRegistry();

    // Register dictionaries
    registry.registerDictionaries(this._dictionaries);

    // Register filters
    registry.registerFilters(this._filters);

    // Build personal dictionary
    const personal =
      this._personal ??
      (this._personalStorage
        ? new PersonalDictionary(this._personalStorage)
        : undefined);

    const opts: { personal?: PersonalDictionary; backend?: SpellCheckBackend; config?: SpellCheckerConfig } = {
      config: {
        language: this._language,
        maxSuggestions: this._maxSuggestions,
        minWordLength: this._minWordLength,
        ...(this._wordPattern !== undefined && { wordPattern: this._wordPattern }),
      },
    };
    if (personal !== undefined) opts.personal = personal;
    if (this._backend !== undefined) opts.backend = this._backend;
    const checker = new SpellChecker(registry, opts);

    return { checker, registry };
  }
}

/** Create a new SpellCheckerBuilder. */
export function spellCheckerBuilder(): SpellCheckerBuilder {
  return new SpellCheckerBuilder();
}
