/**
 * LanguageRegistry — the unified language/document registry introduced
 * by ADR-002 §A2.
 *
 * Before Phase 4 there were two parallel registries — `LanguageRegistry`
 * (parsing) and `DocumentSurfaceRegistry` (rendering) — that callers had
 * to keep in sync by registering the same format twice. This module
 * replaces both with a single record type (`LanguageContribution`) and
 * one registry that resolves by contribution id, language id, or file
 * extension.
 *
 * The registry stays framework-free: it is generic over renderer and
 * editor-extension types. `@prism/core` exposes it with `unknown`
 * defaults; Studio specialises it to `React.ComponentType` and
 * CodeMirror's `Extension`.
 */

import type { LanguageContribution } from "./language-contribution.js";

// ── Resolve options ────────────────────────────────────────────────────────

/** Options for `LanguageRegistry.resolve`. */
export interface ResolveOptions {
  /** Explicit contribution id: `prism:markdown`, `prism:luau`. */
  id?: string;
  /** File path — resolved via extension. Compound extensions supported. */
  filename?: string;
}

// ── Registry ────────────────────────────────────────────────────────────────

/**
 * Registry that owns every `LanguageContribution` known to the current
 * kernel. Contributions are keyed by their namespaced id, and also
 * indexed by every extension they declare so a file path can resolve to
 * a contribution directly.
 *
 * Resolution order for `resolve({ filename })`:
 *   1. longest compound extension first (`.loom.ink` beats `.ink`)
 *   2. final extension fallback
 *
 * Resolution order for `resolve({ id, filename })`:
 *   1. explicit `id` wins if present
 *   2. otherwise fall through to filename lookup
 */
export class LanguageRegistry<
  TRenderer = unknown,
  TEditorExtension = unknown,
> {
  private readonly _byId = new Map<
    string,
    LanguageContribution<TRenderer, TEditorExtension>
  >();
  private readonly _byExt = new Map<string, string>();

  /** Register a contribution. Later registrations replace earlier ones. */
  register(
    contribution: LanguageContribution<TRenderer, TEditorExtension>,
  ): this {
    this._byId.set(contribution.id, contribution);
    for (const ext of contribution.extensions) {
      this._byExt.set(ext.toLowerCase(), contribution.id);
    }
    return this;
  }

  /** Unregister a contribution by id. Drops all its extension mappings. */
  unregister(id: string): void {
    const entry = this._byId.get(id);
    if (!entry) return;
    for (const ext of entry.extensions) {
      if (this._byExt.get(ext.toLowerCase()) === id) {
        this._byExt.delete(ext.toLowerCase());
      }
    }
    this._byId.delete(id);
  }

  /** Look up a contribution by its namespaced id. */
  get(
    id: string,
  ): LanguageContribution<TRenderer, TEditorExtension> | undefined {
    return this._byId.get(id);
  }

  /** Look up a contribution by a single extension (with or without dot). */
  getByExtension(
    ext: string,
  ): LanguageContribution<TRenderer, TEditorExtension> | undefined {
    const key = ext.startsWith(".") ? ext.toLowerCase() : `.${ext.toLowerCase()}`;
    const id = this._byExt.get(key);
    return id ? this._byId.get(id) : undefined;
  }

  /**
   * Resolve a contribution from a file path by trying compound extensions
   * first (longest match wins).
   */
  resolveByPath(
    filePath: string,
  ): LanguageContribution<TRenderer, TEditorExtension> | undefined {
    const lower = filePath.toLowerCase();
    const parts = lower.split(".");
    for (let i = 1; i < parts.length; i++) {
      const compound = "." + parts.slice(i).join(".");
      const id = this._byExt.get(compound);
      if (id) return this._byId.get(id);
    }
    return undefined;
  }

  /** Unified resolve — id override first, filename extension second. */
  resolve(
    options: ResolveOptions,
  ): LanguageContribution<TRenderer, TEditorExtension> | undefined {
    if (options.id) return this.get(options.id);
    if (options.filename) return this.resolveByPath(options.filename);
    return undefined;
  }

  /** All registered contributions. */
  all(): LanguageContribution<TRenderer, TEditorExtension>[] {
    return [...this._byId.values()];
  }

  /** All registered contribution ids. */
  ids(): string[] {
    return [...this._byId.keys()];
  }
}
