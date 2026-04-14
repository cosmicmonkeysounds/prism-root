/**
 * @prism/core/help — context-sensitive help system.
 *
 * Port of the legacy Helm help system per ADR-005:
 * - `HelpRegistry` is a global in-memory map of `HelpEntry` objects.
 *   Packages register entries as import side-effects.
 * - `HelpTooltip` is a hover popover; `DocSheet` is a slide-in panel;
 *   `DocSearch` is a search-over-registry input. All three consume the
 *   same `HelpEntry` shape.
 * - `HelpProvider` / `useHelp()` decouple the "View full docs" action
 *   from any specific backend — the mounting app passes `onOpenDoc`.
 * - Markdown rendering (`HelpMarkdown`) flows through
 *   `parseMarkdown` from `@prism/core/forms`, matching the canonical
 *   tokenizer already used by `@prism/core/markdown`.
 */

export type { HelpEntry } from "./types.js";
export { HelpRegistry } from "./help-registry.js";
export {
  HelpProvider,
  useHelp,
  type HelpContextValue,
} from "./help-context.js";
export { HelpTooltip, type HelpTooltipProps } from "./help-tooltip.js";
export { DocSheet, type DocSheetProps } from "./doc-sheet.js";
export { DocSearch, type DocSearchProps } from "./doc-search.js";
export { HelpMarkdown, slugify } from "./help-markdown.js";
