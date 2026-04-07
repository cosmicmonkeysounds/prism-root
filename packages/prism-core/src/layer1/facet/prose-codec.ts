/**
 * ProseCodec -- bidirectional conversion between Markdown strings
 * and a generic ProseNode document tree.
 *
 * Ported from Helm's markdown-serializer.ts. Uses Prism's own
 * parseMarkdown / parseInline from the forms module as the parsing stage.
 * No TipTap, no ProseMirror, no external dependencies.
 */

import { parseMarkdown, parseInline } from '../forms/markdown.js';
import type { BlockToken, InlineToken } from '../forms/markdown.js';

// -- ProseNode types (generic, framework-agnostic) ----------------------------

export interface ProseNode {
  type: string;
  attrs?: Record<string, unknown>;
  content?: ProseNode[];
  marks?: ProseMark[];
  text?: string;
}

export interface ProseMark {
  type: string;
  attrs?: Record<string, unknown>;
}

// -- Markdown -> ProseNode tree -----------------------------------------------

/** Convert inline tokens to ProseNode inline nodes (text with marks). */
function inlineToNodes(tokens: InlineToken[]): ProseNode[] {
  const nodes: ProseNode[] = [];

  for (const tok of tokens) {
    switch (tok.kind) {
      case 'text':
        nodes.push({ type: 'text', text: tok.text });
        break;

      case 'bold': {
        const children = inlineToNodes(tok.children);
        for (const child of children) {
          const marks: ProseMark[] = [...(child.marks ?? []), { type: 'bold' }];
          nodes.push({ ...child, marks });
        }
        break;
      }

      case 'italic': {
        const children = inlineToNodes(tok.children);
        for (const child of children) {
          const marks: ProseMark[] = [...(child.marks ?? []), { type: 'italic' }];
          nodes.push({ ...child, marks });
        }
        break;
      }

      case 'code':
        nodes.push({
          type: 'text',
          text: tok.text,
          marks: [{ type: 'code' }],
        });
        break;

      case 'link':
        nodes.push({
          type: 'text',
          text: tok.text,
          marks: [{ type: 'link', attrs: { href: tok.href } }],
        });
        break;

      case 'wiki':
        nodes.push({
          type: 'wikilink',
          attrs: { title: tok.display, objectId: tok.id },
        });
        break;
    }
  }

  return nodes;
}

/** Convert a single block token to a ProseNode block node. */
function blockToNode(block: BlockToken): ProseNode | null {
  switch (block.kind) {
    case 'empty':
      return null;

    case 'hr':
      return { type: 'horizontalRule' };

    case 'h1':
      return {
        type: 'heading',
        attrs: { level: 1 },
        content: inlineToNodes(parseInline(block.text)),
      };

    case 'h2':
      return {
        type: 'heading',
        attrs: { level: 2 },
        content: inlineToNodes(parseInline(block.text)),
      };

    case 'h3':
      return {
        type: 'heading',
        attrs: { level: 3 },
        content: inlineToNodes(parseInline(block.text)),
      };

    case 'p':
      return {
        type: 'paragraph',
        content: inlineToNodes(parseInline(block.text)),
      };

    case 'blockquote':
      return {
        type: 'blockquote',
        content: [{
          type: 'paragraph',
          content: inlineToNodes(parseInline(block.text)),
        }],
      };

    case 'code':
      return {
        type: 'codeBlock',
        attrs: block.lang ? { language: block.lang } : undefined,
        content: [{ type: 'text', text: block.text }],
      };

    case 'li':
      return {
        type: 'bulletList',
        content: [{
          type: 'listItem',
          content: [{
            type: 'paragraph',
            content: inlineToNodes(parseInline(block.text)),
          }],
        }],
      };

    case 'oli':
      return {
        type: 'orderedList',
        attrs: { start: block.n },
        content: [{
          type: 'listItem',
          content: [{
            type: 'paragraph',
            content: inlineToNodes(parseInline(block.text)),
          }],
        }],
      };

    case 'task':
      return {
        type: 'taskList',
        content: [{
          type: 'taskItem',
          attrs: { checked: block.checked },
          content: [{
            type: 'paragraph',
            content: inlineToNodes(parseInline(block.text)),
          }],
        }],
      };
  }
}

/**
 * Merge consecutive list blocks of the same kind into single list nodes.
 * The block parser emits one node per item; consumers expect items grouped.
 */
function mergeConsecutiveLists(nodes: ProseNode[]): ProseNode[] {
  const merged: ProseNode[] = [];

  for (const node of nodes) {
    const prev = merged[merged.length - 1];

    if (
      prev && prev.type === node.type &&
      (node.type === 'bulletList' || node.type === 'orderedList' || node.type === 'taskList') &&
      prev.content && node.content
    ) {
      prev.content.push(...node.content);
    } else {
      merged.push(node);
    }
  }

  return merged;
}

/**
 * Convert a Markdown string to a ProseNode document tree.
 *
 * @example
 *   const doc = markdownToNodes('# Hello\n\nSome **bold** text.');
 */
export function markdownToNodes(md: string): ProseNode {
  const blocks = parseMarkdown(md);
  const content: ProseNode[] = [];

  for (const block of blocks) {
    const node = blockToNode(block);
    if (node) content.push(node);
  }

  return {
    type: 'doc',
    content: mergeConsecutiveLists(content),
  };
}

// -- ProseNode tree -> Markdown -----------------------------------------------

/** Serialize ProseNode marks back to markdown inline syntax. */
function serializeMarks(node: ProseNode): string {
  if (!node.text && node.type === 'text') return '';
  if (node.type === 'wikilink') {
    const attrs = node.attrs ?? {};
    const id = String(attrs['objectId'] ?? attrs['title'] ?? '');
    const display = String(attrs['title'] ?? '');
    if (id && display && id !== display) return `[[${id}|${display}]]`;
    return `[[${display || id}]]`;
  }
  if (node.type === 'hardBreak') return '  \n';

  const text = node.text ?? '';
  if (!node.marks?.length) return text;

  let result = text;
  for (const mark of node.marks) {
    switch (mark.type) {
      case 'bold':   result = `**${result}**`; break;
      case 'italic': result = `*${result}*`; break;
      case 'code':   result = `\`${result}\``; break;
      case 'strike': result = `~~${result}~~`; break;
      case 'link': {
        const href = String(mark.attrs?.['href'] ?? '');
        result = `[${result}](${href})`;
        break;
      }
    }
  }
  return result;
}

/** Serialize inline content (array of ProseNode text/mark nodes) to markdown. */
function serializeInline(content: ProseNode[] | undefined): string {
  if (!content) return '';
  return content.map(serializeMarks).join('');
}

/** Serialize a single ProseNode block node to markdown line(s). */
function serializeBlock(node: ProseNode, _depth = 0): string {
  switch (node.type) {
    case 'paragraph':
      return serializeInline(node.content);

    case 'heading': {
      const level = Number(node.attrs?.['level'] ?? 1);
      const prefix = '#'.repeat(Math.min(level, 6));
      return `${prefix} ${serializeInline(node.content)}`;
    }

    case 'blockquote':
      return (node.content ?? [])
        .map(child => `> ${serializeBlock(child, _depth)}`)
        .join('\n');

    case 'codeBlock': {
      const lang = node.attrs?.['language'] ? String(node.attrs['language']) : '';
      const text = serializeInline(node.content);
      return `\`\`\`${lang}\n${text}\n\`\`\``;
    }

    case 'horizontalRule':
      return '---';

    case 'bulletList':
      return (node.content ?? [])
        .map(item => `- ${serializeBlock(item, _depth + 1)}`)
        .join('\n');

    case 'orderedList': {
      let n = Number(node.attrs?.['start'] ?? 1);
      return (node.content ?? [])
        .map(item => `${n++}. ${serializeBlock(item, _depth + 1)}`)
        .join('\n');
    }

    case 'taskList':
      return (node.content ?? [])
        .map(item => {
          const checked = item.attrs?.['checked'] ? 'x' : ' ';
          const inner = (item.content ?? []).map(c => serializeBlock(c, _depth + 1)).join('\n');
          return `- [${checked}] ${inner}`;
        })
        .join('\n');

    case 'listItem':
      return (node.content ?? [])
        .map(child => serializeBlock(child, _depth))
        .join('\n');

    case 'taskItem': {
      // Parent taskList handles the checkbox prefix
      const inner = (node.content ?? []).map(c => serializeBlock(c, _depth)).join('\n');
      return inner;
    }

    case 'text':
      return serializeMarks(node);

    default:
      // Unknown block -- serialize children if present
      if (node.content) {
        return node.content.map(c => serializeBlock(c, _depth)).join('\n');
      }
      return node.text ?? '';
  }
}

/**
 * Convert a ProseNode document tree back to a Markdown string.
 *
 * @example
 *   const md = nodesToMarkdown(doc);
 */
export function nodesToMarkdown(doc: ProseNode): string {
  if (!doc.content) return '';

  const lines: string[] = [];
  for (const block of doc.content) {
    lines.push(serializeBlock(block));
    lines.push(''); // blank line between blocks
  }

  // Trim trailing blank lines
  while (lines.length > 0 && lines[lines.length - 1] === '') lines.pop();
  return lines.join('\n') + '\n';
}
