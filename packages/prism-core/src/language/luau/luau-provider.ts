/**
 * SyntaxProvider for Luau — backed by full-moon.
 *
 * Minimal LSP-like surface: `diagnose()` returns real parser diagnostics
 * from full-moon, `complete()` offers the `ui.*` helpers used by the Luau
 * Facet panel, and `hover()` currently returns `null`. The provider is
 * synchronous per the `SyntaxProvider` interface; callers must await
 * `ensureLuauParserLoaded()` before invoking any method.
 */

import type {
  CompletionItem,
  Diagnostic,
  HoverInfo,
  SyntaxProvider,
} from "@prism/core/syntax";
import { isLuauParserReady } from "./wasm-loader.js";
import { validateLuauSync } from "./luau-ast.js";

const UI_COMPLETIONS: CompletionItem[] = [
  {
    label: "ui.label",
    kind: "function",
    detail: "ui.label(text: string)",
    documentation: "Render a static text label.",
    insertText: 'ui.label("")',
  },
  {
    label: "ui.button",
    kind: "function",
    detail: "ui.button(text: string)",
    documentation: "Render a clickable button.",
    insertText: 'ui.button("")',
  },
  {
    label: "ui.badge",
    kind: "function",
    detail: "ui.badge(text: string, color?: string)",
    documentation: "Render a coloured badge.",
    insertText: 'ui.badge("", "default")',
  },
  {
    label: "ui.input",
    kind: "function",
    detail: "ui.input(placeholder: string, value?: string)",
    documentation: "Render a text input field.",
    insertText: 'ui.input("")',
  },
  {
    label: "ui.section",
    kind: "function",
    detail: "ui.section(title: string, { children })",
    documentation: "Render a titled section with child elements.",
    insertText: 'ui.section("", {  })',
  },
  {
    label: "ui.row",
    kind: "function",
    detail: "ui.row({ children })",
    documentation: "Horizontal container for child elements.",
    insertText: "ui.row({  })",
  },
  {
    label: "ui.column",
    kind: "function",
    detail: "ui.column({ children })",
    documentation: "Vertical container for child elements.",
    insertText: "ui.column({  })",
  },
  {
    label: "ui.spacer",
    kind: "function",
    detail: "ui.spacer()",
    documentation: "Empty spacing element.",
    insertText: "ui.spacer()",
  },
  {
    label: "ui.divider",
    kind: "function",
    detail: "ui.divider()",
    documentation: "Horizontal divider line.",
    insertText: "ui.divider()",
  },
];

export function createLuauSyntaxProvider(): SyntaxProvider {
  return {
    name: "luau",

    diagnose(source): Diagnostic[] {
      if (!isLuauParserReady()) return [];
      return validateLuauSync(source);
    },

    complete(source, offset): CompletionItem[] {
      // Offer `ui.*` completions when the cursor is right after `ui.`.
      const prefix = source.slice(Math.max(0, offset - 3), offset);
      if (prefix.endsWith("ui.")) {
        return UI_COMPLETIONS.map((item) => ({ ...item }));
      }
      // Always offer the full list as a fallback so callers can filter
      // on the client side.
      return UI_COMPLETIONS.map((item) => ({ ...item }));
    },

    hover(): HoverInfo | null {
      return null;
    },
  };
}
