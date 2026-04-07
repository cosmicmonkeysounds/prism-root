/**
 * Built-in TokenFilter implementations.
 *
 * These are NOT auto-registered. The consumer picks which filters they want
 * and registers them on the SpellCheckRegistry:
 *
 *   registry.registerFilters([URL_FILTER, EMAIL_FILTER, CAMEL_CASE_FILTER]);
 *
 * Or use the SpellCheckerBuilder:
 *
 *   spellCheckerBuilder().filter(URL_FILTER).filter(CAMEL_CASE_FILTER).build();
 */

import type { TokenFilter } from '../syntax/spell-check-types';

/** Skip words that look like URLs (http://, https://, ftp://, www.). */
export const URL_FILTER: TokenFilter = {
  id: 'spellcheck:url',
  label: 'URLs',
  shouldSkip(_word, ctx) {
    // Check if the word is inside a URL-like substring in the line
    const before = ctx.line.slice(0, ctx.offsetInLine);
    return /(?:https?:\/\/|ftp:\/\/|www\.)\S*$/i.test(before);
  },
};

/** Skip words that look like email addresses. */
export const EMAIL_FILTER: TokenFilter = {
  id: 'spellcheck:email',
  label: 'Email addresses',
  shouldSkip(word, ctx) {
    const before = ctx.line.slice(0, ctx.offsetInLine);
    const after = ctx.line.slice(ctx.offsetInLine + word.length);
    return (
      before.endsWith('@') ||
      after.startsWith('@') ||
      /\S+@\S+/.test(
        ctx.line.slice(
          Math.max(0, ctx.offsetInLine - 50),
          ctx.offsetInLine + word.length + 50,
        ),
      )
    );
  },
};

/** Skip words that are all uppercase (likely acronyms: NASA, API, HTML). */
export const ALL_CAPS_FILTER: TokenFilter = {
  id: 'spellcheck:all-caps',
  label: 'ALL CAPS words',
  shouldSkip(word) {
    return word.length >= 2 && word === word.toUpperCase();
  },
};

/** Skip camelCase and PascalCase identifiers. */
export const CAMEL_CASE_FILTER: TokenFilter = {
  id: 'spellcheck:camel-case',
  label: 'camelCase identifiers',
  shouldSkip(word) {
    // Contains both upper and lower, with at least one mid-word uppercase
    return /^[a-z]+[A-Z]/.test(word) || /^[A-Z][a-z]+[A-Z]/.test(word);
  },
};

/** Skip words that contain digits (e.g. 'h264', 'utf8', 'v2'). */
export const ALPHANUMERIC_FILTER: TokenFilter = {
  id: 'spellcheck:alphanumeric',
  label: 'Alphanumeric tokens',
  shouldSkip(word) {
    return /\d/.test(word);
  },
};

/** Skip words that look like file paths or extensions (.tsx, /src/). */
export const FILE_PATH_FILTER: TokenFilter = {
  id: 'spellcheck:file-path',
  label: 'File paths',
  shouldSkip(word, ctx) {
    const before = ctx.line.slice(
      Math.max(0, ctx.offsetInLine - 1),
      ctx.offsetInLine,
    );
    const after = ctx.line.slice(
      ctx.offsetInLine + word.length,
      ctx.offsetInLine + word.length + 1,
    );
    return (
      before === '/' ||
      before === '.' ||
      after === '/' ||
      /^\.[a-z]{1,6}$/i.test(word)
    );
  },
};

/** Skip words inside backtick-delimited inline code. */
export const INLINE_CODE_FILTER: TokenFilter = {
  id: 'spellcheck:inline-code',
  label: 'Inline code (backticks)',
  shouldSkip(_word, ctx) {
    // Count backticks before the word in the line
    const before = ctx.line.slice(0, ctx.offsetInLine);
    const backtickCount = (before.match(/`/g) ?? []).length;
    // Odd number of backticks = inside inline code
    return backtickCount % 2 === 1;
  },
};

/** Skip words when the syntax tree says they're in a code/frontmatter node. */
export const SYNTAX_CODE_FILTER: TokenFilter = {
  id: 'spellcheck:syntax-code',
  label: 'Code blocks (syntax tree)',
  shouldSkip(_word, ctx) {
    if (!ctx.syntaxType) return false;
    const t = ctx.syntaxType.toLowerCase();
    return (
      t.includes('code') ||
      t.includes('frontmatter') ||
      t.includes('fencedcode') ||
      t.includes('codeblock') ||
      t.includes('inlinecode') ||
      t.includes('codetext')
    );
  },
};

/** Skip words that look like wiki-links [[id|name]] content. */
export const WIKI_LINK_FILTER: TokenFilter = {
  id: 'spellcheck:wiki-link',
  label: 'Wiki-link content',
  shouldSkip(_word, ctx) {
    const before = ctx.line.slice(0, ctx.offsetInLine);
    const after = ctx.line.slice(ctx.offsetInLine);
    // Inside [[ ... ]]
    const lastOpen = before.lastIndexOf('[[');
    const lastClose = before.lastIndexOf(']]');
    return lastOpen > lastClose && after.includes(']]');
  },
};

/** Skip single-character words. */
export const SINGLE_CHAR_FILTER: TokenFilter = {
  id: 'spellcheck:single-char',
  label: 'Single characters',
  shouldSkip(word) {
    return word.length <= 1;
  },
};

/**
 * Create a filter that skips words inside a custom delimiter pair.
 * E.g. `createDelimiterFilter('mustache', '{{', '}}')` skips template expressions.
 */
export function createDelimiterFilter(
  id: string,
  open: string,
  close: string,
  label?: string,
): TokenFilter {
  return {
    id: `spellcheck:${id}`,
    label: label ?? `${open}...${close} content`,
    shouldSkip(_word, ctx) {
      const before = ctx.line.slice(0, ctx.offsetInLine);
      const after = ctx.line.slice(ctx.offsetInLine);
      const lastOpen = before.lastIndexOf(open);
      const lastClose = before.lastIndexOf(close);
      return lastOpen > lastClose && after.includes(close);
    },
  };
}

/**
 * Create a filter that skips words when the syntax type matches any of the given patterns.
 * Patterns are matched case-insensitively as substrings.
 */
export function createSyntaxFilter(
  id: string,
  syntaxPatterns: string[],
  label?: string,
): TokenFilter {
  const lowered = syntaxPatterns.map((p) => p.toLowerCase());
  return {
    id: `spellcheck:${id}`,
    label,
    shouldSkip(_word, ctx) {
      if (!ctx.syntaxType) return false;
      const t = ctx.syntaxType.toLowerCase();
      return lowered.some((p) => t.includes(p));
    },
  };
}
