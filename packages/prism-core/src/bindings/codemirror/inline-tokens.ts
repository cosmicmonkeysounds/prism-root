/**
 * Inline Token Extensions — CodeMirror 6 decorations for inline tokens.
 *
 * Provides two extension factories that consume InlineTokenDef definitions:
 *
 *   Surface        | Extension
 *   ───────────────┼──────────────────────────────────────────
 *   Code mode      | createTokenMarkExtension()   — CSS class marks
 *   Preview mode   | createTokenPreviewExtension() — widget chip replacements
 *
 * One definition -> every surface. Register once, render everywhere.
 *
 * @example
 * ```ts
 * import { createTokenMarkExtension, inlineTokenTheme } from '@prism/core/codemirror';
 * import { WIKILINK_TOKEN } from '@prism/core/language-registry';
 *
 * const extensions = [
 *   ...createTokenMarkExtension([WIKILINK_TOKEN]),
 *   inlineTokenTheme,
 * ];
 * ```
 */

import {
  EditorView,
  ViewPlugin,
  Decoration,
  WidgetType,
} from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import type { DecorationSet, ViewUpdate } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import type { InlineTokenDef } from "@prism/core/language-registry";

// ── Chip color palette ────────────────────────────────────────────────────────

const CHIP_COLORS: Record<string, { bg: string; fg: string; border: string }> = {
  teal:    { bg: "rgba(20,184,166,0.12)", fg: "rgb(94,234,212)",  border: "rgba(20,184,166,0.25)" },
  amber:   { bg: "rgba(251,191,36,0.12)", fg: "rgb(252,211,77)",  border: "rgba(251,191,36,0.25)" },
  violet:  { bg: "rgba(139,92,246,0.15)", fg: "rgb(196,181,253)", border: "rgba(139,92,246,0.3)" },
  emerald: { bg: "rgba(52,211,153,0.12)", fg: "rgb(110,231,183)", border: "rgba(52,211,153,0.25)" },
  rose:    { bg: "rgba(244,63,94,0.12)",  fg: "rgb(251,113,133)", border: "rgba(244,63,94,0.25)" },
  blue:    { bg: "rgba(59,130,246,0.12)", fg: "rgb(147,197,253)", border: "rgba(59,130,246,0.25)" },
  zinc:    { bg: "rgba(113,113,122,0.12)", fg: "rgb(161,161,170)", border: "rgba(113,113,122,0.25)" },
};

function getChipStyle(color?: string): string {
  const c = CHIP_COLORS[color ?? "zinc"] ?? { bg: "rgba(113,113,122,0.12)", fg: "rgb(161,161,170)", border: "rgba(113,113,122,0.25)" };
  return [
    "display:inline-flex", "align-items:center",
    `background:${c.bg}`, `color:${c.fg}`,
    `border:1px solid ${c.border}`, "border-radius:4px",
    "padding:0 5px", "font-size:0.875em",
    "cursor:pointer", "user-select:none",
    "font-family:'Inter','Segoe UI',system-ui,sans-serif",
  ].join(";");
}

// ── CodeMirror: token chip widget ─────────────────────────────────────────────

class TokenChipWidget extends WidgetType {
  constructor(
    private readonly display: string,
    private readonly tokenId: string,
    private readonly chipColor?: string,
  ) {
    super();
  }

  override toDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = `pt-token-chip pt-token-chip-${this.tokenId}`;
    el.style.cssText = getChipStyle(this.chipColor);
    el.textContent = this.display;
    return el;
  }

  override eq(other: TokenChipWidget): boolean {
    return other.display === this.display && other.tokenId === this.tokenId;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

// ── CodeMirror: mark extension (code mode) ────────────────────────────────────

function buildMarkDecorations(view: EditorView, tokens: InlineTokenDef[]): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const text = view.state.doc.toString();

  const matches: Array<{ from: number; to: number; cls: string }> = [];

  for (const token of tokens) {
    const re = new RegExp(token.pattern.source, token.pattern.flags);
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      matches.push({ from: m.index, to: m.index + m[0].length, cls: token.cssClass ?? '' });
    }
  }

  matches.sort((a, b) => a.from - b.from || a.to - b.to);
  for (const { from, to, cls } of matches) {
    builder.add(from, to, Decoration.mark({ class: cls }));
  }

  return builder.finish();
}

/**
 * Creates a CodeMirror extension that applies CSS class marks to matched
 * inline tokens. For code-mode editing where raw syntax is visible.
 */
export function createTokenMarkExtension(tokens: InlineTokenDef[]): Extension[] {
  if (tokens.length === 0) return [];

  const marksOnly = tokens.filter((t) => t.cssClass);
  if (marksOnly.length === 0) return [];

  return [
    ViewPlugin.define<{ decorations: DecorationSet }>(
      (view) => ({
        decorations: buildMarkDecorations(view, marksOnly),
        update(update: ViewUpdate) {
          if (update.docChanged) {
            this.decorations = buildMarkDecorations(update.view, marksOnly);
          }
        },
      }),
      { decorations: (v) => v.decorations },
    ),
  ];
}

// ── CodeMirror: preview extension (preview mode) ──────────────────────────────

function buildPreviewDecorations(view: EditorView, tokens: InlineTokenDef[]): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc;
  const text = doc.toString();

  // Lines with cursors — show raw syntax
  const activeLines = new Set<number>();
  for (const range of view.state.selection.ranges) {
    activeLines.add(doc.lineAt(range.head).number);
  }

  const replacements: Array<{ from: number; to: number; widget: WidgetType }> = [];

  for (const token of tokens) {
    const re = new RegExp(token.pattern.source, token.pattern.flags);
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      const line = doc.lineAt(m.index);
      if (activeLines.has(line.number)) continue;

      const { display } = token.extract(m);
      replacements.push({
        from: m.index,
        to: m.index + m[0].length,
        widget: new TokenChipWidget(display, token.id, token.chipColor),
      });
    }
  }

  replacements.sort((a, b) => a.from - b.from);
  for (const { from, to, widget } of replacements) {
    builder.add(from, to, Decoration.replace({ widget }));
  }

  return builder.finish();
}

/**
 * Creates a CodeMirror extension that replaces matched inline tokens with
 * chip widgets on inactive lines. For live-preview mode.
 */
export function createTokenPreviewExtension(tokens: InlineTokenDef[]): Extension[] {
  const replaceable = tokens.filter((t) => t.replaceInPreview);
  if (replaceable.length === 0) return [];

  return [
    ViewPlugin.define<{ decorations: DecorationSet }>(
      (view) => ({
        decorations: buildPreviewDecorations(view, replaceable),
        update(update: ViewUpdate) {
          if (update.docChanged || update.selectionSet) {
            this.decorations = buildPreviewDecorations(update.view, replaceable);
          }
        },
      }),
      { decorations: (v) => v.decorations },
    ),
    tokenPreviewTheme,
  ];
}

const tokenPreviewTheme = EditorView.baseTheme({
  ".pt-token-chip": {
    verticalAlign: "baseline",
    lineHeight: "inherit",
  },
  ".pt-token-chip:hover": {
    filter: "brightness(1.2)",
  },
});

// ── Theme for built-in token CSS classes ──────────────────────────────────────

export const inlineTokenTheme = EditorView.baseTheme({
  // ── Link tokens ─────────────────────────────────────────────────────────
  ".pt-token-wikilink": {
    color: "rgb(94,234,212)",
    backgroundColor: "rgba(20,184,166,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
  },
  // ── Expression tokens ──────────────────────────────────────────────────
  ".pt-token-operand": {
    color: "rgb(196,181,253)",
    backgroundColor: "rgba(139,92,246,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  ".pt-token-interpolation": {
    color: "rgb(252,211,77)",
    backgroundColor: "rgba(251,191,36,0.1)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  // ── Reference tokens ──────────────────────────────────────────────────
  ".pt-token-resolve-ref": {
    color: "rgb(252,211,77)",
    backgroundColor: "rgba(251,191,36,0.1)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  ".pt-token-static-ref": {
    color: "rgb(147,197,253)",
    backgroundColor: "rgba(59,130,246,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  // ── Effect tokens ─────────────────────────────────────────────────────
  ".pt-token-trigger": {
    color: "rgb(110,231,183)",
    backgroundColor: "rgba(52,211,153,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  ".pt-token-inline-assign": {
    color: "rgb(251,113,133)",
    backgroundColor: "rgba(244,63,94,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
    fontFamily: "'JetBrains Mono', monospace",
    fontSize: "0.88em",
  },
  // ── Variation tokens ──────────────────────────────────────────────────
  ".pt-token-variation": {
    color: "rgb(110,231,183)",
    backgroundColor: "rgba(52,211,153,0.12)",
    borderRadius: "3px",
    padding: "0 2px",
  },
});
