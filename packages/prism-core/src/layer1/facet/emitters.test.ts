import { describe, it, expect } from 'vitest';
import {
  TypeScriptWriter,
  JavaScriptWriter,
  CSharpWriter,
  LuauWriter,
  JsonWriter,
  YamlWriter,
  TomlWriter,
  emitTypeScript,
  emitJavaScript,
  emitCSharp,
  emitLuau,
  serializeJson,
  serializeYaml,
  serializeToml,
} from './emitters.js';
import type {
  SchemaModel,
  SchemaInterface,
  SchemaEnum,
} from './emitters.js';
import type { CodegenMeta } from '../syntax/codegen/codegen-types.js';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const META: CodegenMeta = { projectName: 'test-project' };

function makeModel(overrides?: Partial<SchemaModel>): SchemaModel {
  return {
    namespace: 'TestNs',
    declarations: [
      {
        kind: 'interface',
        name: 'User',
        fields: [
          { name: 'id', type: 'string' },
          { name: 'age', type: 'number', optional: true },
          { name: 'tags', type: 'string', array: true },
        ],
      } satisfies SchemaInterface,
      {
        kind: 'enum',
        name: 'Role',
        values: ['Admin', 'Editor', 'Viewer'],
      } satisfies SchemaEnum,
    ],
    ...overrides,
  };
}

function makeClassModel(): SchemaModel {
  return {
    declarations: [
      {
        kind: 'class',
        name: 'Widget',
        fields: [
          { name: 'label', type: 'string', defaultValue: 'untitled' },
          { name: 'count', type: 'number', optional: true },
        ],
        extends: ['BaseWidget'],
      } satisfies SchemaInterface,
    ],
  };
}

function makeRecordModel(): SchemaModel {
  return {
    declarations: [
      {
        kind: 'record',
        name: 'Point',
        fields: [
          { name: 'x', type: 'number' },
          { name: 'y', type: 'number' },
        ],
      } satisfies SchemaInterface,
    ],
  };
}

// ---------------------------------------------------------------------------
// TypeScript Writer
// ---------------------------------------------------------------------------

describe('TypeScriptWriter', () => {
  it('emits interface with fields', () => {
    const src = emitTypeScript(makeModel());
    expect(src).toContain('export interface User');
    expect(src).toContain('id: string;');
    expect(src).toContain('age?: number;');
    expect(src).toContain('tags: string[];');
  });

  it('emits enum', () => {
    const src = emitTypeScript(makeModel());
    expect(src).toContain('export enum Role');
    expect(src).toContain("Admin = 'Admin',");
    expect(src).toContain("Viewer = 'Viewer',");
  });

  it('emits namespace comment', () => {
    const src = emitTypeScript(makeModel());
    expect(src).toContain('// @module TestNs');
  });

  it('emits class with extends and default values', () => {
    const src = emitTypeScript(makeClassModel());
    expect(src).toContain('export class Widget extends BaseWidget');
    expect(src).toContain('label: string = "untitled";');
  });

  it('emits interface with extends', () => {
    const model: SchemaModel = {
      declarations: [{
        kind: 'interface',
        name: 'Admin',
        fields: [{ name: 'level', type: 'number' }],
        extends: ['User', 'Serializable'],
      }],
    };
    const src = emitTypeScript(model);
    expect(src).toContain('export interface Admin extends User, Serializable');
  });

  it('emits field descriptions as comments', () => {
    const model: SchemaModel = {
      declarations: [{
        kind: 'interface',
        name: 'Foo',
        fields: [{ name: 'bar', type: 'string', description: 'The bar field' }],
      }],
    };
    const src = emitTypeScript(model);
    expect(src).toContain('// The bar field');
  });

  it('Writer class produces correct filename and language', () => {
    const writer = new TypeScriptWriter();
    const result = writer.emit(makeModel(), META);
    expect(result.errors).toHaveLength(0);
    expect(result.files).toHaveLength(1);
    expect(result.files[0]?.filename).toBe('test-project.ts');
    expect(result.files[0]?.language).toBe('typescript');
  });

  it('Writer class accepts custom filename', () => {
    const writer = new TypeScriptWriter({ filename: 'custom.ts' });
    const result = writer.emit(makeModel(), META);
    expect(result.files[0]?.filename).toBe('custom.ts');
  });
});

// ---------------------------------------------------------------------------
// JavaScript Writer
// ---------------------------------------------------------------------------

describe('JavaScriptWriter', () => {
  it('emits enum as const object', () => {
    const src = emitJavaScript(makeModel());
    expect(src).toContain('export const Role = {');
    expect(src).toContain("Admin: 'Admin',");
  });

  it('emits interface as JSDoc @typedef', () => {
    const src = emitJavaScript(makeModel());
    expect(src).toContain('@typedef {Object} User');
    expect(src).toContain('@property {string} id');
    expect(src).toContain('@property {number} [age]');
    expect(src).toContain('@property {string[]} tags');
  });

  it('includes field description in @property', () => {
    const model: SchemaModel = {
      declarations: [{
        kind: 'interface',
        name: 'Foo',
        fields: [{ name: 'x', type: 'number', description: 'coordinate' }],
      }],
    };
    const src = emitJavaScript(model);
    expect(src).toContain('@property {number} x - coordinate');
  });

  it('Writer class produces .js file', () => {
    const writer = new JavaScriptWriter();
    const result = writer.emit(makeModel(), META);
    expect(result.files[0]?.filename).toBe('test-project.js');
    expect(result.files[0]?.language).toBe('javascript');
  });
});

// ---------------------------------------------------------------------------
// C# Writer
// ---------------------------------------------------------------------------

describe('CSharpWriter', () => {
  it('wraps output in namespace', () => {
    const src = emitCSharp(makeModel());
    expect(src).toContain('namespace TestNs {');
  });

  it('uses default namespace "Generated" when none provided', () => {
    const src = emitCSharp({ declarations: [] });
    expect(src).toContain('namespace Generated {');
  });

  it('emits interface with C# types', () => {
    const src = emitCSharp(makeModel());
    expect(src).toContain('public interface User');
    expect(src).toContain('string Id');
    expect(src).toContain('double? Age');
    expect(src).toContain('List<string> Tags');
  });

  it('emits enum', () => {
    const src = emitCSharp(makeModel());
    expect(src).toContain('public enum Role');
    expect(src).toContain('Admin,');
  });

  it('emits record as positional', () => {
    const src = emitCSharp(makeRecordModel());
    expect(src).toContain('public record Point(double X, double Y);');
  });

  it('emits class with extends', () => {
    const src = emitCSharp(makeClassModel());
    expect(src).toContain('public class Widget : BaseWidget');
  });

  it('maps TS types to C# types', () => {
    const model: SchemaModel = {
      declarations: [{
        kind: 'interface',
        name: 'Types',
        fields: [
          { name: 'a', type: 'boolean' },
          { name: 'b', type: 'Date' },
          { name: 'c', type: 'int' },
          { name: 'd', type: 'unknown' },
          { name: 'e', type: 'CustomType' },
        ],
      }],
    };
    const src = emitCSharp(model);
    expect(src).toContain('bool A');
    expect(src).toContain('DateTime B');
    expect(src).toContain('int C');
    expect(src).toContain('object D');
    expect(src).toContain('CustomType E');
  });

  it('Writer class produces .cs file', () => {
    const writer = new CSharpWriter();
    const result = writer.emit(makeModel(), META);
    expect(result.files[0]?.filename).toBe('test-project.cs');
    expect(result.files[0]?.language).toBe('csharp');
  });
});

// ---------------------------------------------------------------------------
// Luau Writer
// ---------------------------------------------------------------------------

describe('LuauWriter', () => {
  it('emits enum as local table', () => {
    const src = emitLuau(makeModel());
    expect(src).toContain('local Role = {');
    expect(src).toContain("Admin = 'Admin',");
  });

  it('emits interface as local table with field assignments', () => {
    const src = emitLuau(makeModel());
    expect(src).toContain('local User = {}');
    expect(src).toContain('User.id = nil');
    expect(src).toContain('User.age = nil');
    expect(src).toContain('User.tags = nil');
  });

  it('emits default values', () => {
    const model: SchemaModel = {
      declarations: [{
        kind: 'interface',
        name: 'Cfg',
        fields: [
          { name: 'name', type: 'string', defaultValue: 'hello' },
          { name: 'count', type: 'number', defaultValue: 42 },
        ],
      }],
    };
    const src = emitLuau(model);
    expect(src).toContain('Cfg.name = "hello"');
    expect(src).toContain('Cfg.count = 42');
  });

  it('emits namespace comment', () => {
    const src = emitLuau(makeModel());
    expect(src).toContain('// @module TestNs');
  });

  it('Writer class produces .luau file', () => {
    const writer = new LuauWriter();
    const result = writer.emit(makeModel(), META);
    expect(result.files[0]?.filename).toBe('test-project.luau');
    expect(result.files[0]?.language).toBe('luau');
  });
});

// ---------------------------------------------------------------------------
// JSON Writer
// ---------------------------------------------------------------------------

describe('JsonWriter', () => {
  it('serializes with default indent of 2', () => {
    const json = serializeJson({ a: 1 });
    expect(json).toBe('{\n  "a": 1\n}\n');
  });

  it('serializes with custom indent', () => {
    const json = serializeJson({ a: 1 }, 4);
    expect(json).toBe('{\n    "a": 1\n}\n');
  });

  it('Writer class produces .json file', () => {
    const writer = new JsonWriter();
    const result = writer.emit({ hello: 'world' }, META);
    expect(result.errors).toHaveLength(0);
    expect(result.files[0]?.filename).toBe('test-project.json');
    expect(result.files[0]?.language).toBe('json');
  });

  it('Writer class reports error for circular references', () => {
    const circular: Record<string, unknown> = {};
    circular['self'] = circular;
    const writer = new JsonWriter();
    const result = writer.emit(circular, META);
    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.files).toHaveLength(0);
  });

  it('Writer class accepts custom indent option', () => {
    const writer = new JsonWriter({ indent: 4 });
    const result = writer.emit({ x: 1 }, META);
    expect(result.files[0]?.content).toContain('    "x"');
  });
});

// ---------------------------------------------------------------------------
// YAML Writer
// ---------------------------------------------------------------------------

describe('YamlWriter', () => {
  it('serializes scalars', () => {
    const yaml = serializeYaml({ name: 'Alice', age: 30, active: true });
    expect(yaml).toContain('name: Alice');
    expect(yaml).toContain('age: 30');
    expect(yaml).toContain('active: true');
  });

  it('quotes reserved YAML words', () => {
    const yaml = serializeYaml({ val: 'true' });
    expect(yaml).toContain('"true"');
  });

  it('quotes strings that look numeric', () => {
    const yaml = serializeYaml({ code: '007' });
    expect(yaml).toContain('"007"');
  });

  it('serializes nested objects', () => {
    const yaml = serializeYaml({ outer: { inner: 'val' } });
    expect(yaml).toContain('outer:');
    expect(yaml).toContain('inner: val');
  });

  it('serializes arrays', () => {
    const yaml = serializeYaml({ items: ['a', 'b'] });
    expect(yaml).toContain('items:');
    expect(yaml).toContain('- a');
    expect(yaml).toContain('- b');
  });

  it('serializes empty arrays as []', () => {
    const yaml = serializeYaml({ items: [] });
    expect(yaml).toContain('items: []');
  });

  it('serializes empty objects as {}', () => {
    const yaml = serializeYaml({ cfg: {} });
    expect(yaml).toContain('cfg: {}');
  });

  it('serializes null and undefined as null', () => {
    const yaml = serializeYaml({ a: null, b: undefined });
    expect(yaml).toContain('a: null');
    expect(yaml).toContain('b: null');
  });

  it('handles empty string values', () => {
    const yaml = serializeYaml({ empty: '' });
    expect(yaml).toContain('empty: ""');
  });

  it('handles multiline strings with block scalar', () => {
    const yaml = serializeYaml({ text: 'line1\nline2' });
    expect(yaml).toContain('text: |');
  });

  it('quotes keys with special characters', () => {
    const yaml = serializeYaml({ 'key:name': 'val' });
    expect(yaml).toContain('"key:name": val');
  });

  it('Writer class produces .yaml file', () => {
    const writer = new YamlWriter();
    const result = writer.emit({ x: 1 }, META);
    expect(result.files[0]?.filename).toBe('test-project.yaml');
    expect(result.files[0]?.language).toBe('yaml');
  });
});

// ---------------------------------------------------------------------------
// TOML Writer
// ---------------------------------------------------------------------------

describe('TomlWriter', () => {
  it('serializes top-level scalars', () => {
    const toml = serializeToml({ name: 'Alice', count: 42, active: true });
    expect(toml).toContain('name = "Alice"');
    expect(toml).toContain('count = 42');
    expect(toml).toContain('active = true');
  });

  it('serializes nested objects as [table] sections', () => {
    const toml = serializeToml({ database: { host: 'localhost', port: 5432 } });
    expect(toml).toContain('[database]');
    expect(toml).toContain('host = "localhost"');
    expect(toml).toContain('port = 5432');
  });

  it('serializes arrays of objects as [[array-of-tables]]', () => {
    const toml = serializeToml({
      servers: [
        { name: 'alpha', ip: '10.0.0.1' },
        { name: 'beta', ip: '10.0.0.2' },
      ],
    });
    expect(toml).toContain('[[servers]]');
    expect(toml).toContain('name = "alpha"');
    expect(toml).toContain('name = "beta"');
  });

  it('serializes scalar arrays inline', () => {
    const toml = serializeToml({ tags: ['a', 'b', 'c'] });
    expect(toml).toContain('tags = ["a", "b", "c"]');
  });

  it('handles null and undefined as empty string', () => {
    const toml = serializeToml({ x: null });
    expect(toml).toContain('x = ""');
  });

  it('escapes special characters in strings', () => {
    const toml = serializeToml({ msg: 'say "hello"\nworld' });
    expect(toml).toContain('msg = "say \\"hello\\"\\nworld"');
  });

  it('Writer class produces .toml file', () => {
    const writer = new TomlWriter();
    const result = writer.emit({ x: 1 }, META);
    expect(result.files[0]?.filename).toBe('test-project.toml');
    expect(result.files[0]?.language).toBe('toml');
  });
});
