/**
 * Spell check types — consumer-side interfaces for the CM6 extension.
 *
 * These define the contract that any SpellChecker implementation must satisfy
 * for the CodeMirror spell-check extension to consume it. The actual
 * SpellChecker class, registry, and dictionary providers live elsewhere;
 * this file contains only the shapes the extension needs at compile time.
 */

// ── Token Filter ─────────────────────────────────────────────────────────────

/** Context passed to token filters for skip decisions. */
export interface TokenContext {
  /** The full line containing the word. */
  line: string;
  /** Character offset of the word within the line. */
  offsetInLine: number;
  /** Absolute character offset within the full document. */
  offsetInDoc: number;
  /** Syntax node type from the editor's syntax tree (if available). */
  syntaxType?: string;
}

/**
 * Decides whether a word should be skipped during spell checking.
 *
 * The checker evaluates all registered filters — if ANY filter returns
 * true, the word is skipped.
 */
export interface TokenFilter {
  /** Unique filter ID (e.g. 'url', 'camelCase', 'code-block'). */
  id: string;
  /** Human-readable label. */
  label?: string;
  /** Return true to skip this word. */
  shouldSkip(word: string, context: TokenContext): boolean;
}

// ── Spell Check Result ───────────────────────────────────────────────────────

/** A single misspelled word with its position and suggestions. */
export interface SpellCheckDiagnostic {
  /** The misspelled word. */
  word: string;
  /** Absolute start offset in the document. */
  from: number;
  /** Absolute end offset in the document. */
  to: number;
  /** Suggested corrections (ordered by likelihood). */
  suggestions: string[];
}

// ── Personal Dictionary ──────────────────────────────────────────────────────

/** Minimal personal dictionary surface needed by the extension. */
export interface PersonalDictionary {
  /** Whether a word is known (either in the dictionary or ignored). */
  isKnown(word: string): boolean;
}

// ── Spell Checker ────────────────────────────────────────────────────────────

/**
 * Consumer-side SpellChecker interface.
 *
 * The extension only needs the ability to check text and manage words.
 * Dictionary loading, backend selection, and registry wiring are the
 * caller's responsibility — the extension never calls loadDictionary().
 */
export interface SpellChecker {
  /** Whether the dictionary has been loaded and is ready for checking. */
  readonly isLoaded: boolean;

  /** The personal dictionary (if any). Presence enables "Add to dictionary". */
  readonly personal: PersonalDictionary | undefined;

  /**
   * Check an entire text string. Returns diagnostics for misspelled words.
   *
   * @param text - The full document text.
   * @param options.syntaxTypes - Map of character offset to syntax node type.
   * @param options.filters - Additional one-shot filters for this call only.
   */
  checkText(
    text: string,
    options?: {
      syntaxTypes?: Map<number, string>;
      filters?: TokenFilter[];
    },
  ): SpellCheckDiagnostic[];

  /** Add a word to the personal dictionary (persisted). */
  addToPersonal(word: string): Promise<void> | void;

  /** Ignore a word for this session only. */
  ignoreWord(word: string): void;
}
