/**
 * LanguageContribution compatibility bridge.
 *
 * Phase 1 of ADR-002 introduces `LanguageContribution` as a new type,
 * but does not yet rewrite the existing `LanguageRegistry` +
 * `DocumentSurfaceRegistry` pair that Luau, the expression language,
 * Markdown, YAML, JSON, HTML, CSV, and SVG already register into.
 *
 * This module adapts an existing `LanguageDefinition` + matching
 * `DocumentContributionDef` into a synthetic `LanguageContribution` view
 * so that new code can depend on the unified shape today. Phase 4
 * collapses the two underlying registries into one and deletes this
 * bridge.
 *
 * The bridge is deliberately read-only: it does not mutate the input
 * registries and it does not cache. A fresh `LanguageContribution`
 * object is produced on every call, which keeps the semantics obvious
 * while the migration is in flight.
 */

import type {
  LanguageRegistry,
  LanguageDefinition,
  DocumentSurfaceRegistry,
  DocumentContributionDef,
  RootNode,
  ProcessorContext,
} from "@prism/core/syntax";
import type { LanguageContribution } from "./language-contribution.js";

// ── Inputs ──────────────────────────────────────────────────────────────────

/**
 * Option 1: hand the bridge the two registries and it resolves both
 * sides from a common key (path or id).
 */
export interface ContributionResolveOptions {
  /** Existing language parser registry. */
  languages: LanguageRegistry;
  /** Existing document surface registry. */
  surfaces: DocumentSurfaceRegistry;
  /** Explicit contribution id override (takes precedence). */
  documentType?: string;
  /** Explicit language id override (takes precedence). */
  languageId?: string;
  /** File path used to resolve extension → language + surface. */
  filename?: string;
}

// ── Parse context shim ──────────────────────────────────────────────────────

/**
 * `LanguageDefinition.parse` takes a `ProcessorContext`; the unified
 * shape drops it because nothing downstream of `parse` reads ctx today
 * beyond the diagnostic sink. We synthesise a minimal context on the
 * bridge so the inner parser still receives a valid value.
 */
function makeBridgeCtx(
  source: string,
  language: string,
  filename: string | undefined,
): ProcessorContext {
  return {
    source,
    filename,
    language,
    data: new Map<string, unknown>(),
    diagnostics: [],
    report() {
      // Bridge drops diagnostics — Phase 4 will hook these into the
      // unified `SyntaxProvider.diagnose` path. Until then, callers that
      // need diagnostics should use the existing `LanguageRegistry`
      // directly.
    },
  };
}

// ── Bridge ──────────────────────────────────────────────────────────────────

/**
 * Adapt a `LanguageDefinition` + `DocumentContributionDef` pair into a
 * `LanguageContribution` view.
 *
 * Either half may be missing:
 * - A language without a registered surface (e.g. Luau in Phase 1) gets
 *   a minimal `surface` with `defaultMode: "code"` and only `"code"` in
 *   `availableModes`. Phase 2 replaces this with Luau's real surface.
 * - A surface without a registered language (e.g. Markdown in Phase 1)
 *   omits `parse`/`serialize`; Phase 4 adds a real Markdown parser.
 *
 * Throws if both inputs are null — there is nothing to bridge.
 */
export function contributionFromLegacy(
  language: LanguageDefinition | null,
  surface: DocumentContributionDef | null,
): LanguageContribution {
  if (!language && !surface) {
    throw new Error(
      "contributionFromLegacy: at least one of language or surface must be provided",
    );
  }

  // ── Identity ────────────────────────────────────────────────────────────
  // Prefer the surface id (namespaced "prism:markdown") over the raw
  // language id ("markdown") when both exist, matching how plugins
  // already address document types.
  const id = surface?.id ?? language?.id ?? "unknown";
  const extensions =
    surface?.extensions ?? language?.extensions ?? [];
  const displayName = surface?.displayName ?? language?.id ?? id;
  const mimeType = surface?.mimeType ?? language?.mimeTypes?.[0];

  // ── Surface ─────────────────────────────────────────────────────────────
  // Fall back to a minimal code-only surface when the language has no
  // registered `DocumentContributionDef`. This keeps the unified shape
  // total — callers never have to null-check `contribution.surface`.
  const bridgedSurface: LanguageContribution["surface"] = surface
    ? {
        defaultMode: surface.defaultMode,
        availableModes: surface.availableModes,
        ...(surface.inlineTokens ? { inlineTokens: surface.inlineTokens } : {}),
      }
    : {
        defaultMode: "code",
        availableModes: ["code"],
      };

  // ── Syntax ──────────────────────────────────────────────────────────────
  const result: LanguageContribution = {
    id,
    extensions,
    displayName,
    ...(mimeType ? { mimeType } : {}),
    surface: bridgedSurface,
  };

  if (language?.parse) {
    result.parse = (text: string): RootNode =>
      language.parse(text, makeBridgeCtx(text, language.id, undefined));
  }
  if (language?.serialize) {
    result.serialize = language.serialize.bind(language);
  }

  return result;
}

/**
 * Resolve a `LanguageContribution` from an existing pair of registries,
 * matching on explicit ids first and filename extension second.
 *
 * Returns `null` if neither registry has an entry for the options —
 * callers can then fall through to a plain-text default or surface
 * whichever error they like.
 */
export function resolveContribution(
  opts: ContributionResolveOptions,
): LanguageContribution | null {
  const surfaceEntry = opts.surfaces.resolve(opts.filename, opts.documentType);
  let language: LanguageDefinition | null;
  if (opts.languageId != null) {
    language = opts.languages.getById(opts.languageId);
  } else if (opts.filename != null) {
    language = opts.languages.resolve({ filename: opts.filename });
  } else {
    language = null;
  }

  if (!language && !surfaceEntry) return null;
  return contributionFromLegacy(language, surfaceEntry?.contribution ?? null);
}
