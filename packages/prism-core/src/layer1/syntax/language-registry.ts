/**
 * Language Registry — file-extension-to-parser resolution and processing pipeline.
 *
 * Orthogonal to SyntaxProvider (LSP-like diagnostics/completions/hover).
 * SyntaxProvider handles expression-level intelligence; LanguageRegistry
 * handles file-level parsing, plugin pipelines, and compilation.
 *
 * Ported from Helm core/processing/syntax processor system.
 */

import type { RootNode } from './ast-types.js';

// ── Diagnostic ──────────────────────────────────────────────────────────────

export interface ProcessorDiagnostic {
  severity: 'error' | 'warning' | 'info';
  message: string;
  position?: { start: { offset: number; line: number; column: number };
               end:   { offset: number; line: number; column: number } };
  source?: string;   // plugin id
}

// ── Processor context ───────────────────────────────────────────────────────

export interface ProcessorContext {
  readonly source: string;
  readonly filename: string | undefined;
  readonly language: string;
  /** Shared plugin storage — plugins store/retrieve data here by key. */
  readonly data: Map<string, unknown>;
  readonly diagnostics: ProcessorDiagnostic[];
  report(d: ProcessorDiagnostic): void;
}

// ── Syntax plugin ───────────────────────────────────────────────────────────

export interface SyntaxPlugin {
  /** Unique plugin id. */
  id: string;
  /** Called before parsing — transform raw source string. */
  preprocess?(source: string, ctx: ProcessorContext): string;
  /** Called after base language parse — augment/annotate the tree. */
  parse?(tree: RootNode, ctx: ProcessorContext): RootNode;
  /** AST-to-AST transformation. */
  transform?(tree: RootNode, ctx: ProcessorContext): RootNode;
  /** AST-to-string compilation. Return null to defer to the next plugin. */
  compile?(tree: RootNode, ctx: ProcessorContext): string | null;
}

// ── Language definition ─────────────────────────────────────────────────────

export interface LanguageDefinition {
  id: string;
  extensions: string[];
  mimeTypes?: string[];
  parse(source: string, ctx: ProcessorContext): RootNode;
  serialize?(tree: RootNode): string;
}

// ── Pipeline result ─────────────────────────────────────────────────────────

export interface PipelineResult {
  tree: RootNode;
  output: string;
  diagnostics: ProcessorDiagnostic[];
}

// ── Parse options ───────────────────────────────────────────────────────────

export interface ParseOptions {
  language?: string;
  filename?: string;
}

// ── Language Registry ───────────────────────────────────────────────────────

export class LanguageRegistry {
  private _byId = new Map<string, LanguageDefinition>();
  private _byExt = new Map<string, LanguageDefinition>();

  register(lang: LanguageDefinition): this {
    this._byId.set(lang.id, lang);
    for (const ext of lang.extensions) this._byExt.set(ext.toLowerCase(), lang);
    return this;
  }

  getByExtension(ext: string): LanguageDefinition | null {
    return this._byExt.get(ext.toLowerCase()) ?? null;
  }

  getById(id: string): LanguageDefinition | null {
    return this._byId.get(id) ?? null;
  }

  /**
   * Resolve language from filename or explicit id.
   * Falls back to 'markdown' if neither is provided.
   */
  resolve(options: { language?: string; filename?: string }): LanguageDefinition | null {
    if (options.language) return this.getById(options.language);
    if (options.filename) {
      // Support compound extensions like .loom.ink — try longest first
      const parts = options.filename.split('.');
      for (let i = 1; i < parts.length; i++) {
        const compound = '.' + parts.slice(i).join('.');
        const lang = this.getByExtension(compound);
        if (lang) return lang;
      }
      const ext = options.filename.slice(options.filename.lastIndexOf('.'));
      return this.getByExtension(ext);
    }
    return this.getById('markdown');
  }

  listIds(): string[] {
    return [...this._byId.keys()];
  }
}

// ── Processor ───────────────────────────────────────────────────────────────

function makeCtx(
  source: string,
  filename: string | undefined,
  language: string,
): ProcessorContext {
  const diagnostics: ProcessorDiagnostic[] = [];
  const data = new Map<string, unknown>();
  return {
    source, filename, language, data, diagnostics,
    report(d) { diagnostics.push(d); },
  };
}

/**
 * Pipeline processor — chains SyntaxPlugins through preprocess, parse,
 * transform, and compile phases over a LanguageRegistry-resolved language.
 */
export class Processor {
  private _plugins: SyntaxPlugin[] = [];
  private _installed = new Set<string>();
  readonly languages: LanguageRegistry;

  constructor(languages?: LanguageRegistry) {
    this.languages = languages ?? new LanguageRegistry();
  }

  use(plugin: SyntaxPlugin): this {
    if (this._installed.has(plugin.id)) return this;
    this._installed.add(plugin.id);
    this._plugins.push(plugin);
    return this;
  }

  process(source: string, options: ParseOptions = {}): PipelineResult {
    const lang = this.languages.resolve(options) ?? this.languages.getById('markdown');
    if (!lang) throw new Error(`No language registered for options: ${JSON.stringify(options)}`);
    const ctx = makeCtx(source, options.filename, lang.id);

    // 1. preprocess
    let src = source;
    for (const p of this._plugins) if (p.preprocess) src = p.preprocess(src, ctx);

    // 2. base language parse
    let tree: RootNode = lang.parse(src, ctx);

    // 3. plugin parse hooks
    for (const p of this._plugins) if (p.parse) tree = p.parse(tree, ctx);

    // 4. transform
    for (const p of this._plugins) if (p.transform) tree = p.transform(tree, ctx);

    // 5. compile (first non-null wins)
    let output = '';
    for (const p of this._plugins) {
      if (p.compile) {
        const result = p.compile(tree, ctx);
        if (result !== null) { output = result; break; }
      }
    }

    return { tree, output, diagnostics: ctx.diagnostics };
  }
}
