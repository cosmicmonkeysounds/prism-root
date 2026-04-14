import {
  parseMarkdown,
  parseInline,
  type BlockToken,
  type InlineToken,
} from "@prism/core/forms";
import { Fragment, type ReactElement, type ReactNode } from "react";

/**
 * Render a markdown string as React nodes using the canonical Prism
 * tokenizer from `@prism/core/forms`.
 *
 * Keeps the help system's markdown rendering on the same parser used by
 * `@prism/core/markdown`'s LanguageContribution — no private regex, no
 * second copy of the tokenizer (per the "Parsing goes through Prism
 * Syntax" constraint from ADR-005 and the user's feedback memory).
 *
 * Supports the full BlockToken grammar: h1/h2/h3, paragraphs, code blocks
 * (with optional language label), horizontal rules, blockquotes,
 * unordered / ordered / task list items, and the six inline token kinds
 * including wiki-links.
 *
 * Consecutive `li`/`oli`/`task` tokens are grouped into a single <ul>/<ol>
 * so the output is structurally correct HTML.
 */
export function HelpMarkdown({ source }: { source: string }): ReactElement {
  const blocks = parseMarkdown(source);
  return <Fragment>{renderBlocks(blocks)}</Fragment>;
}

type ListKind = "ul" | "ol";

function renderBlocks(blocks: BlockToken[]): ReactNode[] {
  const out: ReactNode[] = [];
  let i = 0;
  let key = 0;
  while (i < blocks.length) {
    const block = blocks[i];
    if (!block) break;
    if (block.kind === "li" || block.kind === "task") {
      const items: BlockToken[] = [];
      let next = blocks[i];
      while (next && (next.kind === "li" || next.kind === "task")) {
        items.push(next);
        i += 1;
        next = blocks[i];
      }
      out.push(renderList("ul", items, key++));
      continue;
    }
    if (block.kind === "oli") {
      const items: BlockToken[] = [];
      let next = blocks[i];
      while (next && next.kind === "oli") {
        items.push(next);
        i += 1;
        next = blocks[i];
      }
      out.push(renderList("ol", items, key++));
      continue;
    }
    out.push(renderBlock(block, key++));
    i += 1;
  }
  return out;
}

function renderBlock(block: BlockToken, key: number): ReactNode {
  switch (block.kind) {
    case "empty":
      return null;
    case "hr":
      return <hr key={key} className="prism-help-hr" />;
    case "h1":
      return (
        <h1 key={key} id={slugify(block.text)} data-anchor={slugify(block.text)}>
          {renderInline(parseInline(block.text))}
        </h1>
      );
    case "h2":
      return (
        <h2 key={key} id={slugify(block.text)} data-anchor={slugify(block.text)}>
          {renderInline(parseInline(block.text))}
        </h2>
      );
    case "h3":
      return (
        <h3 key={key} id={slugify(block.text)} data-anchor={slugify(block.text)}>
          {renderInline(parseInline(block.text))}
        </h3>
      );
    case "p":
      return <p key={key}>{renderInline(parseInline(block.text))}</p>;
    case "blockquote":
      return (
        <blockquote key={key}>
          {renderInline(parseInline(block.text))}
        </blockquote>
      );
    case "code":
      return (
        <pre key={key} className="prism-help-code" data-lang={block.lang ?? undefined}>
          <code>{block.text}</code>
        </pre>
      );
    case "li":
    case "oli":
    case "task":
      // Handled by renderList via the grouping pass.
      return null;
  }
}

function renderList(
  kind: ListKind,
  items: BlockToken[],
  key: number,
): ReactElement {
  const Tag = kind;
  return (
    <Tag key={key}>
      {items.map((item, idx) => {
        if (item.kind === "task") {
          return (
            <li key={idx} className="prism-help-task">
              <input type="checkbox" checked={item.checked} readOnly />
              <span>{renderInline(parseInline(item.text))}</span>
            </li>
          );
        }
        if (item.kind === "oli" || item.kind === "li") {
          return <li key={idx}>{renderInline(parseInline(item.text))}</li>;
        }
        return null;
      })}
    </Tag>
  );
}

function renderInline(tokens: InlineToken[]): ReactNode[] {
  return tokens.map((token, idx) => renderInlineToken(token, idx));
}

function renderInlineToken(token: InlineToken, key: number): ReactNode {
  switch (token.kind) {
    case "text":
      return <Fragment key={key}>{token.text}</Fragment>;
    case "bold":
      return <strong key={key}>{renderInline(token.children)}</strong>;
    case "italic":
      return <em key={key}>{renderInline(token.children)}</em>;
    case "code":
      return <code key={key}>{token.text}</code>;
    case "link":
      return (
        <a key={key} href={token.href} target="_blank" rel="noopener noreferrer">
          {token.text}
        </a>
      );
    case "wiki":
      return (
        <span key={key} className="prism-help-wikilink" data-wiki-id={token.id}>
          {token.display}
        </span>
      );
  }
}

/**
 * Produce a deterministic anchor slug from a heading. Lower-case, strip
 * punctuation, replace whitespace with hyphens. Exported for tests.
 */
export function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-");
}
