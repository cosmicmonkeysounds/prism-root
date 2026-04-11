import { describe, it, expect } from 'vitest';
import { markdownToNodes, nodesToMarkdown } from './prose-codec.js';
import type { ProseNode } from './prose-codec.js';

// ---------------------------------------------------------------------------
// markdownToNodes — block parsing
// ---------------------------------------------------------------------------

describe('markdownToNodes', () => {
  describe('headings', () => {
    it('parses h1', () => {
      const doc = markdownToNodes('# Hello');
      expect(doc.content?.[0]).toMatchObject({
        type: 'heading',
        attrs: { level: 1 },
        content: [{ type: 'text', text: 'Hello' }],
      });
    });

    it('parses h2', () => {
      const doc = markdownToNodes('## Sub');
      expect(doc.content?.[0]).toMatchObject({
        type: 'heading',
        attrs: { level: 2 },
        content: [{ type: 'text', text: 'Sub' }],
      });
    });

    it('parses h3', () => {
      const doc = markdownToNodes('### Deep');
      expect(doc.content?.[0]).toMatchObject({
        type: 'heading',
        attrs: { level: 3 },
        content: [{ type: 'text', text: 'Deep' }],
      });
    });
  });

  describe('paragraphs', () => {
    it('parses plain paragraph', () => {
      const doc = markdownToNodes('Hello world');
      expect(doc.content?.[0]).toMatchObject({
        type: 'paragraph',
        content: [{ type: 'text', text: 'Hello world' }],
      });
    });

    it('skips empty lines', () => {
      const doc = markdownToNodes('A\n\nB');
      const blocks = doc.content?.filter(n => n.type !== 'empty') ?? [];
      expect(blocks).toHaveLength(2);
      expect(blocks[0]?.type).toBe('paragraph');
      expect(blocks[1]?.type).toBe('paragraph');
    });
  });

  describe('unordered lists', () => {
    it('parses single bullet item', () => {
      const doc = markdownToNodes('- Item one');
      expect(doc.content?.[0]).toMatchObject({
        type: 'bulletList',
        content: [{
          type: 'listItem',
          content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Item one' }] }],
        }],
      });
    });

    it('merges consecutive bullet items into one list', () => {
      const doc = markdownToNodes('- A\n- B\n- C');
      const list = doc.content?.[0];
      expect(list?.type).toBe('bulletList');
      expect(list?.content).toHaveLength(3);
    });
  });

  describe('ordered lists', () => {
    it('parses ordered list with start number', () => {
      const doc = markdownToNodes('1. First\n2. Second');
      const list = doc.content?.[0];
      expect(list?.type).toBe('orderedList');
      expect(list?.attrs?.['start']).toBe(1);
      expect(list?.content).toHaveLength(2);
    });

    it('preserves start number', () => {
      const doc = markdownToNodes('3. Third');
      expect(doc.content?.[0]?.attrs?.['start']).toBe(3);
    });
  });

  describe('code blocks', () => {
    it('parses code block without language', () => {
      const doc = markdownToNodes('```\nconsole.log("hi")\n```');
      const block = doc.content?.[0];
      expect(block?.type).toBe('codeBlock');
      expect(block?.attrs?.['language']).toBeUndefined();
      expect(block?.content?.[0]?.text).toBe('console.log("hi")');
    });

    it('parses code block with language', () => {
      const doc = markdownToNodes('```typescript\nconst x = 1;\n```');
      const block = doc.content?.[0];
      expect(block?.type).toBe('codeBlock');
      expect(block?.attrs?.['language']).toBe('typescript');
      expect(block?.content?.[0]?.text).toBe('const x = 1;');
    });

    it('preserves multi-line code', () => {
      const doc = markdownToNodes('```\nline1\nline2\nline3\n```');
      expect(doc.content?.[0]?.content?.[0]?.text).toBe('line1\nline2\nline3');
    });
  });

  describe('blockquotes', () => {
    it('parses blockquote', () => {
      const doc = markdownToNodes('> Quoted text');
      const bq = doc.content?.[0];
      expect(bq?.type).toBe('blockquote');
      expect(bq?.content?.[0]?.type).toBe('paragraph');
      expect(bq?.content?.[0]?.content?.[0]?.text).toBe('Quoted text');
    });
  });

  describe('horizontal rules', () => {
    it('parses ---', () => {
      const doc = markdownToNodes('---');
      expect(doc.content?.[0]).toMatchObject({ type: 'horizontalRule' });
    });
  });

  describe('task lists', () => {
    it('parses unchecked task', () => {
      const doc = markdownToNodes('- [ ] Todo');
      const list = doc.content?.[0];
      expect(list?.type).toBe('taskList');
      expect(list?.content?.[0]?.attrs?.['checked']).toBe(false);
    });

    it('parses checked task', () => {
      const doc = markdownToNodes('- [x] Done');
      const list = doc.content?.[0];
      expect(list?.type).toBe('taskList');
      expect(list?.content?.[0]?.attrs?.['checked']).toBe(true);
    });

    it('merges consecutive tasks into one list', () => {
      const doc = markdownToNodes('- [ ] A\n- [x] B\n- [ ] C');
      const list = doc.content?.[0];
      expect(list?.type).toBe('taskList');
      expect(list?.content).toHaveLength(3);
    });
  });

  describe('document structure', () => {
    it('returns doc type at root', () => {
      const doc = markdownToNodes('Hello');
      expect(doc.type).toBe('doc');
      expect(doc.content).toBeDefined();
    });

    it('handles empty input', () => {
      const doc = markdownToNodes('');
      expect(doc.type).toBe('doc');
    });
  });
});

// ---------------------------------------------------------------------------
// markdownToNodes — inline parsing
// ---------------------------------------------------------------------------

describe('inline parsing via markdownToNodes', () => {
  it('parses bold text', () => {
    const doc = markdownToNodes('**bold**');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.text).toBe('bold');
    expect(inline?.marks).toContainEqual({ type: 'bold' });
  });

  it('parses italic text', () => {
    const doc = markdownToNodes('*italic*');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.text).toBe('italic');
    expect(inline?.marks).toContainEqual({ type: 'italic' });
  });

  it('parses inline code', () => {
    const doc = markdownToNodes('`code`');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.text).toBe('code');
    expect(inline?.marks).toContainEqual({ type: 'code' });
  });

  it('parses links', () => {
    const doc = markdownToNodes('[click](https://example.com)');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.text).toBe('click');
    expect(inline?.marks).toContainEqual({
      type: 'link',
      attrs: { href: 'https://example.com' },
    });
  });

  it('parses wiki-links', () => {
    const doc = markdownToNodes('[[MyPage]]');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.type).toBe('wikilink');
    expect(inline?.attrs?.['title']).toBe('MyPage');
    expect(inline?.attrs?.['objectId']).toBe('MyPage');
  });

  it('parses wiki-links with display text', () => {
    const doc = markdownToNodes('[[id123|Display Name]]');
    const inline = doc.content?.[0]?.content?.[0];
    expect(inline?.type).toBe('wikilink');
    expect(inline?.attrs?.['objectId']).toBe('id123');
    expect(inline?.attrs?.['title']).toBe('Display Name');
  });

  it('handles mixed inline content', () => {
    const doc = markdownToNodes('Start **bold** middle *italic* end');
    const content = doc.content?.[0]?.content ?? [];
    expect(content.length).toBeGreaterThanOrEqual(5);
    expect(content[0]?.text).toBe('Start ');
    expect(content[1]?.marks).toContainEqual({ type: 'bold' });
    expect(content[3]?.marks).toContainEqual({ type: 'italic' });
  });

  it('parses bold inside heading', () => {
    const doc = markdownToNodes('# **Title**');
    const heading = doc.content?.[0];
    expect(heading?.type).toBe('heading');
    expect(heading?.content?.[0]?.marks).toContainEqual({ type: 'bold' });
  });
});

// ---------------------------------------------------------------------------
// nodesToMarkdown — serialization
// ---------------------------------------------------------------------------

describe('nodesToMarkdown', () => {
  it('serializes heading', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'heading',
        attrs: { level: 2 },
        content: [{ type: 'text', text: 'Title' }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('## Title\n');
  });

  it('serializes paragraph', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'text', text: 'Hello' }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('Hello\n');
  });

  it('serializes bold mark', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'text', text: 'strong', marks: [{ type: 'bold' }] }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('**strong**\n');
  });

  it('serializes italic mark', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'text', text: 'em', marks: [{ type: 'italic' }] }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('*em*\n');
  });

  it('serializes inline code mark', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'text', text: 'fn()', marks: [{ type: 'code' }] }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('`fn()`\n');
  });

  it('serializes link mark', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{
          type: 'text',
          text: 'click',
          marks: [{ type: 'link', attrs: { href: 'https://x.com' } }],
        }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('[click](https://x.com)\n');
  });

  it('serializes wikilink', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'wikilink', attrs: { title: 'Page', objectId: 'Page' } }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('[[Page]]\n');
  });

  it('serializes wikilink with different id and display', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [{ type: 'wikilink', attrs: { title: 'Display', objectId: 'id123' } }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('[[id123|Display]]\n');
  });

  it('serializes code block with language', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'codeBlock',
        attrs: { language: 'rust' },
        content: [{ type: 'text', text: 'fn main() {}' }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('```rust\nfn main() {}\n```\n');
  });

  it('serializes code block without language', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'codeBlock',
        content: [{ type: 'text', text: 'hello' }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('```\nhello\n```\n');
  });

  it('serializes blockquote', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'blockquote',
        content: [{
          type: 'paragraph',
          content: [{ type: 'text', text: 'Quoted' }],
        }],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('> Quoted\n');
  });

  it('serializes horizontal rule', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{ type: 'horizontalRule' }],
    };
    expect(nodesToMarkdown(doc)).toBe('---\n');
  });

  it('serializes bullet list', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'bulletList',
        content: [
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'A' }] }] },
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'B' }] }] },
        ],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('- A\n- B\n');
  });

  it('serializes ordered list', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'orderedList',
        attrs: { start: 1 },
        content: [
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'First' }] }] },
          { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Second' }] }] },
        ],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('1. First\n2. Second\n');
  });

  it('serializes task list', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [{
        type: 'taskList',
        content: [
          { type: 'taskItem', attrs: { checked: false }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Todo' }] }] },
          { type: 'taskItem', attrs: { checked: true }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Done' }] }] },
        ],
      }],
    };
    expect(nodesToMarkdown(doc)).toBe('- [ ] Todo\n- [x] Done\n');
  });

  it('serializes empty doc', () => {
    const doc: ProseNode = { type: 'doc' };
    expect(nodesToMarkdown(doc)).toBe('');
  });

  it('separates blocks with blank lines', () => {
    const doc: ProseNode = {
      type: 'doc',
      content: [
        { type: 'paragraph', content: [{ type: 'text', text: 'A' }] },
        { type: 'paragraph', content: [{ type: 'text', text: 'B' }] },
      ],
    };
    expect(nodesToMarkdown(doc)).toBe('A\n\nB\n');
  });
});

// ---------------------------------------------------------------------------
// Round-trip tests
// ---------------------------------------------------------------------------

describe('round-trip: markdownToNodes -> nodesToMarkdown', () => {
  const roundTrip = (md: string): string => nodesToMarkdown(markdownToNodes(md));

  it('round-trips heading', () => {
    expect(roundTrip('# Hello')).toBe('# Hello\n');
  });

  it('round-trips paragraph', () => {
    expect(roundTrip('Just text')).toBe('Just text\n');
  });

  it('round-trips bold', () => {
    expect(roundTrip('**bold**')).toBe('**bold**\n');
  });

  it('round-trips italic', () => {
    expect(roundTrip('*italic*')).toBe('*italic*\n');
  });

  it('round-trips inline code', () => {
    expect(roundTrip('`code`')).toBe('`code`\n');
  });

  it('round-trips link', () => {
    expect(roundTrip('[text](https://example.com)')).toBe('[text](https://example.com)\n');
  });

  it('round-trips wiki-link', () => {
    expect(roundTrip('[[Page]]')).toBe('[[Page]]\n');
  });

  it('round-trips code block with language', () => {
    expect(roundTrip('```js\nalert(1)\n```')).toBe('```js\nalert(1)\n```\n');
  });

  it('round-trips blockquote', () => {
    expect(roundTrip('> Quote')).toBe('> Quote\n');
  });

  it('round-trips horizontal rule', () => {
    expect(roundTrip('---')).toBe('---\n');
  });

  it('round-trips bullet list', () => {
    expect(roundTrip('- A\n- B')).toBe('- A\n- B\n');
  });

  it('round-trips ordered list', () => {
    expect(roundTrip('1. First\n2. Second')).toBe('1. First\n2. Second\n');
  });

  it('round-trips task list', () => {
    expect(roundTrip('- [ ] Todo\n- [x] Done')).toBe('- [ ] Todo\n- [x] Done\n');
  });

  it('round-trips mixed document', () => {
    const md = [
      '# Title',
      '',
      'A paragraph with **bold** and *italic*.',
      '',
      '- Item 1',
      '- Item 2',
      '',
      '> A quote',
      '',
      '```ts',
      'const x = 1;',
      '```',
    ].join('\n');

    const result = roundTrip(md);
    expect(result).toContain('# Title');
    expect(result).toContain('**bold**');
    expect(result).toContain('*italic*');
    expect(result).toContain('- Item 1');
    expect(result).toContain('- Item 2');
    expect(result).toContain('> A quote');
    expect(result).toContain('```ts');
    expect(result).toContain('const x = 1;');
  });
});
