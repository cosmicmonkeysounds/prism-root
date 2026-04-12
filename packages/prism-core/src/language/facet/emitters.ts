/**
 * Code generation writers -- emit SchemaModel (and plain data) to multiple target languages.
 *
 * Ported from Helm's codegen/writers/. All writers consolidated into a single module.
 * Each writer implements the Emitter<T> interface from the codegen pipeline.
 */

import type { Emitter, CodegenMeta, CodegenResult } from '@prism/core/codegen';
import { SourceBuilder } from '@prism/core/codegen';

// -- Schema types (self-contained, matching Helm's writers/schema.ts) ---------

/** Field within a schema declaration. */
export interface SchemaField {
  name: string;
  /** TypeScript-style type string: 'string', 'number', 'boolean', 'Date', 'MyType', etc. */
  type: string;
  optional?: boolean;
  array?: boolean;
  description?: string;
  defaultValue?: unknown;
}

export interface SchemaInterface {
  kind: 'interface' | 'class' | 'record';
  name: string;
  fields: SchemaField[];
  description?: string;
  /** Names of types/interfaces this extends or implements. */
  extends?: string[];
}

export interface SchemaEnum {
  kind: 'enum';
  name: string;
  /** String values for enum members. */
  values: string[];
  description?: string;
}

export type SchemaDeclaration = SchemaInterface | SchemaEnum;

export interface SchemaModel {
  /** Namespace/module name (used as TS module comment, C# namespace, JS @module tag). */
  namespace?: string;
  declarations: SchemaDeclaration[];
}

// =============================================================================
// TypeScript Writer
// =============================================================================

function emitTsDeclaration(b: SourceBuilder, decl: SchemaDeclaration): void {
  if (decl.kind === 'enum') {
    if (decl.description) b.comment(decl.description);
    b.block(`export enum ${decl.name}`, bld => {
      for (const v of decl.values) bld.line(`${v} = '${v}',`);
    });
    return;
  }

  if (decl.description) b.comment(decl.description);
  const fields = decl.fields ?? [];

  if (decl.kind === 'interface') {
    const ext = decl.extends?.length ? ` extends ${decl.extends.join(', ')}` : '';
    b.block(`export interface ${decl.name}${ext}`, bld => {
      for (const f of fields) {
        if (f.description) bld.comment(f.description);
        const t = f.array ? `${f.type}[]` : f.type;
        bld.line(`${f.name}${f.optional ? '?' : ''}: ${t};`);
      }
    });
    return;
  }

  // class or record
  const ext = decl.extends?.length ? ` extends ${decl.extends[0]}` : '';
  b.block(`export class ${decl.name}${ext}`, bld => {
    for (const f of fields) {
      if (f.description) bld.comment(f.description);
      const t = f.array ? `${f.type}[]` : f.type;
      const def = f.defaultValue !== undefined ? ` = ${JSON.stringify(f.defaultValue)}` : '';
      bld.line(`${f.name}${f.optional ? '?' : ''}: ${t}${def};`);
    }
  });
}

/** Emit a SchemaModel as TypeScript source. */
export function emitTypeScript(model: SchemaModel): string {
  const b = new SourceBuilder();

  if (model.namespace) {
    b.comment(`@module ${model.namespace}`);
    b.blank();
  }

  for (let i = 0; i < model.declarations.length; i++) {
    if (i > 0) b.blank();
    const decl = model.declarations[i];
    if (decl) emitTsDeclaration(b, decl);
  }

  return b.build() + '\n';
}

/** Emitter that generates a TypeScript file from a SchemaModel. */
export class TypeScriptWriter implements Emitter<SchemaModel> {
  readonly id = 'typescript';
  readonly inputKind = 'schema' as const;

  constructor(private options: { filename?: string } = {}) {}

  emit(input: SchemaModel, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.ts`;
    return {
      files: [{ filename, content: emitTypeScript(input), language: 'typescript' }],
      errors: [],
    };
  }
}

// =============================================================================
// JavaScript Writer
// =============================================================================

function emitJsDeclaration(b: SourceBuilder, decl: SchemaDeclaration): void {
  if (decl.kind === 'enum') {
    if (decl.description) b.comment(decl.description);
    b.constBlock(decl.name, bld => {
      for (const v of decl.values) bld.line(`${v}: '${v}',`);
    });
    return;
  }

  // interface / class / record: emit as JSDoc @typedef
  b.line('/**');
  if (decl.description) b.line(` * ${decl.description}`);
  b.line(` * @typedef {Object} ${decl.name}`);
  for (const f of decl.fields ?? []) {
    const t = f.array ? `${f.type}[]` : f.type;
    const nameTag = f.optional ? `[${f.name}]` : f.name;
    const desc = f.description ? ` - ${f.description}` : '';
    b.line(` * @property {${t}} ${nameTag}${desc}`);
  }
  b.line(' */');
}

/** Emit a SchemaModel as JavaScript source with JSDoc annotations. */
export function emitJavaScript(model: SchemaModel): string {
  const b = new SourceBuilder();

  if (model.namespace) {
    b.comment(`@module ${model.namespace}`);
    b.blank();
  }

  for (let i = 0; i < model.declarations.length; i++) {
    if (i > 0) b.blank();
    const decl = model.declarations[i];
    if (decl) emitJsDeclaration(b, decl);
  }

  return b.build() + '\n';
}

/** Emitter that generates a JavaScript file from a SchemaModel. */
export class JavaScriptWriter implements Emitter<SchemaModel> {
  readonly id = 'javascript';
  readonly inputKind = 'schema' as const;

  constructor(private options: { filename?: string } = {}) {}

  emit(input: SchemaModel, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.js`;
    return {
      files: [{ filename, content: emitJavaScript(input), language: 'javascript' }],
      errors: [],
    };
  }
}

// =============================================================================
// C# Writer
// =============================================================================

const CSHARP_TYPE_MAP: Record<string, string> = {
  string: 'string',
  number: 'double',
  int: 'int',
  integer: 'int',
  float: 'float',
  double: 'double',
  bool: 'bool',
  boolean: 'bool',
  object: 'object',
  unknown: 'object',
  Date: 'DateTime',
  date: 'DateTime',
};

function csharpType(type: string, array: boolean, optional: boolean): string {
  const base = CSHARP_TYPE_MAP[type] ?? type;
  const t = array ? `List<${base}>` : base;
  return optional ? `${t}?` : t;
}

function pascalCase(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function emitCSharpDeclaration(b: SourceBuilder, decl: SchemaDeclaration): void {
  if (decl.description) b.comment(`<summary>${decl.description}</summary>`);

  if (decl.kind === 'enum') {
    b.block(`public enum ${decl.name}`, bld => {
      for (const v of decl.values) bld.line(`${v},`);
    });
    return;
  }

  const fields = decl.fields ?? [];

  if (decl.kind === 'record') {
    const params = fields
      .map(f => `${csharpType(f.type, !!f.array, !!f.optional)} ${pascalCase(f.name)}`)
      .join(', ');
    b.line(`public record ${decl.name}(${params});`);
    return;
  }

  // class or interface
  const keyword = decl.kind === 'interface' ? 'interface' : 'class';
  const accessibility = decl.kind === 'interface' ? '' : 'public ';
  const ext = decl.extends?.length ? ` : ${decl.extends.join(', ')}` : '';
  b.block(`public ${keyword} ${decl.name}${ext}`, bld => {
    for (const f of fields) {
      if (f.description) bld.comment(f.description);
      const t = csharpType(f.type, !!f.array, !!f.optional);
      const propEnd = f.optional ? ' { get; set; }' : ' { get; set; } = default!;';
      bld.line(`${accessibility}${t} ${pascalCase(f.name)}${propEnd}`);
    }
  });
}

/** Emit a SchemaModel as C# source. */
export function emitCSharp(model: SchemaModel): string {
  const b = new SourceBuilder();
  const ns = model.namespace ?? 'Generated';

  b.block(`namespace ${ns}`, bld => {
    for (let i = 0; i < model.declarations.length; i++) {
      if (i > 0) bld.blank();
      const decl = model.declarations[i];
      if (decl) emitCSharpDeclaration(bld, decl);
    }
  });

  return b.build() + '\n';
}

/** Emitter that generates a C# file from a SchemaModel. */
export class CSharpWriter implements Emitter<SchemaModel> {
  readonly id = 'csharp';
  readonly inputKind = 'schema' as const;

  constructor(private options: { filename?: string } = {}) {}

  emit(input: SchemaModel, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.cs`;
    return {
      files: [{ filename, content: emitCSharp(input), language: 'csharp' }],
      errors: [],
    };
  }
}

// =============================================================================
// Luau Writer
// =============================================================================

function emitLuauDeclaration(b: SourceBuilder, decl: SchemaDeclaration): void {
  if (decl.description) b.comment(decl.description);

  if (decl.kind === 'enum') {
    b.line(`local ${decl.name} = {`);
    b.indent();
    for (const v of decl.values) b.line(`${v} = '${v}',`);
    b.dedent();
    b.line('}');
    return;
  }

  const fields = decl.fields ?? [];
  b.line(`local ${decl.name} = {}`);
  for (const f of fields) {
    if (f.description) b.comment(f.description);
    const def = f.defaultValue !== undefined ? JSON.stringify(f.defaultValue) : 'nil';
    b.line(`${decl.name}.${f.name} = ${def}`);
  }
}

/** Emit a SchemaModel as Luau source. */
export function emitLuau(model: SchemaModel): string {
  const b = new SourceBuilder();

  if (model.namespace) {
    b.comment(`@module ${model.namespace}`);
    b.blank();
  }

  for (let i = 0; i < model.declarations.length; i++) {
    if (i > 0) b.blank();
    const decl = model.declarations[i];
    if (decl) emitLuauDeclaration(b, decl);
  }

  return b.build() + '\n';
}

/** Emitter that generates a Luau file from a SchemaModel. */
export class LuauWriter implements Emitter<SchemaModel> {
  readonly id = 'luau';
  readonly inputKind = 'schema' as const;

  constructor(private options: { filename?: string } = {}) {}

  emit(input: SchemaModel, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.luau`;
    return {
      files: [{ filename, content: emitLuau(input), language: 'luau' }],
      errors: [],
    };
  }
}

// =============================================================================
// JSON Writer
// =============================================================================

export interface JsonWriterOptions {
  filename?: string;
  indent?: number;
}

/** Serialize a JS value to a JSON string. */
export function serializeJson(value: unknown, indent = 2): string {
  return JSON.stringify(value, null, indent) + '\n';
}

/** Emitter that serializes a JS value to a .json file. */
export class JsonWriter implements Emitter<unknown> {
  readonly id = 'json';
  readonly inputKind = 'data' as const;

  constructor(private options: JsonWriterOptions = {}) {}

  emit(input: unknown, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.json`;
    try {
      return {
        files: [{ filename, content: serializeJson(input, this.options.indent ?? 2), language: 'json' }],
        errors: [],
      };
    } catch (err) {
      return { files: [], errors: [`JSON serialization failed: ${err instanceof Error ? err.message : String(err)}`] };
    }
  }
}

// =============================================================================
// YAML Writer
// =============================================================================

export interface YamlWriterOptions {
  filename?: string;
}

function yamlScalar(s: string, depth: number): string {
  if (s === '') return '""';

  // Multiline: use literal block scalar
  if (s.includes('\n')) {
    const pad = '  '.repeat(depth + 1);
    return `|\n${s.split('\n').map(l => pad + l).join('\n')}`;
  }

  // Conservative quoting: reserved words, number-like strings, special chars
  const needsQuote =
    /^(true|false|null|yes|no|on|off|~)$/i.test(s) ||
    /^[-+]?\d/.test(s) ||
    /[:#{}[\],&*!|>'"@`]/.test(s) ||
    /^[ \t]/.test(s) ||
    /[ \t]$/.test(s);

  if (needsQuote) {
    return '"' + s.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"';
  }
  return s;
}

function yamlSerializeNode(value: unknown, depth: number): string {
  if (value === null || value === undefined) return 'null';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return isFinite(value) ? String(value) : 'null';
  if (typeof value === 'string') return yamlScalar(value, depth);
  if (Array.isArray(value)) return yamlSerializeArray(value, depth);
  if (typeof value === 'object') return yamlSerializeObject(value as Record<string, unknown>, depth);
  return String(value);
}

function yamlSerializeArray(arr: unknown[], depth: number): string {
  if (arr.length === 0) return '[]';
  const pad = '  '.repeat(depth);

  return arr.map(item => {
    if (typeof item === 'object' && item !== null) {
      const val = yamlSerializeNode(item, depth + 1);
      const lines = val.split('\n').filter(l => l.length > 0);
      if (lines.length === 0) return `${pad}-`;
      const rest = lines.slice(1).map(l => `${pad}  ${l.trimStart()}`);
      return [`${pad}- ${(lines[0] ?? '').trimStart()}`, ...rest].join('\n');
    }
    return `${pad}- ${yamlSerializeNode(item, depth)}`;
  }).join('\n');
}

function yamlNeedsKeyQuoting(k: string): boolean {
  return /[:{}[\],#&*!|>'"@` ]/.test(k) || k.trim() !== k || k === '';
}

function yamlSerializeObject(obj: Record<string, unknown>, depth: number): string {
  const entries = Object.entries(obj);
  if (entries.length === 0) return '{}';
  const pad = '  '.repeat(depth);

  return entries.map(([k, v]) => {
    const key = yamlNeedsKeyQuoting(k) ? `"${k}"` : k;

    if (typeof v === 'object' && v !== null) {
      if (Array.isArray(v) && v.length === 0) return `${pad}${key}: []`;
      if (!Array.isArray(v) && Object.keys(v as Record<string, unknown>).length === 0) return `${pad}${key}: {}`;
      return `${pad}${key}:\n${yamlSerializeNode(v, depth + 1)}`;
    }
    return `${pad}${key}: ${yamlSerializeNode(v, depth)}`;
  }).join('\n');
}

/** Serialize a JS value to a YAML string (no external deps). */
export function serializeYaml(value: unknown): string {
  return yamlSerializeNode(value, 0).trimEnd() + '\n';
}

/** Emitter that serializes a JS value to a .yaml file. */
export class YamlWriter implements Emitter<unknown> {
  readonly id = 'yaml';
  readonly inputKind = 'data' as const;

  constructor(private options: YamlWriterOptions = {}) {}

  emit(input: unknown, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.yaml`;
    return {
      files: [{ filename, content: serializeYaml(input), language: 'yaml' }],
      errors: [],
    };
  }
}

// =============================================================================
// TOML Writer
// =============================================================================

export interface TomlWriterOptions {
  filename?: string;
}

function tomlValue(v: unknown): string {
  if (v === null || v === undefined) return '""';
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') return String(v);
  if (typeof v === 'string') {
    return '"' + v
      .replace(/\\/g, '\\\\')
      .replace(/"/g, '\\"')
      .replace(/\n/g, '\\n')
      .replace(/\r/g, '\\r')
      .replace(/\t/g, '\\t') + '"';
  }
  if (Array.isArray(v)) {
    // Only inline arrays of scalars; arrays of objects handled as [[array-of-tables]]
    return '[' + v.map(tomlValue).join(', ') + ']';
  }
  if (typeof v === 'object') {
    // Inline table fallback for nested objects inside a [table]
    const pairs = Object.entries(v as Record<string, unknown>)
      .map(([k, val]) => `${k} = ${tomlValue(val)}`);
    return '{ ' + pairs.join(', ') + ' }';
  }
  return String(v);
}

/**
 * Serialize a plain JS object to a TOML string.
 *
 * Handles:
 * - Top-level scalars and scalar arrays
 * - One level of nested objects as `[table]` sections
 * - Arrays of objects as `[[array-of-tables]]` sections
 * - Deeper nesting inside a [table] falls back to inline tables `{ k = v }`
 */
export function serializeToml(input: Record<string, unknown>): string {
  const scalars: string[] = [];
  const tables: Array<[string, Record<string, unknown>]> = [];
  const arrayTables: Array<[string, Record<string, unknown>[]]> = [];

  for (const [k, v] of Object.entries(input)) {
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      tables.push([k, v as Record<string, unknown>]);
    } else if (
      Array.isArray(v) &&
      v.length > 0 &&
      v.every(i => typeof i === 'object' && i !== null && !Array.isArray(i))
    ) {
      arrayTables.push([k, v as Record<string, unknown>[]]);
    } else {
      scalars.push(`${k} = ${tomlValue(v)}`);
    }
  }

  const parts: string[] = [];

  if (scalars.length) parts.push(scalars.join('\n'));

  for (const [key, obj] of tables) {
    const lines: string[] = [`[${key}]`];
    for (const [k, v] of Object.entries(obj)) {
      lines.push(`${k} = ${tomlValue(v)}`);
    }
    parts.push(lines.join('\n'));
  }

  for (const [key, arr] of arrayTables) {
    for (const item of arr) {
      const lines: string[] = [`[[${key}]]`];
      for (const [k, v] of Object.entries(item)) {
        lines.push(`${k} = ${tomlValue(v)}`);
      }
      parts.push(lines.join('\n'));
    }
  }

  return parts.join('\n\n') + '\n';
}

/** Emitter that serializes a plain JS object to a .toml file. */
export class TomlWriter implements Emitter<Record<string, unknown>> {
  readonly id = 'toml';
  readonly inputKind = 'data' as const;

  constructor(private options: TomlWriterOptions = {}) {}

  emit(input: Record<string, unknown>, meta: CodegenMeta): CodegenResult {
    const filename = this.options.filename ?? `${meta.projectName}.toml`;
    return {
      files: [{ filename, content: serializeToml(input), language: 'toml' }],
      errors: [],
    };
  }
}
