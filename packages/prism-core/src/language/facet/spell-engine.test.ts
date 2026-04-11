import { describe, it, expect, beforeEach } from 'vitest';

import { SpellCheckRegistry } from './spell-engine-registry';
import { SpellChecker, extractWords } from './spell-engine-checker';
import {
  PersonalDictionary,
  MemoryDictionaryStorage,
} from './spell-engine-personal';
import {
  MockSpellCheckBackend,
  type MockSpellCheckConfig,
} from './spell-engine-mock';
import {
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
import {
  createStaticDictionaryProvider,
  createLazyDictionaryProvider,
} from './spell-engine-providers';
import { SpellCheckerBuilder, spellCheckerBuilder } from './spell-engine-builder';
import type {
  DictionaryProvider,
  SpellCheckEvent,
} from './spell-engine-types';

// ── Helpers ─────────────────────────────────────────────────────────────────

function makeMockBackend(config?: MockSpellCheckConfig): MockSpellCheckBackend {
  return new MockSpellCheckBackend(config);
}

function makeStaticProvider(
  overrides?: Partial<{
    id: string;
    label: string;
    language: string;
    aff: string;
    dic: string;
  }>,
): DictionaryProvider {
  return createStaticDictionaryProvider({
    id: overrides?.id ?? 'test:en',
    label: overrides?.label ?? 'English (Test)',
    language: overrides?.language ?? 'en',
    aff: overrides?.aff ?? '',
    dic: overrides?.dic ?? '',
  });
}

// ── SpellCheckRegistry ──────────────────────────────────────────────────────

describe('SpellCheckRegistry', () => {
  let registry: SpellCheckRegistry;

  beforeEach(() => {
    registry = new SpellCheckRegistry();
  });

  describe('dictionary providers', () => {
    it('registers and retrieves a dictionary provider', () => {
      const provider = makeStaticProvider();
      registry.registerDictionary(provider);
      expect(registry.getDictionary('test:en')).toBe(provider);
    });

    it('registers multiple providers at once', () => {
      const en = makeStaticProvider({ id: 'test:en', language: 'en' });
      const fr = makeStaticProvider({ id: 'test:fr', language: 'fr', label: 'French' });
      registry.registerDictionaries([en, fr]);
      expect(registry.getAllDictionaries()).toHaveLength(2);
    });

    it('replaces a provider with the same ID', () => {
      const v1 = makeStaticProvider({ id: 'same', label: 'v1' });
      const v2 = makeStaticProvider({ id: 'same', label: 'v2' });
      registry.registerDictionary(v1);
      registry.registerDictionary(v2);
      expect(registry.getDictionary('same')?.label).toBe('v2');
      expect(registry.getAllDictionaries()).toHaveLength(1);
    });

    it('unregisters a provider', () => {
      registry.registerDictionary(makeStaticProvider());
      registry.unregisterDictionary('test:en');
      expect(registry.getDictionary('test:en')).toBeUndefined();
    });

    it('unregister is a no-op for unknown ID', () => {
      const events: SpellCheckEvent[] = [];
      registry.on((e) => events.push(e));
      registry.unregisterDictionary('nope');
      expect(events).toHaveLength(0);
    });

    it('finds providers by exact language', () => {
      registry.registerDictionary(makeStaticProvider({ id: 'a', language: 'en' }));
      registry.registerDictionary(makeStaticProvider({ id: 'b', language: 'fr' }));
      const results = registry.getDictionariesForLanguage('en');
      expect(results).toHaveLength(1);
      expect(results[0]?.id).toBe('a');
    });

    it('finds providers by language prefix match', () => {
      registry.registerDictionary(makeStaticProvider({ id: 'a', language: 'en-US' }));
      registry.registerDictionary(makeStaticProvider({ id: 'b', language: 'en-GB' }));
      const results = registry.getDictionariesForLanguage('en');
      expect(results).toHaveLength(2);
    });

    it('finds providers when query is more specific than registration', () => {
      registry.registerDictionary(makeStaticProvider({ id: 'a', language: 'en' }));
      const results = registry.getDictionariesForLanguage('en-US');
      expect(results).toHaveLength(1);
    });

    it('returns available languages', () => {
      registry.registerDictionary(makeStaticProvider({ id: 'a', language: 'en' }));
      registry.registerDictionary(makeStaticProvider({ id: 'b', language: 'fr' }));
      const langs = registry.getAvailableLanguages();
      expect(langs).toContain('en');
      expect(langs).toContain('fr');
    });

    it('clear removes all registrations', () => {
      registry.registerDictionary(makeStaticProvider());
      registry.registerFilter(URL_FILTER);
      registry.clear();
      expect(registry.getAllDictionaries()).toHaveLength(0);
      expect(registry.getAllFilters()).toHaveLength(0);
    });
  });

  describe('token filters', () => {
    it('registers and retrieves a filter', () => {
      registry.registerFilter(URL_FILTER);
      expect(registry.getFilter('spellcheck:url')).toBe(URL_FILTER);
    });

    it('registers multiple filters at once', () => {
      registry.registerFilters([URL_FILTER, EMAIL_FILTER]);
      expect(registry.getAllFilters()).toHaveLength(2);
    });

    it('replaces a filter with the same ID', () => {
      registry.registerFilter(URL_FILTER);
      const replacement = { ...URL_FILTER, label: 'Custom URLs' };
      registry.registerFilter(replacement);
      expect(registry.getFilter('spellcheck:url')?.label).toBe('Custom URLs');
      expect(registry.getAllFilters()).toHaveLength(1);
    });

    it('unregisters a filter', () => {
      registry.registerFilter(URL_FILTER);
      registry.unregisterFilter('spellcheck:url');
      expect(registry.getFilter('spellcheck:url')).toBeUndefined();
    });
  });

  describe('events', () => {
    it('emits dictionary-registered on register', () => {
      const events: SpellCheckEvent[] = [];
      registry.on((e) => events.push(e));
      registry.registerDictionary(makeStaticProvider());
      expect(events).toHaveLength(1);
      expect(events[0]).toEqual({
        type: 'dictionary-registered',
        providerId: 'test:en',
      });
    });

    it('emits dictionary-unregistered on unregister', () => {
      registry.registerDictionary(makeStaticProvider());
      const events: SpellCheckEvent[] = [];
      registry.on((e) => events.push(e));
      registry.unregisterDictionary('test:en');
      expect(events).toHaveLength(1);
      expect(events[0]).toEqual({
        type: 'dictionary-unregistered',
        providerId: 'test:en',
      });
    });

    it('emits filter-registered on register', () => {
      const events: SpellCheckEvent[] = [];
      registry.on((e) => events.push(e));
      registry.registerFilter(URL_FILTER);
      expect(events[0]).toEqual({
        type: 'filter-registered',
        filterId: 'spellcheck:url',
      });
    });

    it('emits filter-unregistered on unregister', () => {
      registry.registerFilter(URL_FILTER);
      const events: SpellCheckEvent[] = [];
      registry.on((e) => events.push(e));
      registry.unregisterFilter('spellcheck:url');
      expect(events[0]).toEqual({
        type: 'filter-unregistered',
        filterId: 'spellcheck:url',
      });
    });

    it('unsubscribe stops events', () => {
      const events: SpellCheckEvent[] = [];
      const unsub = registry.on((e) => events.push(e));
      registry.registerFilter(URL_FILTER);
      expect(events).toHaveLength(1);
      unsub();
      registry.registerFilter(EMAIL_FILTER);
      expect(events).toHaveLength(1);
    });

    it('swallows listener errors', () => {
      const good: SpellCheckEvent[] = [];
      registry.on(() => {
        throw new Error('boom');
      });
      registry.on((e) => good.push(e));
      registry.registerFilter(URL_FILTER);
      expect(good).toHaveLength(1);
    });
  });
});

// ── extractWords ────────────────────────────────────────────────────────────

describe('extractWords', () => {
  it('extracts words with position info', () => {
    const words = extractWords('hello world');
    expect(words).toHaveLength(2);
    expect(words[0]).toEqual({
      word: 'hello',
      from: 0,
      to: 5,
      line: 'hello world',
      offsetInLine: 0,
    });
    expect(words[1]).toEqual({
      word: 'world',
      from: 6,
      to: 11,
      line: 'hello world',
      offsetInLine: 6,
    });
  });

  it('handles multi-line text', () => {
    const words = extractWords('hello\nworld');
    expect(words).toHaveLength(2);
    expect(words[0]?.from).toBe(0);
    expect(words[0]?.line).toBe('hello');
    expect(words[1]?.from).toBe(6);
    expect(words[1]?.line).toBe('world');
    expect(words[1]?.offsetInLine).toBe(0);
  });

  it('handles contractions', () => {
    const words = extractWords("don't");
    expect(words).toHaveLength(1);
    expect(words[0]?.word).toBe("don't");
  });

  it('returns empty for non-word text', () => {
    const words = extractWords('123 @#$ !!!');
    expect(words).toHaveLength(0);
  });

  it('accepts a custom word pattern', () => {
    const words = extractWords('hello 123 world', /\d+/g);
    expect(words).toHaveLength(1);
    expect(words[0]?.word).toBe('123');
  });
});

// ── PersonalDictionary ──────────────────────────────────────────────────────

describe('PersonalDictionary', () => {
  let personal: PersonalDictionary;

  beforeEach(() => {
    personal = new PersonalDictionary(new MemoryDictionaryStorage());
  });

  it('add and has', async () => {
    await personal.add('Prism');
    expect(personal.has('prism')).toBe(true);
    expect(personal.has('PRISM')).toBe(true);
  });

  it('remove', async () => {
    await personal.add('test');
    await personal.remove('test');
    expect(personal.has('test')).toBe(false);
  });

  it('remove unknown word is a no-op', async () => {
    await personal.load();
    await personal.remove('nope');
    expect(personal.getWords()).toHaveLength(0);
  });

  it('ignore and isIgnored', () => {
    personal.ignore('TempWord');
    expect(personal.isIgnored('tempword')).toBe(true);
  });

  it('isKnown covers both permanent and ignored', async () => {
    await personal.add('permanent');
    personal.ignore('session');
    expect(personal.isKnown('permanent')).toBe(true);
    expect(personal.isKnown('session')).toBe(true);
    expect(personal.isKnown('unknown')).toBe(false);
  });

  it('getWords returns permanent words', async () => {
    await personal.add('alpha');
    await personal.add('beta');
    personal.ignore('gamma');
    const words = personal.getWords();
    expect(words).toContain('alpha');
    expect(words).toContain('beta');
    expect(words).not.toContain('gamma');
  });

  it('add clears ignored status', async () => {
    personal.ignore('word');
    expect(personal.isIgnored('word')).toBe(true);
    await personal.add('word');
    expect(personal.isIgnored('word')).toBe(false);
    expect(personal.has('word')).toBe(true);
  });

  it('clearIgnored removes session words only', async () => {
    await personal.add('permanent');
    personal.ignore('session');
    personal.clearIgnored();
    expect(personal.isIgnored('session')).toBe(false);
    expect(personal.has('permanent')).toBe(true);
  });

  it('load is idempotent', async () => {
    await personal.add('first');
    await personal.load();
    await personal.load();
    expect(personal.getWords()).toHaveLength(1);
  });

  it('emits events on add/remove', async () => {
    const events: SpellCheckEvent[] = [];
    personal.on((e) => events.push(e));
    await personal.add('hello');
    await personal.remove('hello');
    expect(events).toHaveLength(2);
    expect(events[0]).toEqual({ type: 'personal-word-added', word: 'hello' });
    expect(events[1]).toEqual({ type: 'personal-word-removed', word: 'hello' });
  });

  it('does not emit for duplicate add', async () => {
    await personal.add('hello');
    const events: SpellCheckEvent[] = [];
    personal.on((e) => events.push(e));
    await personal.add('hello');
    expect(events).toHaveLength(0);
  });

  it('unsubscribe stops events', async () => {
    const events: SpellCheckEvent[] = [];
    const unsub = personal.on((e) => events.push(e));
    await personal.add('a');
    unsub();
    await personal.add('b');
    expect(events).toHaveLength(1);
  });

  it('dispose clears all state', async () => {
    await personal.add('word');
    personal.ignore('temp');
    personal.dispose();
    expect(personal.has('word')).toBe(false);
    expect(personal.isIgnored('temp')).toBe(false);
  });

  it('persists words via storage', async () => {
    const storage = new MemoryDictionaryStorage();
    const dict1 = new PersonalDictionary(storage);
    await dict1.add('persisted');

    const dict2 = new PersonalDictionary(storage);
    await dict2.load();
    expect(dict2.has('persisted')).toBe(true);
  });
});

// ── MockSpellCheckBackend ───────────────────────────────────────────────────

describe('MockSpellCheckBackend', () => {
  it('correct returns true for known words', () => {
    const backend = makeMockBackend({
      correct: new Set(['hello', 'world']),
    });
    expect(backend.correct('hello')).toBe(true);
    expect(backend.correct('xyz')).toBe(false);
  });

  it('correct is case-insensitive', () => {
    const backend = makeMockBackend({ correct: new Set(['hello']) });
    expect(backend.correct('Hello')).toBe(true);
    expect(backend.correct('HELLO')).toBe(true);
  });

  it('suggest returns mapped suggestions', () => {
    const backend = makeMockBackend({
      suggestions: { helo: ['hello', 'help'] },
    });
    expect(backend.suggest('helo')).toEqual(['hello', 'help']);
    expect(backend.suggest('unknown')).toEqual([]);
  });

  it('add makes word correct', () => {
    const backend = makeMockBackend();
    expect(backend.correct('custom')).toBe(false);
    backend.add('custom');
    expect(backend.correct('custom')).toBe(true);
  });

  it('remove makes word incorrect again', () => {
    const backend = makeMockBackend({ correct: new Set(['hello']) });
    backend.remove('hello');
    expect(backend.correct('hello')).toBe(false);
  });

  it('load is a no-op', () => {
    const backend = makeMockBackend({ correct: new Set(['hello']) });
    backend.load({ aff: '', dic: '' });
    expect(backend.correct('hello')).toBe(true);
  });

  it('dispose clears state', () => {
    const backend = makeMockBackend({ correct: new Set(['hello']) });
    backend.dispose();
    expect(backend.correct('hello')).toBe(false);
  });
});

// ── SpellChecker ────────────────────────────────────────────────────────────

describe('SpellChecker', () => {
  let registry: SpellCheckRegistry;
  let backend: MockSpellCheckBackend;
  let checker: SpellChecker;

  beforeEach(() => {
    registry = new SpellCheckRegistry();
    registry.registerDictionary(makeStaticProvider());
    backend = makeMockBackend({
      correct: new Set(['hello', 'world', 'the', 'is']),
      suggestions: { wrold: ['world'], helo: ['hello', 'help'] },
    });
    checker = new SpellChecker(registry, { backend });
  });

  it('is not loaded initially', () => {
    expect(checker.isLoaded).toBe(false);
  });

  it('loads dictionary from registry', async () => {
    await checker.loadDictionary();
    expect(checker.isLoaded).toBe(true);
  });

  it('loadDictionary is idempotent', async () => {
    await checker.loadDictionary();
    await checker.loadDictionary();
    expect(checker.isLoaded).toBe(true);
  });

  it('throws when no provider for language', async () => {
    const emptyReg = new SpellCheckRegistry();
    const c = new SpellChecker(emptyReg, { backend });
    await expect(c.loadDictionary()).rejects.toThrow(/No dictionary provider/);
  });

  it('throws when no backend', async () => {
    const c = new SpellChecker(registry);
    await expect(c.loadDictionary()).rejects.toThrow(/No SpellCheckBackend/);
  });

  it('correct returns true when not loaded (permissive)', () => {
    expect(checker.correct('anything')).toBe(true);
  });

  it('correct checks loaded backend', async () => {
    await checker.loadDictionary();
    expect(checker.correct('hello')).toBe(true);
    expect(checker.correct('wrold')).toBe(false);
  });

  it('correct skips short words', async () => {
    await checker.loadDictionary();
    expect(checker.correct('x')).toBe(true);
  });

  it('correct checks personal dictionary', async () => {
    const personal = new PersonalDictionary();
    await personal.add('prism');
    const c = new SpellChecker(registry, { backend, personal });
    await c.loadDictionary();
    expect(c.correct('prism')).toBe(true);
  });

  it('suggest returns empty when not loaded', () => {
    expect(checker.suggest('wrold')).toEqual([]);
  });

  it('suggest returns suggestions from backend', async () => {
    await checker.loadDictionary();
    expect(checker.suggest('wrold')).toEqual(['world']);
  });

  it('suggest respects limit', async () => {
    await checker.loadDictionary();
    const suggestions = checker.suggest('helo', 1);
    expect(suggestions).toHaveLength(1);
    expect(suggestions[0]).toBe('hello');
  });

  describe('checkText', () => {
    it('returns empty when not loaded', () => {
      expect(checker.checkText('wrold')).toEqual([]);
    });

    it('returns diagnostics for misspelled words', async () => {
      await checker.loadDictionary();
      const diags = checker.checkText('hello wrold');
      expect(diags).toHaveLength(1);
      expect(diags[0]?.word).toBe('wrold');
      expect(diags[0]?.from).toBe(6);
      expect(diags[0]?.to).toBe(11);
      expect(diags[0]?.suggestions).toEqual(['world']);
    });

    it('skips personal dictionary words', async () => {
      const personal = new PersonalDictionary();
      await personal.add('prism');
      const c = new SpellChecker(registry, { backend, personal });
      await c.loadDictionary();
      const diags = c.checkText('hello prism wrold');
      expect(diags).toHaveLength(1);
      expect(diags[0]?.word).toBe('wrold');
    });

    it('applies registered filters', async () => {
      registry.registerFilter(ALL_CAPS_FILTER);
      await checker.loadDictionary();
      const diags = checker.checkText('hello NASA wrold');
      // NASA should be skipped, wrold flagged
      expect(diags).toHaveLength(1);
      expect(diags[0]?.word).toBe('wrold');
    });

    it('applies one-shot filters', async () => {
      await checker.loadDictionary();
      const customFilter = {
        id: 'custom',
        shouldSkip: (word: string) => word === 'wrold',
      };
      const diags = checker.checkText('wrold', { filters: [customFilter] });
      expect(diags).toHaveLength(0);
    });

    it('passes syntaxTypes to filter context', async () => {
      const syntaxTypes = new Map<number, string>([[6, 'CodeBlock']]);
      registry.registerFilter(SYNTAX_CODE_FILTER);
      await checker.loadDictionary();
      const diags = checker.checkText('hello wrold', { syntaxTypes });
      // wrold starts at offset 6 which is tagged CodeBlock, so skipped
      expect(diags).toHaveLength(0);
    });

    it('skips words shorter than minWordLength', async () => {
      const c = new SpellChecker(registry, {
        backend,
        config: { minWordLength: 4 },
      });
      await c.loadDictionary();
      // "is" is only 2 chars, should be skipped regardless
      const diags = c.checkText('is wrold');
      expect(diags).toHaveLength(1);
      expect(diags[0]?.word).toBe('wrold');
    });
  });

  describe('personal dictionary integration', () => {
    it('addToPersonal adds to both personal dict and backend', async () => {
      const personal = new PersonalDictionary();
      const c = new SpellChecker(registry, { backend, personal });
      await c.loadDictionary();
      await c.addToPersonal('prism');
      expect(personal.has('prism')).toBe(true);
      expect(backend.correct('prism')).toBe(true);
    });

    it('ignoreWord makes word known in personal dict', async () => {
      const personal = new PersonalDictionary();
      const c = new SpellChecker(registry, { backend, personal });
      await c.loadDictionary();
      c.ignoreWord('prism');
      expect(personal.isKnown('prism')).toBe(true);
    });
  });

  describe('setBackend', () => {
    it('replaces the backend and resets loaded state', async () => {
      await checker.loadDictionary();
      expect(checker.isLoaded).toBe(true);
      const newBackend = makeMockBackend({ correct: new Set(['new']) });
      checker.setBackend(newBackend);
      expect(checker.isLoaded).toBe(false);
    });
  });

  describe('dispose', () => {
    it('clears loaded state', async () => {
      await checker.loadDictionary();
      checker.dispose();
      expect(checker.isLoaded).toBe(false);
    });
  });

  describe('loads personal words into backend', () => {
    it('personal words are loaded into the backend on loadDictionary', async () => {
      const personal = new PersonalDictionary();
      await personal.add('prism');
      const c = new SpellChecker(registry, { backend, personal });
      await c.loadDictionary();
      // prism was added to the backend by loadDictionary
      expect(backend.correct('prism')).toBe(true);
    });
  });
});

// ── Built-in Filters ────────────────────────────────────────────────────────

describe('Built-in Filters', () => {
  const ctx = (
    line: string,
    offsetInLine: number,
    syntaxType?: string,
  ) => ({
    line,
    offsetInLine,
    offsetInDoc: offsetInLine,
    ...(syntaxType !== undefined && { syntaxType }),
  });

  describe('URL_FILTER', () => {
    it('skips words inside URLs', () => {
      expect(URL_FILTER.shouldSkip('example', ctx('visit https://example.com', 14))).toBe(true);
    });

    it('does not skip normal words', () => {
      expect(URL_FILTER.shouldSkip('hello', ctx('hello world', 0))).toBe(false);
    });
  });

  describe('EMAIL_FILTER', () => {
    it('skips words adjacent to @', () => {
      expect(EMAIL_FILTER.shouldSkip('user', ctx('user@example.com', 0))).toBe(true);
    });

    it('skips domain part of email', () => {
      expect(EMAIL_FILTER.shouldSkip('example', ctx('user@example.com', 5))).toBe(true);
    });

    it('does not skip normal words', () => {
      expect(EMAIL_FILTER.shouldSkip('hello', ctx('hello world', 0))).toBe(false);
    });
  });

  describe('ALL_CAPS_FILTER', () => {
    it('skips all-caps words', () => {
      expect(ALL_CAPS_FILTER.shouldSkip('NASA', ctx('NASA rocks', 0))).toBe(true);
      expect(ALL_CAPS_FILTER.shouldSkip('API', ctx('the API', 4))).toBe(true);
    });

    it('does not skip single-char caps', () => {
      expect(ALL_CAPS_FILTER.shouldSkip('I', ctx('I am', 0))).toBe(false);
    });

    it('does not skip mixed-case words', () => {
      expect(ALL_CAPS_FILTER.shouldSkip('Hello', ctx('Hello', 0))).toBe(false);
    });
  });

  describe('CAMEL_CASE_FILTER', () => {
    it('skips camelCase', () => {
      expect(CAMEL_CASE_FILTER.shouldSkip('camelCase', ctx('camelCase', 0))).toBe(true);
    });

    it('skips PascalCase', () => {
      expect(CAMEL_CASE_FILTER.shouldSkip('PascalCase', ctx('PascalCase', 0))).toBe(true);
    });

    it('does not skip lowercase', () => {
      expect(CAMEL_CASE_FILTER.shouldSkip('hello', ctx('hello', 0))).toBe(false);
    });

    it('does not skip all-caps', () => {
      expect(CAMEL_CASE_FILTER.shouldSkip('NASA', ctx('NASA', 0))).toBe(false);
    });
  });

  describe('ALPHANUMERIC_FILTER', () => {
    it('skips words with digits', () => {
      expect(ALPHANUMERIC_FILTER.shouldSkip('h264', ctx('h264', 0))).toBe(true);
      expect(ALPHANUMERIC_FILTER.shouldSkip('utf8', ctx('utf8', 0))).toBe(true);
    });

    it('does not skip pure alpha words', () => {
      expect(ALPHANUMERIC_FILTER.shouldSkip('hello', ctx('hello', 0))).toBe(false);
    });
  });

  describe('FILE_PATH_FILTER', () => {
    it('skips words preceded by /', () => {
      expect(FILE_PATH_FILTER.shouldSkip('src', ctx('/src/index.ts', 1))).toBe(true);
    });

    it('skips words preceded by .', () => {
      expect(FILE_PATH_FILTER.shouldSkip('tsx', ctx('file.tsx', 5))).toBe(true);
    });

    it('does not skip normal words', () => {
      expect(FILE_PATH_FILTER.shouldSkip('hello', ctx('hello world', 0))).toBe(false);
    });
  });

  describe('INLINE_CODE_FILTER', () => {
    it('skips words inside backticks', () => {
      expect(INLINE_CODE_FILTER.shouldSkip('code', ctx('use `code` here', 5))).toBe(true);
    });

    it('does not skip words outside backticks', () => {
      expect(INLINE_CODE_FILTER.shouldSkip('use', ctx('use `code` here', 0))).toBe(false);
    });

    it('does not skip after closing backtick', () => {
      expect(INLINE_CODE_FILTER.shouldSkip('here', ctx('use `code` here', 11))).toBe(false);
    });
  });

  describe('SYNTAX_CODE_FILTER', () => {
    it('skips when syntaxType contains code', () => {
      expect(SYNTAX_CODE_FILTER.shouldSkip('fn', ctx('fn()', 0, 'CodeBlock'))).toBe(true);
    });

    it('skips when syntaxType contains frontmatter', () => {
      expect(SYNTAX_CODE_FILTER.shouldSkip('title', ctx('title: x', 0, 'Frontmatter'))).toBe(true);
    });

    it('does not skip without syntaxType', () => {
      expect(SYNTAX_CODE_FILTER.shouldSkip('hello', ctx('hello', 0))).toBe(false);
    });

    it('does not skip for non-code syntax type', () => {
      expect(SYNTAX_CODE_FILTER.shouldSkip('hello', ctx('hello', 0, 'Paragraph'))).toBe(false);
    });
  });

  describe('WIKI_LINK_FILTER', () => {
    it('skips words inside wiki links', () => {
      expect(WIKI_LINK_FILTER.shouldSkip('pageid', ctx('see [[pageid|Name]]', 6))).toBe(true);
    });

    it('does not skip words outside wiki links', () => {
      expect(WIKI_LINK_FILTER.shouldSkip('see', ctx('see [[pageid]]', 0))).toBe(false);
    });
  });

  describe('SINGLE_CHAR_FILTER', () => {
    it('skips single-character words', () => {
      expect(SINGLE_CHAR_FILTER.shouldSkip('I', ctx('I am', 0))).toBe(true);
    });

    it('does not skip multi-character words', () => {
      expect(SINGLE_CHAR_FILTER.shouldSkip('am', ctx('I am', 2))).toBe(false);
    });
  });

  describe('createDelimiterFilter', () => {
    it('skips words inside custom delimiters', () => {
      const mustache = createDelimiterFilter('mustache', '{{', '}}');
      expect(mustache.shouldSkip('name', ctx('Hello {{name}}!', 8))).toBe(true);
      expect(mustache.shouldSkip('Hello', ctx('Hello {{name}}!', 0))).toBe(false);
    });

    it('has correct id', () => {
      const f = createDelimiterFilter('test', '<', '>');
      expect(f.id).toBe('spellcheck:test');
    });
  });

  describe('createSyntaxFilter', () => {
    it('skips when syntaxType matches pattern', () => {
      const f = createSyntaxFilter('yaml', ['yaml', 'toml']);
      expect(f.shouldSkip('key', ctx('key: value', 0, 'YamlBlock'))).toBe(true);
      expect(f.shouldSkip('key', ctx('key: value', 0, 'Paragraph'))).toBe(false);
    });

    it('does not skip without syntaxType', () => {
      const f = createSyntaxFilter('yaml', ['yaml']);
      expect(f.shouldSkip('key', ctx('key: value', 0))).toBe(false);
    });
  });
});

// ── Dictionary Providers ────────────────────────────────────────────────────

describe('Dictionary Providers', () => {
  describe('createStaticDictionaryProvider', () => {
    it('returns correct metadata', () => {
      const p = createStaticDictionaryProvider({
        id: 'test:en',
        label: 'English',
        language: 'en',
        aff: 'AFF',
        dic: 'DIC',
      });
      expect(p.id).toBe('test:en');
      expect(p.label).toBe('English');
      expect(p.language).toBe('en');
    });

    it('load returns the static data', async () => {
      const p = createStaticDictionaryProvider({
        id: 'test:en',
        label: 'English',
        language: 'en',
        aff: 'AFF_CONTENT',
        dic: 'DIC_CONTENT',
      });
      const data = await p.load();
      expect(data.aff).toBe('AFF_CONTENT');
      expect(data.dic).toBe('DIC_CONTENT');
    });

    it('load returns the same object on subsequent calls', async () => {
      const p = createStaticDictionaryProvider({
        id: 'x',
        label: 'x',
        language: 'en',
        aff: '',
        dic: '',
      });
      const first = await p.load();
      const second = await p.load();
      expect(first).toBe(second);
    });
  });

  describe('createLazyDictionaryProvider', () => {
    it('calls loader once and caches', async () => {
      let calls = 0;
      const p = createLazyDictionaryProvider({
        id: 'lazy',
        label: 'Lazy',
        language: 'en',
        loader: async () => {
          calls++;
          return { aff: 'A', dic: 'D' };
        },
      });
      const first = await p.load();
      const second = await p.load();
      expect(calls).toBe(1);
      expect(first).toBe(second);
      expect(first.aff).toBe('A');
    });
  });
});

// ── SpellCheckerBuilder ─────────────────────────────────────────────────────

describe('SpellCheckerBuilder', () => {
  it('builds with defaults', () => {
    const backend = makeMockBackend();
    const { checker, registry } = spellCheckerBuilder()
      .backend(backend)
      .dictionary(makeStaticProvider())
      .build();
    expect(checker).toBeInstanceOf(SpellChecker);
    expect(registry).toBeInstanceOf(SpellCheckRegistry);
    expect(checker.language).toBe('en');
  });

  it('sets language', () => {
    const { checker } = spellCheckerBuilder()
      .language('fr')
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider({ language: 'fr' }))
      .build();
    expect(checker.language).toBe('fr');
  });

  it('registers dictionaries', () => {
    const en = makeStaticProvider({ id: 'a', language: 'en' });
    const fr = makeStaticProvider({ id: 'b', language: 'fr' });
    const { registry } = spellCheckerBuilder()
      .dictionaries([en, fr])
      .backend(makeMockBackend())
      .build();
    expect(registry.getAllDictionaries()).toHaveLength(2);
  });

  it('registers filters', () => {
    const { registry } = spellCheckerBuilder()
      .filter(URL_FILTER)
      .filter(CAMEL_CASE_FILTER)
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider())
      .build();
    expect(registry.getAllFilters()).toHaveLength(2);
  });

  it('registers multiple filters at once', () => {
    const { registry } = spellCheckerBuilder()
      .filters([URL_FILTER, EMAIL_FILTER, ALL_CAPS_FILTER])
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider())
      .build();
    expect(registry.getAllFilters()).toHaveLength(3);
  });

  it('uses an existing registry', () => {
    const existing = new SpellCheckRegistry();
    existing.registerFilter(URL_FILTER);
    const { registry } = spellCheckerBuilder()
      .registry(existing)
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider())
      .build();
    expect(registry).toBe(existing);
    // The provider from the builder should be added to the existing registry
    expect(registry.getAllDictionaries()).toHaveLength(1);
    // The pre-existing filter should still be there
    expect(registry.getAllFilters()).toHaveLength(1);
  });

  it('sets personal dictionary', () => {
    const personal = new PersonalDictionary();
    const { checker } = spellCheckerBuilder()
      .personal(personal)
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider())
      .build();
    expect(checker.personal).toBe(personal);
  });

  it('creates personal dictionary from storage', () => {
    const storage = new MemoryDictionaryStorage();
    const { checker } = spellCheckerBuilder()
      .personalStorage(storage)
      .backend(makeMockBackend())
      .dictionary(makeStaticProvider())
      .build();
    expect(checker.personal).toBeInstanceOf(PersonalDictionary);
  });

  it('sets maxSuggestions', async () => {
    const { checker } = spellCheckerBuilder()
      .maxSuggestions(2)
      .backend(
        makeMockBackend({
          correct: new Set(['hello']),
          suggestions: { wrold: ['world', 'would', 'wold'] },
        }),
      )
      .dictionary(makeStaticProvider())
      .build();
    await checker.loadDictionary();
    const suggestions = checker.suggest('wrold');
    expect(suggestions).toHaveLength(2);
  });

  it('sets minWordLength', async () => {
    const { checker } = spellCheckerBuilder()
      .minWordLength(5)
      .backend(makeMockBackend({ correct: new Set(['hello']) }))
      .dictionary(makeStaticProvider())
      .build();
    await checker.loadDictionary();
    // "is" is 2 chars, below minWordLength 5, so treated as correct
    expect(checker.correct('is')).toBe(true);
  });

  it('sets custom wordPattern', async () => {
    const { checker } = spellCheckerBuilder()
      .wordPattern(/[a-z]+/g)
      .backend(makeMockBackend({ correct: new Set(['hello']) }))
      .dictionary(makeStaticProvider())
      .build();
    await checker.loadDictionary();
    // "HELLO" won't match the lowercase-only pattern, so checkText won't find it
    const diags = checker.checkText('HELLO hello');
    expect(diags).toHaveLength(0);
  });

  it('fluent API is chainable', () => {
    const builder = new SpellCheckerBuilder();
    const result = builder
      .language('en')
      .dictionary(makeStaticProvider())
      .filter(URL_FILTER)
      .backend(makeMockBackend())
      .maxSuggestions(3)
      .minWordLength(2);
    expect(result).toBe(builder);
  });

  it('end-to-end: build, load, check', async () => {
    const { checker } = spellCheckerBuilder()
      .language('en')
      .dictionary(makeStaticProvider())
      .filter(ALL_CAPS_FILTER)
      .filter(CAMEL_CASE_FILTER)
      .backend(
        makeMockBackend({
          correct: new Set(['the', 'quick', 'brown', 'fox']),
          suggestions: { fxo: ['fox', 'faux'] },
        }),
      )
      .maxSuggestions(3)
      .build();

    await checker.loadDictionary();
    const diags = checker.checkText('the quick brown fxo NASA camelCase');
    // fxo is misspelled, NASA skipped by ALL_CAPS, camelCase skipped by CAMEL_CASE
    expect(diags).toHaveLength(1);
    expect(diags[0]?.word).toBe('fxo');
    expect(diags[0]?.suggestions).toEqual(['fox', 'faux']);
  });
});
