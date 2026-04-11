/**
 * markdownLivePreview -- CodeMirror 6 extension for live Markdown preview.
 *
 * Renders markdown formatting inline ("live preview" / Obsidian-style):
 *   - `**bold**` / `__bold__`   -> hides markers, bolds the content
 *   - `*italic*` / `_italic_`   -> hides markers, italicises the content
 *   - `# Heading` / `## ...`    -> hides `#` prefix, applies heading class
 *   - `` `code` ``              -> hides backticks, applies code class
 *   - `[[wiki-link]]`           -> renders as an inline chip widget
 *   - `> blockquote`            -> applies blockquote line class
 *   - `---` / `===` alone       -> replaces line with an <hr> widget
 *
 * Raw syntax is always shown on the line that holds the cursor.
 *
 * Usage:
 *   extensions={[markdownLivePreview()]}
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

// -- Widget: inline wiki-link chip --------------------------------------------

class WikiLinkChip extends WidgetType {
  constructor(private readonly text: string) {
    super();
  }

  override toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = "cm-md-wikilink";
    span.textContent = this.text;
    span.title = `Link: ${this.text}`;
    return span;
  }

  override eq(other: WikiLinkChip): boolean {
    return other.text === this.text;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

// -- Widget: horizontal rule --------------------------------------------------

class HrWidget extends WidgetType {
  override toDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = "cm-md-hr";
    el.setAttribute("aria-hidden", "true");
    return el;
  }

  override eq(): boolean {
    return true;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

// -- Helpers ------------------------------------------------------------------

/** Returns true if the cursor (any selection head) is on the given line number. */
function cursorOnLine(view: EditorView, lineNum: number): boolean {
  for (const range of view.state.selection.ranges) {
    const head = view.state.doc.lineAt(range.head);
    if (head.number === lineNum) return true;
  }
  return false;
}

// -- Inline decoration collector ----------------------------------------------

interface InlineMark {
  from: number;
  to: number;
  dec: Decoration;
}

/**
 * Scan `text` for inline markdown constructs and add decorations.
 * `lineBase` is the document offset of `text[0]`.
 */
function addInlineDecorations(
  builder: RangeSetBuilder<Decoration>,
  _startOffset: number,
  text: string,
  lineBase: number,
): void {
  const marks: InlineMark[] = [];
  let i = 0;

  while (i < text.length) {
    const ch = text[i];
    const rest = text.slice(i);

    // -- Wiki-link [[...|...]] ------------------------------------------------
    if (rest.startsWith("[[")) {
      const closeIdx = rest.indexOf("]]");
      if (closeIdx !== -1) {
        const inner = rest.slice(2, closeIdx);
        const display = inner.includes("|")
          ? (inner.split("|")[1] ?? inner)
          : inner;
        const docFrom = lineBase + i;
        const docTo = lineBase + i + closeIdx + 2;
        marks.push({
          from: docFrom,
          to: docTo,
          dec: Decoration.replace({ widget: new WikiLinkChip(display) }),
        });
        i += closeIdx + 2;
        continue;
      }
    }

    // -- Bold: **...** or __...__ ---------------------------------------------
    if (rest.startsWith("**") || rest.startsWith("__")) {
      const marker = rest.slice(0, 2);
      const closeIdx = rest.indexOf(marker, 2);
      if (closeIdx !== -1 && closeIdx > 2) {
        const docFrom = lineBase + i;
        const contentFrom = docFrom + 2;
        const contentTo = lineBase + i + closeIdx;
        const docTo = contentTo + 2;
        marks.push({ from: docFrom, to: contentFrom, dec: Decoration.replace({}) });
        marks.push({ from: contentFrom, to: contentTo, dec: Decoration.mark({ class: "cm-md-bold" }) });
        marks.push({ from: contentTo, to: docTo, dec: Decoration.replace({}) });
        i += closeIdx + 2;
        continue;
      }
    }

    // -- Italic: *...* or _..._ (single, not double) --------------------------
    if (
      (ch === "*" && text[i + 1] !== "*") ||
      (ch === "_" && text[i + 1] !== "_")
    ) {
      const marker = ch;
      const closeIdx = text.indexOf(marker, i + 1);
      if (closeIdx !== -1 && closeIdx > i + 1 && text[closeIdx + 1] !== marker) {
        const docFrom = lineBase + i;
        const contentFrom = docFrom + 1;
        const contentTo = lineBase + closeIdx;
        const docTo = contentTo + 1;
        marks.push({ from: docFrom, to: contentFrom, dec: Decoration.replace({}) });
        marks.push({ from: contentFrom, to: contentTo, dec: Decoration.mark({ class: "cm-md-italic" }) });
        marks.push({ from: contentTo, to: docTo, dec: Decoration.replace({}) });
        i = closeIdx + 1;
        continue;
      }
    }

    // -- Inline code: `...` ---------------------------------------------------
    if (ch === "`") {
      const closeIdx = text.indexOf("`", i + 1);
      if (closeIdx !== -1 && closeIdx > i + 1) {
        const docFrom = lineBase + i;
        const contentFrom = docFrom + 1;
        const contentTo = lineBase + closeIdx;
        const docTo = contentTo + 1;
        marks.push({ from: docFrom, to: contentFrom, dec: Decoration.replace({}) });
        marks.push({ from: contentFrom, to: contentTo, dec: Decoration.mark({ class: "cm-md-code" }) });
        marks.push({ from: contentTo, to: docTo, dec: Decoration.replace({}) });
        i = closeIdx + 1;
        continue;
      }
    }

    // -- Strikethrough: ~~...~~ -----------------------------------------------
    if (rest.startsWith("~~")) {
      const closeIdx = rest.indexOf("~~", 2);
      if (closeIdx !== -1 && closeIdx > 2) {
        const docFrom = lineBase + i;
        const contentFrom = docFrom + 2;
        const contentTo = lineBase + i + closeIdx;
        const docTo = contentTo + 2;
        marks.push({ from: docFrom, to: contentFrom, dec: Decoration.replace({}) });
        marks.push({ from: contentFrom, to: contentTo, dec: Decoration.mark({ class: "cm-md-strike" }) });
        marks.push({ from: contentTo, to: docTo, dec: Decoration.replace({}) });
        i += closeIdx + 2;
        continue;
      }
    }

    i++;
  }

  // RangeSetBuilder requires non-overlapping ranges in ascending order.
  marks.sort((a, b) => a.from - b.from || a.to - b.to);
  for (const { from, to, dec } of marks) {
    builder.add(from, to, dec);
  }
}

// -- Decoration builder -------------------------------------------------------

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc;

  for (let ln = 1; ln <= doc.lines; ln++) {
    const line = doc.line(ln);
    const text = line.text;
    const base = line.from;

    // Raw syntax on active line -- skip all decorations
    if (cursorOnLine(view, ln)) continue;

    // Blank lines -- nothing to do
    if (text.trim() === "") continue;

    // -- Horizontal rule ------------------------------------------------------
    if (/^(\s*)(---+|===+)\s*$/.test(text)) {
      builder.add(
        base,
        line.to,
        Decoration.replace({ widget: new HrWidget(), block: false }),
      );
      continue;
    }

    // -- Heading: # / ## / ### ------------------------------------------------
    const headingMatch = text.match(/^(#{1,6})\s/);
    if (headingMatch) {
      const hashes = (headingMatch[1] ?? '').length;
      // Replace "# " prefix with nothing, mark rest as heading
      builder.add(base, base + hashes + 1, Decoration.replace({}));
      builder.add(
        base + hashes + 1,
        line.to,
        Decoration.mark({ class: `cm-md-h${hashes}` }),
      );
      // Process inline formatting within the heading text
      addInlineDecorations(
        builder,
        base + hashes + 1,
        text.slice(hashes + 1),
        base + hashes + 1,
      );
      continue;
    }

    // -- Blockquote: > --------------------------------------------------------
    if (/^\s*> /.test(text)) {
      const gtIdx = text.indexOf(">");
      builder.add(base + gtIdx, base + gtIdx + 2, Decoration.replace({}));
      builder.add(
        line.from,
        line.to,
        Decoration.line({ class: "cm-md-blockquote" }),
      );
      addInlineDecorations(
        builder,
        base + gtIdx + 2,
        text.slice(gtIdx + 2),
        base + gtIdx + 2,
      );
      continue;
    }

    // -- Normal line: scan for inline constructs ------------------------------
    addInlineDecorations(builder, base, text, base);
  }

  return builder.finish();
}

// -- Theme --------------------------------------------------------------------

const markdownLivePreviewTheme = EditorView.baseTheme({
  ".cm-md-h1": {
    fontSize: "1.45em",
    fontWeight: "700",
    fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    color: "var(--cm-md-heading-color, #e4e4e7)",
    lineHeight: "1.3",
  },
  ".cm-md-h2": {
    fontSize: "1.25em",
    fontWeight: "600",
    fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    color: "var(--cm-md-heading-color, #e4e4e7)",
  },
  ".cm-md-h3": {
    fontSize: "1.1em",
    fontWeight: "600",
    fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    color: "var(--cm-md-heading-color, #d4d4d8)",
  },
  ".cm-md-h4, .cm-md-h5, .cm-md-h6": {
    fontWeight: "600",
    fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    color: "var(--cm-md-heading-color, #a1a1aa)",
  },
  ".cm-md-bold": {
    fontWeight: "700",
    color: "var(--cm-md-bold-color, inherit)",
  },
  ".cm-md-italic": {
    fontStyle: "italic",
    color: "var(--cm-md-italic-color, inherit)",
  },
  ".cm-md-strike": {
    textDecoration: "line-through",
    color: "#71717a",
  },
  ".cm-md-code": {
    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
    fontSize: "0.88em",
    backgroundColor: "rgba(255,255,255,0.08)",
    borderRadius: "3px",
    padding: "1px 4px",
    color: "#86efac",
  },
  ".cm-md-blockquote": {
    borderLeft: "2px solid rgba(139,92,246,0.5)",
    paddingLeft: "12px",
    color: "#71717a",
    fontStyle: "italic",
  },
  ".cm-md-hr": {
    display: "block",
    width: "100%",
    height: "1px",
    backgroundColor: "#27272a",
    margin: "4px 0",
  },
  ".cm-md-wikilink": {
    display: "inline-flex",
    alignItems: "center",
    backgroundColor: "rgba(20,184,166,0.12)",
    color: "rgb(94,234,212)",
    border: "1px solid rgba(20,184,166,0.25)",
    borderRadius: "4px",
    padding: "0 5px",
    fontSize: "0.875em",
    cursor: "pointer",
    userSelect: "none",
    fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
  },
  ".cm-md-wikilink:hover": {
    backgroundColor: "rgba(20,184,166,0.2)",
    borderColor: "rgba(20,184,166,0.4)",
  },
});

// -- Exported extension -------------------------------------------------------

/**
 * Returns a set of CodeMirror extensions that render Markdown inline.
 * Raw syntax is shown on the line containing the cursor.
 */
export function markdownLivePreview(): Extension[] {
  return [
    ViewPlugin.define<{ decorations: DecorationSet }>(
      (view) => ({
        decorations: buildDecorations(view),
        update(update: ViewUpdate) {
          if (
            update.docChanged ||
            update.selectionSet ||
            update.viewportChanged
          ) {
            this.decorations = buildDecorations(update.view);
          }
        },
      }),
      { decorations: (v) => v.decorations },
    ),
    markdownLivePreviewTheme,
  ];
}
