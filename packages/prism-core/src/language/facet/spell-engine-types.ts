/**
 * Spell engine types — all interfaces for the Prism spell checking system.
 *
 * Designed around dependency injection: dictionaries, filters, storage,
 * and backends are all pluggable via interfaces.
 *
 * NOTE: The consumer-side interfaces (SpellChecker, TokenFilter, TokenContext,
 * SpellCheckDiagnostic, PersonalDictionary) used by the CM6 extension live in
 * `@prism/core/syntax` spell-check-types.ts. This file defines the ENGINE-side
 * interfaces that power the implementation.
 */

// ── Dictionary Provider ──────────────────────────────────────────────────────

/** Raw dictionary data — Hunspell .aff + .dic file contents. */
export interface DictionaryData {
  aff: string;
  dic: string;
}

/**
 * Pluggable dictionary source. Register via SpellCheckRegistry.
 *
 * Implementations load dictionary data from any source: bundled files,
 * CDN URLs, filesystem, IndexedDB cache, etc.
 */
export interface DictionaryProvider {
  /** Unique provider ID (e.g. 'hunspell:en-us', 'custom:medical'). */
  id: string;
  /** Human-readable label (e.g. 'English (US)'). */
  label: string;
  /** BCP-47 language tag (e.g. 'en', 'en-US', 'fr-FR'). */
  language: string;
  /** Load the dictionary data. Called once, result is cached by SpellChecker. */
  load(): Promise<DictionaryData>;
}

// ── Personal Dictionary Storage ──────────────────────────────────────────────

/**
 * Persistence backend for the personal dictionary.
 * Implement for localStorage, IndexedDB, filesystem, etc.
 */
export interface PersonalDictionaryStorage {
  load(): Promise<string[]>;
  save(words: string[]): Promise<void>;
}

// ── Extracted Word ──────────────────────────────────────────────────────────

/** Extracted word with position information. */
export interface ExtractedWord {
  word: string;
  from: number;
  to: number;
  line: string;
  offsetInLine: number;
}

// ── Spell Checker Backend ────────────────────────────────────────────────────

/**
 * Low-level spell checking engine interface.
 *
 * This is the only contract for spell checking backends. A real backend
 * (nspell, hunspell.js, or any WASM-based engine) can be injected via
 * this interface. The MockSpellCheckBackend is provided for testing.
 */
export interface SpellCheckBackend {
  /** Load dictionary data into the engine. */
  load(data: DictionaryData): void;
  /** Add a supplementary dictionary (e.g. personal words, domain terms). */
  addDictionary?(data: DictionaryData): void;
  /** Check if a word is correctly spelled. */
  correct(word: string): boolean;
  /** Get spelling suggestions for a word. */
  suggest(word: string): string[];
  /** Add a word to the runtime dictionary (session-only). */
  add?(word: string): void;
  /** Remove a word from the runtime dictionary. */
  remove?(word: string): void;
  /** Dispose of resources. */
  dispose?(): void;
}

// ── Configuration ────────────────────────────────────────────────────────────

/** SpellChecker construction options. */
export interface SpellCheckerConfig {
  /** BCP-47 language tag to use (default: 'en'). */
  language?: string;
  /** Maximum suggestions per word (default: 5). */
  maxSuggestions?: number;
  /** Minimum word length to check (default: 2). */
  minWordLength?: number;
  /** Custom word extraction regex (default: Latin + apostrophes). */
  wordPattern?: RegExp;
}

// ── Events ───────────────────────────────────────────────────────────────────

export type SpellCheckEvent =
  | { type: 'dictionary-loaded'; language: string }
  | { type: 'personal-word-added'; word: string }
  | { type: 'personal-word-removed'; word: string }
  | { type: 'filter-registered'; filterId: string }
  | { type: 'filter-unregistered'; filterId: string }
  | { type: 'dictionary-registered'; providerId: string }
  | { type: 'dictionary-unregistered'; providerId: string };

export type SpellCheckEventListener = (event: SpellCheckEvent) => void;
