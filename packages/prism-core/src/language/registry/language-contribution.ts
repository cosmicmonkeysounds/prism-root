/**
 * LanguageContribution — unified registration for a file format.
 *
 * Introduced by ADR-002 §A2 to collapse the disjoint pair of
 * `LanguageRegistry` (parsing) and `DocumentSurfaceRegistry` (rendering)
 * into one record per format. A `LanguageContribution` owns *everything*
 * about a language: parse/serialize, the syntax provider (diagnostics,
 * completions, hover), the editor surface (mode + inline tokens +
 * renderers), and any optional codegen emitters.
 *
 * Phase 1 of the ADR introduces this type additively alongside the
 * existing registries. Phase 4 collapses the old registries into a
 * single `LanguageRegistry.register(contribution)` and retires the
 * compatibility bridge in `./compat.ts`. Until then, existing callers
 * keep using `LanguageDefinition` + `DocumentContributionDef` unchanged.
 *
 * ## Generic type parameters
 *
 * `@prism/core/language` stays framework-free. The two slots that would
 * otherwise force a CodeMirror or React dependency are exposed as
 * generic parameters with `unknown` defaults, mirroring the pattern used
 * by `LensBundle<TComponent>` in `@prism/core/lens`:
 *
 * - `TRenderer`        — the surface renderer type (React component in
 *                        Studio, something else in a headless test).
 * - `TEditorExtension` — the editor extension type (CodeMirror's
 *                        `Extension` in Studio, anything opaque
 *                        elsewhere).
 */

import type { Emitter } from "@prism/core/syntax";
import type {
  RootNode,
  SyntaxProvider,
  SurfaceMode,
  InlineTokenDef,
} from "@prism/core/syntax";

// ── Surface ─────────────────────────────────────────────────────────────────

/**
 * The editor surface contributed by a language.
 *
 * `renderers` is a partial record because not every language supports
 * every mode (Markdown has no `spreadsheet`, CSV has no `preview`). The
 * renderer type is opaque at the core level — Studio specializes it to
 * a React component; a headless test can specialize it to `void`.
 */
export interface LanguageSurface<TRenderer = unknown> {
  /** Default editing mode when opening a file of this language. */
  defaultMode: SurfaceMode;
  /** All modes the user can switch between. Must include `defaultMode`. */
  availableModes: SurfaceMode[];
  /** Inline tokens rendered identically across surface modes. */
  inlineTokens?: InlineTokenDef[];
  /**
   * Optional renderers keyed by mode. Phase 1 leaves this optional so
   * the compat bridge can produce a `LanguageContribution` from an
   * existing `DocumentContributionDef` (which has no renderers yet).
   */
  renderers?: Partial<Record<SurfaceMode, TRenderer>>;
}

// ── Codegen ─────────────────────────────────────────────────────────────────

/**
 * Optional codegen slot. Phase 4 wires this into `CodegenPipeline` so
 * `LanguageDefinition.serialize` stops being a dead hook.
 */
export interface LanguageCodegen {
  /** One or more emitters keyed by input kind (symbols/schema/ast/facet). */
  emitters: Emitter[];
}

// ── LanguageContribution ────────────────────────────────────────────────────

/**
 * The unified record a language plugin registers with the core.
 *
 * Everything except `id`, `extensions`, `displayName`, and `surface` is
 * optional so that binary formats (images, CAD files) can participate
 * by contributing only a surface — and pure-parse languages (headless
 * CLI use) can contribute without a surface at all.
 */
export interface LanguageContribution<
  TRenderer = unknown,
  TEditorExtension = unknown,
> {
  /** Namespaced contribution id: `prism:luau`, `prism:markdown`. */
  id: string;
  /** File extensions this contribution handles: `['.md', '.mdx']`. */
  extensions: string[];
  /** Human-readable format name shown in the toolbar. */
  displayName: string;
  /** MIME type for clipboard / drag-and-drop interop. */
  mimeType?: string;

  // ── Syntax (optional — binary formats may omit) ────────────────────────

  /** Parse source text into an AST. Optional for binary formats. */
  parse?(text: string): RootNode;
  /** Round-trip an AST back into source text. */
  serialize?(ast: RootNode): string;
  /** LSP-like provider for diagnostics, completion, hover. */
  syntaxProvider?(): SyntaxProvider;
  /**
   * Lazy CodeMirror extensions (language support, linting, etc.). The
   * thunk + `Promise` keeps heavy language imports out of the startup
   * path. `TEditorExtension` is a generic so core stays CodeMirror-free;
   * Studio specializes with `@codemirror/state`'s `Extension`.
   */
  codemirrorExtensions?(): Promise<TEditorExtension[]>;

  // ── Surface (editor UI) ────────────────────────────────────────────────

  /** The editor surface this language exposes. */
  surface: LanguageSurface<TRenderer>;

  // ── Codegen (optional) ─────────────────────────────────────────────────

  /** Optional codegen pipeline wiring. Phase 4 consumes this. */
  codegen?: LanguageCodegen;
}
