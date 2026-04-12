/**
 * Markdown `LanguageContribution` — unified registration consumed by
 * `LanguageRegistry.register(createMarkdownContribution())`.
 *
 * The parser reuses `parseMarkdown` from `@prism/core/forms` so there is
 * exactly one markdown tokenizer in the codebase. Block tokens are
 * projected into the generic `RootNode` shape used by every other
 * language in the unified registry: each block becomes a child node
 * with `type` = the block kind (`h1`/`p`/`li`/`code`/…) and its text
 * content stored as the `value` field.
 *
 * The surface enables both `code` and `preview` modes with the built-in
 * wikilink token. The React shell supplies the actual preview renderer
 * — core stays framework-free.
 */

import type { LanguageContribution } from "@prism/core/language-registry";
import { WIKILINK_TOKEN } from "@prism/core/language-registry";
import type { RootNode, SyntaxNode } from "@prism/core/syntax";
import { parseMarkdown } from "@prism/core/forms";

function blockToNode(
  block: ReturnType<typeof parseMarkdown>[number],
): SyntaxNode | null {
  switch (block.kind) {
    case "empty":
      return null;
    case "hr":
      return { type: "hr" };
    case "h1":
    case "h2":
    case "h3":
      return { type: block.kind, value: block.text };
    case "p":
    case "blockquote":
    case "li":
      return { type: block.kind, value: block.text };
    case "oli":
      return {
        type: "oli",
        value: block.text,
        data: { n: block.n },
      };
    case "task":
      return {
        type: "task",
        value: block.text,
        data: { checked: block.checked },
      };
    case "code":
      return {
        type: "code",
        value: block.text,
        ...(block.lang ? { data: { lang: block.lang } } : {}),
      };
  }
}

/** Create the unified Markdown `LanguageContribution`. */
export function createMarkdownContribution(): LanguageContribution {
  return {
    id: "prism:markdown",
    extensions: [".md", ".mdx", ".markdown"],
    displayName: "Markdown",
    mimeType: "text/markdown",

    parse(source: string): RootNode {
      const blocks = parseMarkdown(source);
      const children: SyntaxNode[] = [];
      for (const block of blocks) {
        const node = blockToNode(block);
        if (node) children.push(node);
      }
      return { type: "root", children };
    },

    surface: {
      defaultMode: "preview",
      availableModes: ["code", "preview"],
      inlineTokens: [WIKILINK_TOKEN],
    },
  };
}
