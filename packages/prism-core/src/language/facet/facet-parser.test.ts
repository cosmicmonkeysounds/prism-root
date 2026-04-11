import { describe, it, expect } from 'vitest';
import { detectFormat, parseValues, serializeValues, inferFields } from './facet-parser.js';

// ── detectFormat ────────────────────────────────────────────────────────────

describe('detectFormat', () => {
  it('detects JSON objects', () => {
    expect(detectFormat('{ "key": "value" }')).toBe('json');
  });

  it('detects JSON arrays', () => {
    expect(detectFormat('[1, 2, 3]')).toBe('json');
  });

  it('detects JSON with leading whitespace', () => {
    expect(detectFormat('  \n  { "a": 1 }')).toBe('json');
  });

  it('detects YAML key-value', () => {
    expect(detectFormat('title: Hello')).toBe('yaml');
  });

  it('detects YAML for plain text', () => {
    expect(detectFormat('some text')).toBe('yaml');
  });

  it('detects YAML for empty string', () => {
    expect(detectFormat('')).toBe('yaml');
  });
});

// ── parseValues ─────────────────────────────────────────────────────────────

describe('parseValues', () => {
  describe('JSON', () => {
    it('parses a JSON object', () => {
      expect(parseValues('{"name": "Alice", "age": 30}', 'json')).toEqual({
        name: 'Alice',
        age: 30,
      });
    });

    it('wraps non-object JSON in _root', () => {
      expect(parseValues('[1, 2]', 'json')).toEqual({ _root: [1, 2] });
    });

    it('wraps null JSON in _root', () => {
      expect(parseValues('null', 'json')).toEqual({ _root: null });
    });

    it('returns empty record for invalid JSON', () => {
      expect(parseValues('{bad', 'json')).toEqual({});
    });

    it('returns empty record for empty string', () => {
      expect(parseValues('', 'json')).toEqual({});
    });
  });

  describe('YAML', () => {
    it('parses simple key-value pairs', () => {
      expect(parseValues('title: Hello\nauthor: Bob', 'yaml')).toEqual({
        title: 'Hello',
        author: 'Bob',
      });
    });

    it('coerces booleans', () => {
      expect(parseValues('draft: true\npublished: false', 'yaml')).toEqual({
        draft: true,
        published: false,
      });
    });

    it('coerces numbers', () => {
      expect(parseValues('count: 42\npi: 3.14', 'yaml')).toEqual({
        count: 42,
        pi: 3.14,
      });
    });

    it('coerces negative numbers', () => {
      expect(parseValues('offset: -5', 'yaml')).toEqual({ offset: -5 });
    });

    it('coerces null variants', () => {
      const result = parseValues('a: null\nb: ~\nc:', 'yaml');
      expect(result.a).toBeNull();
      expect(result.b).toBeNull();
      expect(result.c).toBeNull();
    });

    it('parses inline JSON arrays', () => {
      expect(parseValues('tags: ["a", "b"]', 'yaml')).toEqual({
        tags: ['a', 'b'],
      });
    });

    it('strips quotes from values', () => {
      expect(parseValues('name: "Alice"\nlabel: \'test\'', 'yaml')).toEqual({
        name: 'Alice',
        label: 'test',
      });
    });

    it('skips comments and blank lines', () => {
      const src = '# comment\n\ntitle: Hi\n# another\nauthor: Me';
      expect(parseValues(src, 'yaml')).toEqual({
        title: 'Hi',
        author: 'Me',
      });
    });

    it('returns empty record for whitespace-only input', () => {
      expect(parseValues('   \n  ', 'yaml')).toEqual({});
    });
  });
});

// ── serializeValues ─────────────────────────────────────────────────────────

describe('serializeValues', () => {
  describe('JSON', () => {
    it('serializes to pretty JSON', () => {
      const result = serializeValues({ a: 1, b: 'two' }, 'json', '');
      expect(JSON.parse(result)).toEqual({ a: 1, b: 'two' });
    });
  });

  describe('YAML', () => {
    it('preserves comments from original source', () => {
      const original = '# header comment\ntitle: Old\n# footer';
      const result = serializeValues({ title: 'New' }, 'yaml', original);
      expect(result).toContain('# header comment');
      expect(result).toContain('# footer');
      expect(result).toContain('title: New');
      expect(result).not.toContain('Old');
    });

    it('preserves key ordering from original source', () => {
      const original = 'beta: 2\nalpha: 1';
      const result = serializeValues({ alpha: 10, beta: 20 }, 'yaml', original);
      const lines = result.split('\n');
      const betaIdx = lines.findIndex(l => l.startsWith('beta'));
      const alphaIdx = lines.findIndex(l => l.startsWith('alpha'));
      expect(betaIdx).toBeLessThan(alphaIdx);
    });

    it('appends new keys not in original', () => {
      const original = 'title: Hi';
      const result = serializeValues({ title: 'Hi', newKey: 'val' }, 'yaml', original);
      expect(result).toContain('newKey: val');
    });

    it('serializes null as bare key', () => {
      const result = serializeValues({ empty: null }, 'yaml', 'empty: old');
      expect(result).toBe('empty:');
    });

    it('serializes booleans', () => {
      const result = serializeValues({ flag: true }, 'yaml', 'flag: false');
      expect(result).toBe('flag: true');
    });

    it('serializes numbers', () => {
      const result = serializeValues({ count: 42 }, 'yaml', 'count: 0');
      expect(result).toBe('count: 42');
    });

    it('serializes arrays as inline JSON', () => {
      const result = serializeValues({ tags: ['a', 'b'] }, 'yaml', 'tags: []');
      expect(result).toBe('tags: ["a","b"]');
    });

    it('quotes strings with special characters', () => {
      const result = serializeValues({ note: 'has: colon' }, 'yaml', 'note: old');
      expect(result).toBe('note: "has: colon"');
    });

    it('escapes double quotes in values', () => {
      const result = serializeValues({ note: 'say "hi"' }, 'yaml', 'note: old');
      expect(result).toBe('note: "say \\"hi\\""');
    });

    it('preserves blank lines from original', () => {
      const original = 'a: 1\n\nb: 2';
      const result = serializeValues({ a: 1, b: 2 }, 'yaml', original);
      expect(result).toBe('a: 1\n\nb: 2');
    });
  });
});

// ── inferFields ─────────────────────────────────────────────────────────────

describe('inferFields', () => {
  it('infers boolean fields', () => {
    const fields = inferFields({ draft: true });
    expect(fields).toEqual([{ id: 'draft', label: 'Draft', type: 'boolean' }]);
  });

  it('infers number fields', () => {
    const fields = inferFields({ count: 42 });
    expect(fields).toEqual([{ id: 'count', label: 'Count', type: 'number' }]);
  });

  it('infers tags from arrays', () => {
    const fields = inferFields({ tags: ['a', 'b'] });
    expect(fields).toEqual([{ id: 'tags', label: 'Tags', type: 'tags' }]);
  });

  it('infers url from https prefix', () => {
    const fields = inferFields({ website: 'https://example.com' });
    expect(fields[0]?.type).toBe('url');
  });

  it('infers url from http prefix', () => {
    const fields = inferFields({ link: 'http://example.com' });
    expect(fields[0]?.type).toBe('url');
  });

  it('infers email from value with @ and .', () => {
    const fields = inferFields({ contact: 'alice@example.com' });
    expect(fields[0]?.type).toBe('email');
  });

  it('infers date from YYYY-MM-DD pattern', () => {
    const fields = inferFields({ created: '2024-01-15' });
    expect(fields[0]?.type).toBe('date');
  });

  it('infers date from ISO datetime', () => {
    const fields = inferFields({ updated: '2024-01-15T10:30:00Z' });
    expect(fields[0]?.type).toBe('date');
  });

  it('infers textarea for long strings', () => {
    const longText = 'x'.repeat(81);
    const fields = inferFields({ bio: longText });
    expect(fields[0]?.type).toBe('textarea');
  });

  it('falls back to text for short strings', () => {
    const fields = inferFields({ name: 'Alice' });
    expect(fields[0]?.type).toBe('text');
  });

  it('falls back to text for null values', () => {
    const fields = inferFields({ empty: null });
    expect(fields[0]?.type).toBe('text');
  });

  it('humanizes camelCase keys', () => {
    const fields = inferFields({ firstName: 'Alice' });
    expect(fields[0]?.label).toBe('First Name');
  });

  it('humanizes snake_case keys', () => {
    const fields = inferFields({ first_name: 'Alice' });
    expect(fields[0]?.label).toBe('First name');
  });

  it('humanizes kebab-case keys', () => {
    const fields = inferFields({ 'last-name': 'Smith' });
    expect(fields[0]?.label).toBe('Last name');
  });

  it('handles multiple fields in order', () => {
    const fields = inferFields({ title: 'Hi', count: 5, done: false });
    expect(fields.map(f => f.id)).toEqual(['title', 'count', 'done']);
    expect(fields.map(f => f.type)).toEqual(['text', 'number', 'boolean']);
  });
});
