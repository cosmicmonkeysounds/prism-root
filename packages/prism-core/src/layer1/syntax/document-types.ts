/**
 * Document Surface types — type definitions and registry for multi-mode
 * document editing surfaces.
 *
 * Every text file in a Prism workspace opens through the DocumentSurface
 * component (Layer 2). This module defines the type contracts and a
 * registry that manages registered document type contributions.
 *
 * Surface modes:
 *   code        — CodeMirror raw syntax editing
 *   preview     — CodeMirror live-preview (rendered on inactive lines)
 *   form        — Schema-driven field inputs
 *   spreadsheet — Grid editing for tabular data
 *   report      — Full HTML layout engine
 */

// ── Surface Mode ─────────────────────────────────────────────────────────────

/**
 * Editing modes available in the document surface.
 * Prism uses CodeMirror 6 exclusively — no richtext/TipTap mode.
 */
export type SurfaceMode = "code" | "preview" | "form" | "spreadsheet" | "report";

// ── Inline Token Definition ──────────────────────────────────────────────────

/**
 * Defines a pattern-matched inline token that renders identically across
 * all surface modes (code marks, preview chips, form chips).
 *
 * Plugins register these via DocumentContributionDef.inlineTokens or
 * the documentSurfaceBuilder.
 *
 * @example
 * ```ts
 * const wikiToken: InlineTokenDef = {
 *   id: 'wikilink',
 *   pattern: /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g,
 *   extract: (m) => ({ display: m[2] ?? m[1], data: { id: m[1] } }),
 *   cssClass: 'pt-token-wikilink',
 *   chipColor: 'teal',
 *   replaceInPreview: true,
 * };
 * ```
 */
export interface InlineTokenDef {
  /** Unique token ID: 'wikilink', 'operand', 'resolve-ref'. */
  id: string;
  /** Global regex with capture groups. MUST have the `g` flag. */
  pattern: RegExp;
  /**
   * Extract display text and structured data from a regex match.
   * Called for every match to determine what to render.
   */
  extract: (match: RegExpExecArray) => {
    display: string;
    data?: Record<string, unknown>;
  };
  /** CSS class applied in code-mode surfaces (CodeMirror mark decoration). */
  cssClass?: string;
  /**
   * Semantic color hint for chip renderers.
   * Palette: 'teal', 'amber', 'violet', 'emerald', 'rose', 'blue', 'zinc'.
   */
  chipColor?: string;
  /** In preview modes, replace raw syntax with a chip widget. */
  replaceInPreview?: boolean;
}

// ── Document Contribution Definition ─────────────────────────────────────────

/**
 * Declares a document type that the DocumentSurface can edit.
 *
 * Plugins contribute these to register new file formats. The surface
 * resolves contributions by file extension and renders the appropriate
 * editing mode.
 */
export interface DocumentContributionDef {
  /** Namespaced contribution ID: 'prism:markdown', 'loom:lang'. */
  id: string;
  /** File extensions this type handles: ['.md', '.mdx']. */
  extensions: string[];
  /** Human-readable format name shown in the toolbar. */
  displayName: string;
  /** Default editing mode when opening a file. */
  defaultMode: SurfaceMode;
  /** All modes the user can switch between. */
  availableModes: SurfaceMode[];
  /** MIME type for clipboard / drag-and-drop interop. */
  mimeType?: string;
  /** Inline tokens specific to this document type. */
  inlineTokens?: InlineTokenDef[];
}

// ── Document Surface Registry ────────────────────────────────────────────────

/** Configuration for a registered document surface. */
export interface DocumentSurfaceEntry {
  contribution: DocumentContributionDef;
  /** Factory for additional CodeMirror extensions (language, linting, etc). */
  codemirrorExtensionIds?: string[];
}

/**
 * Registry that manages document type contributions.
 *
 * Plugins register contributions via `register()`. The DocumentSurface
 * component resolves the appropriate contribution by file path or ID.
 */
export class DocumentSurfaceRegistry {
  private readonly _entries = new Map<string, DocumentSurfaceEntry>();
  private readonly _extIndex = new Map<string, string>();

  /** Register a document contribution. */
  register(entry: DocumentSurfaceEntry): void {
    this._entries.set(entry.contribution.id, entry);
    for (const ext of entry.contribution.extensions) {
      this._extIndex.set(ext, entry.contribution.id);
    }
  }

  /** Unregister a document contribution by ID. */
  unregister(id: string): void {
    const entry = this._entries.get(id);
    if (!entry) return;
    for (const ext of entry.contribution.extensions) {
      if (this._extIndex.get(ext) === id) {
        this._extIndex.delete(ext);
      }
    }
    this._entries.delete(id);
  }

  /** Resolve by contribution ID. */
  get(id: string): DocumentSurfaceEntry | undefined {
    return this._entries.get(id);
  }

  /**
   * Resolve by file path — matches longest extension first
   * (e.g. `.loom.yaml` before `.yaml`).
   */
  resolveByPath(filePath: string): DocumentSurfaceEntry | undefined {
    const sorted = [...this._extIndex.entries()].sort(
      (a, b) => b[0].length - a[0].length,
    );
    for (const [ext, id] of sorted) {
      if (filePath.endsWith(ext)) return this._entries.get(id);
    }
    return undefined;
  }

  /**
   * Resolve by file path or contribution ID.
   * Contribution ID takes precedence when both are provided.
   */
  resolve(
    filePath?: string,
    documentType?: string,
  ): DocumentSurfaceEntry | undefined {
    if (documentType) return this.get(documentType);
    if (filePath) return this.resolveByPath(filePath);
    return undefined;
  }

  /** Get all registered entries. */
  all(): DocumentSurfaceEntry[] {
    return [...this._entries.values()];
  }

  /** Get all registered contribution IDs. */
  ids(): string[] {
    return [...this._entries.keys()];
  }
}

// ── Inline Token Builder ─────────────────────────────────────────────────────

/**
 * Fluent builder for concise InlineTokenDef creation.
 *
 * @example
 * ```ts
 * const token = new InlineTokenBuilder('wikilink', /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g)
 *   .extract(m => ({ display: m[2] ?? m[1], data: { raw: m[1] } }))
 *   .css('pt-token-wikilink')
 *   .chip('teal')
 *   .replaceInPreview()
 *   .build();
 * ```
 */
export class InlineTokenBuilder {
  private readonly _def: Partial<InlineTokenDef> & { id: string; pattern: RegExp };

  constructor(id: string, pattern: RegExp) {
    this._def = { id, pattern };
  }

  /** Define how to extract display text and structured data from a regex match. */
  extract(fn: InlineTokenDef["extract"]): this {
    this._def.extract = fn;
    return this;
  }

  /** CSS class applied in code-mode surfaces (CodeMirror mark decoration). */
  css(className: string): this {
    this._def.cssClass = className;
    return this;
  }

  /** Semantic color hint for chip renderers. */
  chip(color: string): this {
    this._def.chipColor = color;
    return this;
  }

  /** In preview modes, replace raw syntax with a chip widget. */
  replaceInPreview(replace = true): this {
    this._def.replaceInPreview = replace;
    return this;
  }

  build(): InlineTokenDef {
    if (!this._def.extract) {
      throw new Error(`InlineTokenBuilder(${this._def.id}): extract is required`);
    }
    return this._def as InlineTokenDef;
  }
}

/**
 * Factory for concise InlineTokenDef creation.
 *
 * @param id      Unique token ID: 'wikilink', 'operand', 'resolve-ref'.
 * @param pattern Global regex with capture groups. MUST have the `g` flag.
 */
export function inlineToken(id: string, pattern: RegExp): InlineTokenBuilder {
  return new InlineTokenBuilder(id, pattern);
}

// ── Document Surface Builder ─────────────────────────────────────────────────

/**
 * Fluent builder for document surface registration.
 *
 * @example
 * ```ts
 * documentSurfaceBuilder('loom:lang')
 *   .extensions(['.loom'])
 *   .displayName('Loom Lang')
 *   .defaultMode('preview')
 *   .modes('code', 'preview', 'report')
 *   .inlineTokens(wikiLink, operand)
 *   .mimeType('text/x-loom')
 *   .build();
 * ```
 */
export class DocumentSurfaceBuilder {
  private readonly _contribution: Partial<DocumentContributionDef> & { id: string };

  constructor(id: string) {
    this._contribution = { id };
  }

  /** File extensions this document type handles. */
  extensions(exts: string[]): this {
    this._contribution.extensions = exts;
    return this;
  }

  /** Human-readable format name shown in the toolbar. */
  displayName(name: string): this {
    this._contribution.displayName = name;
    return this;
  }

  /** Default editing mode when opening a file. */
  defaultMode(mode: SurfaceMode): this {
    this._contribution.defaultMode = mode;
    return this;
  }

  /** All modes the user can switch between. Must include defaultMode. */
  modes(...modes: SurfaceMode[]): this {
    this._contribution.availableModes = modes;
    return this;
  }

  /** Inline tokens specific to this document type. */
  inlineTokens(...tokens: InlineTokenDef[]): this {
    this._contribution.inlineTokens = tokens;
    return this;
  }

  /** MIME type for clipboard / drag-and-drop interop. */
  mimeType(mime: string): this {
    this._contribution.mimeType = mime;
    return this;
  }

  build(): DocumentContributionDef {
    const c = this._contribution;
    if (!c.extensions?.length) {
      throw new Error(`DocumentSurfaceBuilder(${c.id}): extensions is required`);
    }
    if (!c.displayName) {
      throw new Error(`DocumentSurfaceBuilder(${c.id}): displayName is required`);
    }
    if (!c.defaultMode) {
      throw new Error(`DocumentSurfaceBuilder(${c.id}): defaultMode is required`);
    }
    if (!c.availableModes?.length) {
      throw new Error(`DocumentSurfaceBuilder(${c.id}): modes is required`);
    }
    if (!c.availableModes.includes(c.defaultMode)) {
      throw new Error(
        `DocumentSurfaceBuilder(${c.id}): defaultMode '${c.defaultMode}' must be in modes`,
      );
    }
    return c as DocumentContributionDef;
  }

  /** Build and register into the given DocumentSurfaceRegistry. */
  register(registry: DocumentSurfaceRegistry): DocumentContributionDef {
    const contribution = this.build();
    registry.register({ contribution });
    return contribution;
  }
}

/**
 * Fluent builder for document surface registration.
 *
 * @param id Namespaced contribution ID: 'loom:lang', 'prism:markdown'.
 */
export function documentSurfaceBuilder(id: string): DocumentSurfaceBuilder {
  return new DocumentSurfaceBuilder(id);
}

// ── Built-in Contributions ───────────────────────────────────────────────────

export const MARKDOWN_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:markdown",
  extensions: [".md", ".mdx", ".markdown"],
  displayName: "Markdown",
  defaultMode: "preview",
  availableModes: ["code", "preview"],
  mimeType: "text/markdown",
};

export const YAML_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:yaml",
  extensions: [".yaml", ".yml"],
  displayName: "YAML",
  defaultMode: "code",
  availableModes: ["code", "form"],
  mimeType: "application/x-yaml",
};

export const JSON_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:json",
  extensions: [".json"],
  displayName: "JSON",
  defaultMode: "code",
  availableModes: ["code", "form"],
  mimeType: "application/json",
};

export const PLAINTEXT_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:plaintext",
  extensions: [".txt", ".text", ".log"],
  displayName: "Plain Text",
  defaultMode: "code",
  availableModes: ["code"],
  mimeType: "text/plain",
};

export const HTML_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:html",
  extensions: [".html", ".htm"],
  displayName: "HTML",
  defaultMode: "preview",
  availableModes: ["code", "preview"],
  mimeType: "text/html",
};

export const CSV_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:csv",
  extensions: [".csv", ".tsv"],
  displayName: "CSV",
  defaultMode: "spreadsheet",
  availableModes: ["code", "spreadsheet"],
  mimeType: "text/csv",
};

export const SVG_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:svg",
  extensions: [".svg"],
  displayName: "SVG",
  defaultMode: "preview",
  availableModes: ["code", "preview"],
  mimeType: "image/svg+xml",
};

/** Wiki-link inline token: [[id|display]] or [[id]]. */
export const WIKILINK_TOKEN: InlineTokenDef = {
  id: "wikilink",
  pattern: /\[\[([^\]|]+?)(?:\|([^\]]+?))?\]\]/g,
  extract: (m) => ({
    display: m[2] ?? m[1]?.split(":").pop() ?? m[1] ?? "",
    data: { raw: m[1] ?? "", display: m[2] ?? "" },
  }),
  cssClass: "pt-token-wikilink",
  chipColor: "teal",
  replaceInPreview: true,
};

/**
 * Create a DocumentSurfaceRegistry pre-populated with all built-in
 * document contributions.
 */
export function createDocumentSurfaceRegistry(): DocumentSurfaceRegistry {
  const registry = new DocumentSurfaceRegistry();
  registry.register({ contribution: MARKDOWN_CONTRIBUTION });
  registry.register({ contribution: YAML_CONTRIBUTION });
  registry.register({ contribution: JSON_CONTRIBUTION });
  registry.register({ contribution: PLAINTEXT_CONTRIBUTION });
  registry.register({ contribution: HTML_CONTRIBUTION });
  registry.register({ contribution: CSV_CONTRIBUTION });
  registry.register({ contribution: SVG_CONTRIBUTION });
  return registry;
}
