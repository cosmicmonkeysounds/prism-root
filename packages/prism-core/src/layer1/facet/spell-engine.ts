/**
 * Spell Engine — Prism spell checking system.
 *
 * Registry-based, pluggable dictionaries, extensible token filters,
 * personal dictionary with pluggable persistence.
 *
 * Nothing is hardcoded. Consumers register:
 *  - DictionaryProviders (for language dictionaries)
 *  - TokenFilters (for skip rules — what NOT to check)
 *  - PersonalDictionaryStorage (for persistence)
 *  - SpellCheckBackend (to swap the engine — no default provided)
 *
 * NOTE: No nspell/hunspell dependency is bundled. Inject a real backend
 * implementing SpellCheckBackend for production use. MockSpellCheckBackend
 * is provided for testing.
 *
 * @example
 *   import {
 *     SpellCheckRegistry, SpellChecker, PersonalDictionary,
 *     MockSpellCheckBackend,
 *     createUrlDictionaryProvider,
 *     URL_FILTER, CAMEL_CASE_FILTER, ALL_CAPS_FILTER,
 *     spellCheckerBuilder,
 *   } from './spell-engine';
 *
 *   // Builder pattern
 *   const { checker, registry } = spellCheckerBuilder()
 *     .language('en-US')
 *     .dictionary(createUrlDictionaryProvider({ ... }))
 *     .filter(URL_FILTER)
 *     .filter(CAMEL_CASE_FILTER)
 *     .backend(new MockSpellCheckBackend({ correct: new Set(['hello']) }))
 *     .build();
 *
 *   await checker.loadDictionary();
 *   const diagnostics = checker.checkText('Ths is a tset');
 */

// ── Types ────────────────────────────────────────────────────────────────────
export type {
  DictionaryData,
  DictionaryProvider,
  PersonalDictionaryStorage,
  ExtractedWord,
  SpellCheckBackend,
  SpellCheckerConfig,
  SpellCheckEvent,
  SpellCheckEventListener,
} from './spell-engine-types';

// ── Registry ─────────────────────────────────────────────────────────────────
export { SpellCheckRegistry } from './spell-engine-registry';

// ── Checker ──────────────────────────────────────────────────────────────────
export { SpellChecker, extractWords } from './spell-engine-checker';

// ── Personal dictionary ──────────────────────────────────────────────────────
export { PersonalDictionary, MemoryDictionaryStorage } from './spell-engine-personal';

// ── Builder ──────────────────────────────────────────────────────────────────
export { SpellCheckerBuilder, spellCheckerBuilder } from './spell-engine-builder';

// ── Dictionary providers ─────────────────────────────────────────────────────
export {
  createUrlDictionaryProvider,
  createStaticDictionaryProvider,
  createLazyDictionaryProvider,
} from './spell-engine-providers';

// ── Built-in filters (NOT auto-registered — consumer picks what they want) ──
export {
  URL_FILTER,
  EMAIL_FILTER,
  ALL_CAPS_FILTER,
  CAMEL_CASE_FILTER,
  ALPHANUMERIC_FILTER,
  FILE_PATH_FILTER,
  INLINE_CODE_FILTER,
  SYNTAX_CODE_FILTER,
  WIKI_LINK_FILTER,
  SINGLE_CHAR_FILTER,
  createDelimiterFilter,
  createSyntaxFilter,
} from './spell-engine-filters';

// ── Mock backend (for tests) ─────────────────────────────────────────────────
export { MockSpellCheckBackend, type MockSpellCheckConfig } from './spell-engine-mock';
