/**
 * Surface type primitives for the unified LanguageRegistry.
 *
 * These were previously co-located with `DocumentSurfaceRegistry` under
 * `language/syntax/document-types.ts`. ADR-002 §A2 collapsed the parser
 * registry and the surface registry into a single `LanguageContribution`
 * record, so the primitive building blocks live with the registry rather
 * than the syntax engine.
 *
 * Surface modes:
 *   code        — CodeMirror raw syntax editing
 *   preview     — CodeMirror live-preview (rendered on inactive lines)
 *   form        — Schema-driven field inputs
 *   spreadsheet — Grid editing for tabular data
 *   report      — Full HTML layout engine
 */

// ── Surface Mode ────────────────────────────────────────────────────────────

/**
 * Editing modes available in the document surface.
 * Prism uses CodeMirror 6 exclusively — no richtext / TipTap mode.
 */
export type SurfaceMode = "code" | "preview" | "form" | "spreadsheet" | "report";

// ── Inline Token Definition ─────────────────────────────────────────────────

/**
 * Defines a pattern-matched inline token that renders identically across
 * all surface modes (code marks, preview chips, form chips).
 *
 * Languages contribute these via `LanguageSurface.inlineTokens`.
 *
 * @example
 * ```ts
 * const wikiToken: InlineTokenDef = {
 *   id: 'wikilink',
 *   pattern: /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g,
 *   extract: (m) => ({ display: m[2] ?? m[1], data: { id: m[1] } }),
 *   cssClass: 'pt-token-wikilink',
 *   chipColor: 'teal',
 *   replaceInPreview: true,
 * };
 * ```
 */
export interface InlineTokenDef {
  /** Unique token ID: 'wikilink', 'operand', 'resolve-ref'. */
  id: string;
  /** Global regex with capture groups. MUST have the `g` flag. */
  pattern: RegExp;
  /**
   * Extract display text and structured data from a regex match.
   * Called for every match to determine what to render.
   */
  extract: (match: RegExpExecArray) => {
    display: string;
    data?: Record<string, unknown>;
  };
  /** CSS class applied in code-mode surfaces (CodeMirror mark decoration). */
  cssClass?: string;
  /**
   * Semantic color hint for chip renderers.
   * Palette: 'teal', 'amber', 'violet', 'emerald', 'rose', 'blue', 'zinc'.
   */
  chipColor?: string;
  /** In preview modes, replace raw syntax with a chip widget. */
  replaceInPreview?: boolean;
}

// ── Inline Token Builder ────────────────────────────────────────────────────

/**
 * Fluent builder for concise InlineTokenDef creation.
 *
 * @example
 * ```ts
 * const token = inlineToken('wikilink', /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g)
 *   .extract(m => ({ display: m[2] ?? m[1], data: { raw: m[1] } }))
 *   .css('pt-token-wikilink')
 *   .chip('teal')
 *   .replaceInPreview()
 *   .build();
 * ```
 */
export class InlineTokenBuilder {
  private readonly _def: Partial<InlineTokenDef> & { id: string; pattern: RegExp };

  constructor(id: string, pattern: RegExp) {
    this._def = { id, pattern };
  }

  /** Define how to extract display text and structured data from a regex match. */
  extract(fn: InlineTokenDef["extract"]): this {
    this._def.extract = fn;
    return this;
  }

  /** CSS class applied in code-mode surfaces (CodeMirror mark decoration). */
  css(className: string): this {
    this._def.cssClass = className;
    return this;
  }

  /** Semantic color hint for chip renderers. */
  chip(color: string): this {
    this._def.chipColor = color;
    return this;
  }

  /** In preview modes, replace raw syntax with a chip widget. */
  replaceInPreview(replace = true): this {
    this._def.replaceInPreview = replace;
    return this;
  }

  build(): InlineTokenDef {
    if (!this._def.extract) {
      throw new Error(`InlineTokenBuilder(${this._def.id}): extract is required`);
    }
    return this._def as InlineTokenDef;
  }
}

/**
 * Factory for concise InlineTokenDef creation.
 *
 * @param id      Unique token ID: 'wikilink', 'operand', 'resolve-ref'.
 * @param pattern Global regex with capture groups. MUST have the `g` flag.
 */
export function inlineToken(id: string, pattern: RegExp): InlineTokenBuilder {
  return new InlineTokenBuilder(id, pattern);
}

// ── Built-in inline tokens ──────────────────────────────────────────────────

/** Wiki-link inline token: `[[id|display]]` or `[[id]]`. */
export const WIKILINK_TOKEN: InlineTokenDef = {
  id: "wikilink",
  pattern: /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g,
  extract: (m) => ({
    display: m[2] ?? m[1]?.split(":").pop() ?? m[1] ?? "",
    data: { raw: m[1] ?? "", display: m[2] ?? "" },
  }),
  cssClass: "pt-token-wikilink",
  chipColor: "teal",
  replaceInPreview: true,
};
