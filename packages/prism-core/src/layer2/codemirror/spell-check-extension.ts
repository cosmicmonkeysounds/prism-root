/**
 * CodeMirror 6 spell check extension.
 *
 * Wires a SpellChecker into the CodeMirror lint infrastructure.
 * Produces non-obtrusive info-severity diagnostics with suggestion actions.
 *
 * Two usage patterns:
 *
 *   // 1. Direct — pass a configured SpellChecker
 *   import { spellCheckExtension } from '@prism/core/codemirror';
 *   extensions={[spellCheckExtension({ checker })]}
 *
 *   // 2. Builder — fluent configuration
 *   import { spellCheckExtensionBuilder } from '@prism/core/codemirror';
 *   const ext = spellCheckExtensionBuilder()
 *     .checker(myChecker)
 *     .severity('info')
 *     .delay(400)
 *     .maxSuggestions(8)
 *     .build();
 */

import { linter, type Diagnostic } from "@codemirror/lint";
import { syntaxTree } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type {
  SpellChecker,
  TokenFilter,
} from "../../layer1/syntax/spell-check-types.js";

// ── Types ────────────────────────────────────────────────────────────────────

export interface SpellCheckExtensionConfig {
  /** The SpellChecker instance to use. Must already have loadDictionary() called (or be loading). */
  checker: SpellChecker;
  /** Diagnostic severity. Default: 'info' (subtle dotted underline). */
  severity?: "error" | "warning" | "info" | "hint";
  /** Debounce delay in ms. Default: 300. */
  delay?: number;
  /** Maximum suggestions shown per word. Default: 5. */
  maxSuggestions?: number;
  /** Additional one-shot filters for this extension only. */
  filters?: TokenFilter[];
  /** Whether to use syntax tree info for filter context. Default: true. */
  useSyntaxTree?: boolean;
  /** Source label shown in the lint panel. Default: 'Spelling'. */
  source?: string;
}

// ── Extension factory ────────────────────────────────────────────────────────

/**
 * Create a CodeMirror extension that spell-checks the document content
 * and produces lint diagnostics for misspelled words.
 */
export function spellCheckExtension(
  config: SpellCheckExtensionConfig,
): Extension {
  const {
    checker,
    severity = "info",
    delay = 300,
    maxSuggestions = 5,
    filters,
    useSyntaxTree = true,
    source = "Spelling",
  } = config;

  return linter(
    (view: EditorView): Diagnostic[] => {
      if (!checker.isLoaded) return [];

      const text = view.state.doc.toString();

      // Build syntax type map from the editor's syntax tree
      let syntaxTypes: Map<number, string> | undefined;
      if (useSyntaxTree) {
        syntaxTypes = new Map();
        const tree = syntaxTree(view.state);
        tree.iterate({
          enter(node) {
            syntaxTypes!.set(node.from, node.name);
          },
        });
      }

      const checkOptions: {
        syntaxTypes?: Map<number, string>;
        filters?: TokenFilter[];
      } = {};
      if (syntaxTypes !== undefined) checkOptions.syntaxTypes = syntaxTypes;
      if (filters !== undefined) checkOptions.filters = filters;

      const results = checker.checkText(text, checkOptions);

      return results.map((diag) => {
        const actions: Array<{
          name: string;
          apply: (view: EditorView, from: number, to: number) => void;
        }> = [];

        // Suggestion actions — replace the word
        const suggestions = diag.suggestions.slice(0, maxSuggestions);
        for (const suggestion of suggestions) {
          actions.push({
            name: suggestion,
            apply(view: EditorView, from: number, to: number) {
              view.dispatch({ changes: { from, to, insert: suggestion } });
            },
          });
        }

        // "Add to dictionary" action
        if (checker.personal) {
          actions.push({
            name: "Add to dictionary",
            apply(view: EditorView) {
              checker.addToPersonal(diag.word);
              // Re-lint by dispatching an empty transaction
              requestAnimationFrame(() => {
                view.dispatch({});
              });
            },
          });
        }

        // "Ignore" action (session-only)
        actions.push({
          name: "Ignore",
          apply(view: EditorView) {
            checker.ignoreWord(diag.word);
            requestAnimationFrame(() => {
              view.dispatch({});
            });
          },
        });

        return {
          from: diag.from,
          to: diag.to,
          severity,
          message: `Unknown word: "${diag.word}"`,
          source,
          actions,
        };
      });
    },
    { delay },
  );
}

// ── Builder ──────────────────────────────────────────────────────────────────

export class SpellCheckExtensionBuilder {
  private _config: Partial<SpellCheckExtensionConfig> = {};

  /** Set the SpellChecker instance. Required. */
  checker(checker: SpellChecker): this {
    this._config.checker = checker;
    return this;
  }

  /** Set diagnostic severity. Default: 'info'. */
  severity(severity: "error" | "warning" | "info" | "hint"): this {
    this._config.severity = severity;
    return this;
  }

  /** Set debounce delay in ms. Default: 300. */
  delay(ms: number): this {
    this._config.delay = ms;
    return this;
  }

  /** Set max suggestions per word. Default: 5. */
  maxSuggestions(n: number): this {
    this._config.maxSuggestions = n;
    return this;
  }

  /** Add a one-shot filter for this extension only. */
  filter(filter: TokenFilter): this {
    if (!this._config.filters) this._config.filters = [];
    this._config.filters.push(filter);
    return this;
  }

  /** Add multiple one-shot filters. */
  filters(filters: TokenFilter[]): this {
    if (!this._config.filters) this._config.filters = [];
    this._config.filters.push(...filters);
    return this;
  }

  /** Enable/disable syntax tree usage for filter context. Default: true. */
  useSyntaxTree(enabled: boolean): this {
    this._config.useSyntaxTree = enabled;
    return this;
  }

  /** Set the source label for the lint panel. Default: 'Spelling'. */
  source(label: string): this {
    this._config.source = label;
    return this;
  }

  /** Build the CodeMirror extension. Throws if no checker was set. */
  build(): Extension {
    if (!this._config.checker) {
      throw new Error(
        "SpellCheckExtensionBuilder: checker is required. Call .checker(myChecker) before .build().",
      );
    }
    return spellCheckExtension(
      this._config as SpellCheckExtensionConfig,
    );
  }
}

/** Create a new SpellCheckExtensionBuilder. */
export function spellCheckExtensionBuilder(): SpellCheckExtensionBuilder {
  return new SpellCheckExtensionBuilder();
}
