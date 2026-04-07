/**
 * SpellChecker — the main spell checking engine.
 *
 * Pulls dictionaries and filters from a SpellCheckRegistry, checks words
 * against the loaded dictionary, and returns diagnostics with suggestions.
 *
 * DI-based: the registry, personal dictionary, and backend are all injected.
 *
 * This class satisfies the SpellChecker interface defined in
 * `@prism/core/syntax` spell-check-types.ts (the consumer-side contract
 * used by the CM6 extension).
 */

import type {
  TokenFilter,
  TokenContext,
  SpellCheckDiagnostic,
  SpellChecker as SpellCheckerInterface,
} from '../syntax/spell-check-types';
import type {
  SpellCheckBackend,
  SpellCheckerConfig,
  ExtractedWord,
} from './spell-engine-types';
import type { SpellCheckRegistry } from './spell-engine-registry';
import type { PersonalDictionary } from './spell-engine-personal';

// Default word pattern: Latin characters + apostrophes (contractions)
const DEFAULT_WORD_PATTERN = /[a-zA-Z\u00C0-\u024F]+(?:'[a-zA-Z]+)*/g;

/** Extract words from text with position information. */
export function extractWords(text: string, pattern?: RegExp): ExtractedWord[] {
  const regex = new RegExp(
    (pattern ?? DEFAULT_WORD_PATTERN).source,
    (pattern ?? DEFAULT_WORD_PATTERN).flags.includes('g')
      ? (pattern ?? DEFAULT_WORD_PATTERN).flags
      : (pattern ?? DEFAULT_WORD_PATTERN).flags + 'g',
  );

  const words: ExtractedWord[] = [];
  const lines = text.split('\n');
  let lineStart = 0;

  for (const line of lines) {
    regex.lastIndex = 0;
    // Create a per-line regex to get offsetInLine
    const lineRegex = new RegExp(regex.source, regex.flags);
    let match: RegExpExecArray | null;
    while ((match = lineRegex.exec(line)) !== null) {
      words.push({
        word: match[0],
        from: lineStart + match.index,
        to: lineStart + match.index + match[0].length,
        line,
        offsetInLine: match.index,
      });
    }
    lineStart += line.length + 1; // +1 for the newline
  }

  return words;
}

/**
 * SpellChecker engine — the full implementation.
 *
 * Satisfies the consumer-side SpellChecker interface from spell-check-types.ts.
 * Requires a SpellCheckBackend to be injected (no default nspell dependency).
 */
export class SpellChecker implements SpellCheckerInterface {
  private readonly _registry: SpellCheckRegistry;
  private readonly _personal: PersonalDictionary | undefined;
  private _backend: SpellCheckBackend | undefined;
  private _loaded = false;
  private _loadingPromise: Promise<void> | undefined;
  private readonly _config: Required<SpellCheckerConfig>;

  constructor(
    registry: SpellCheckRegistry,
    options?: {
      personal?: PersonalDictionary;
      backend?: SpellCheckBackend;
      config?: SpellCheckerConfig;
    },
  ) {
    this._registry = registry;
    this._personal = options?.personal;
    this._backend = options?.backend;
    this._config = {
      language: options?.config?.language ?? 'en',
      maxSuggestions: options?.config?.maxSuggestions ?? 5,
      minWordLength: options?.config?.minWordLength ?? 2,
      wordPattern: options?.config?.wordPattern ?? DEFAULT_WORD_PATTERN,
    };
  }

  /** Whether the dictionary has been loaded and is ready for checking. */
  get isLoaded(): boolean {
    return this._loaded;
  }

  /** The active language. */
  get language(): string {
    return this._config.language;
  }

  /** The registry this checker draws from. */
  get registry(): SpellCheckRegistry {
    return this._registry;
  }

  /** The personal dictionary (if any). */
  get personal(): PersonalDictionary | undefined {
    return this._personal;
  }

  /**
   * Inject or replace the spell check backend.
   * Call before loadDictionary() to use a custom backend.
   *
   * NOTE: No default backend is provided. Inject a real backend (nspell,
   * hunspell.js, or any WASM engine implementing SpellCheckBackend) or
   * use MockSpellCheckBackend for testing.
   */
  setBackend(backend: SpellCheckBackend): void {
    this._backend?.dispose?.();
    this._backend = backend;
    this._loaded = false;
  }

  /**
   * Load the dictionary for the configured language.
   * Uses the first matching DictionaryProvider from the registry.
   * Safe to call multiple times (returns cached promise).
   *
   * Requires a backend to be set via constructor options or setBackend().
   * Throws if no backend is available.
   */
  async loadDictionary(language?: string): Promise<void> {
    if (language) this._config.language = language;
    if (this._loaded) return;
    if (this._loadingPromise) return this._loadingPromise;

    this._loadingPromise = this._doLoad();
    try {
      await this._loadingPromise;
    } finally {
      this._loadingPromise = undefined;
    }
  }

  private async _doLoad(): Promise<void> {
    const providers = this._registry.getDictionariesForLanguage(
      this._config.language,
    );
    if (providers.length === 0) {
      throw new Error(
        `No dictionary provider registered for language '${this._config.language}'. ` +
          `Available: ${this._registry.getAvailableLanguages().join(', ') || '(none)'}`,
      );
    }

    if (!this._backend) {
      throw new Error(
        'No SpellCheckBackend available. Inject one via constructor options ' +
          'or setBackend() before calling loadDictionary(). ' +
          'Use MockSpellCheckBackend for testing, or inject a real backend ' +
          '(nspell, hunspell.js) for production.',
      );
    }

    // Load the primary dictionary
    const data = await providers[0]!.load();
    this._backend.load(data);

    // Load supplementary dictionaries (same language, different providers)
    for (let i = 1; i < providers.length; i++) {
      const extra = await providers[i]!.load();
      if (this._backend.addDictionary) {
        this._backend.addDictionary(extra);
      }
    }

    // Load personal dictionary words into the backend
    if (this._personal) {
      await this._personal.load();
      for (const word of this._personal.getWords()) {
        this._backend.add?.(word);
      }
    }

    this._loaded = true;
  }

  /** Check if a single word is correctly spelled. */
  correct(word: string): boolean {
    if (!this._loaded || !this._backend) return true; // permissive when not loaded
    if (word.length < this._config.minWordLength) return true;
    if (this._personal?.isKnown(word)) return true;
    return this._backend.correct(word);
  }

  /** Get spelling suggestions for a word. */
  suggest(word: string, limit?: number): string[] {
    if (!this._loaded || !this._backend) return [];
    const max = limit ?? this._config.maxSuggestions;
    return this._backend.suggest(word).slice(0, max);
  }

  /**
   * Check an entire text string. Returns diagnostics for misspelled words.
   *
   * Applies all registered token filters from the registry.
   * Pass syntaxTypes to provide syntax tree node names for filter context.
   */
  checkText(
    text: string,
    options?: {
      /** Map of character offset -> syntax node type. */
      syntaxTypes?: Map<number, string>;
      /** Additional one-shot filters (not registered, just for this call). */
      filters?: TokenFilter[];
    },
  ): SpellCheckDiagnostic[] {
    if (!this._loaded || !this._backend) return [];

    const words = extractWords(text, this._config.wordPattern);
    const registeredFilters = this._registry.getAllFilters();
    const extraFilters = options?.filters ?? [];
    const allFilters = [...registeredFilters, ...extraFilters];
    const diagnostics: SpellCheckDiagnostic[] = [];

    for (const extracted of words) {
      if (extracted.word.length < this._config.minWordLength) continue;
      if (this._personal?.isKnown(extracted.word)) continue;

      // Build context for filters
      const ctx: TokenContext = {
        line: extracted.line,
        offsetInLine: extracted.offsetInLine,
        offsetInDoc: extracted.from,
        syntaxType: options?.syntaxTypes?.get(extracted.from),
      };

      // Check all filters — skip if any says so
      let skip = false;
      for (const filter of allFilters) {
        if (filter.shouldSkip(extracted.word, ctx)) {
          skip = true;
          break;
        }
      }
      if (skip) continue;

      // Check spelling
      if (!this._backend.correct(extracted.word)) {
        diagnostics.push({
          word: extracted.word,
          from: extracted.from,
          to: extracted.to,
          suggestions: this._backend
            .suggest(extracted.word)
            .slice(0, this._config.maxSuggestions),
        });
      }
    }

    return diagnostics;
  }

  /** Add a word to the personal dictionary (persisted). */
  async addToPersonal(word: string): Promise<void> {
    if (!this._personal) return;
    await this._personal.add(word);
    this._backend?.add?.(word);
  }

  /** Ignore a word for this session only. */
  ignoreWord(word: string): void {
    this._personal?.ignore(word);
  }

  /** Dispose of resources. */
  dispose(): void {
    this._backend?.dispose?.();
    this._backend = undefined;
    this._loaded = false;
  }
}
