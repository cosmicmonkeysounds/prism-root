// ── LanguageContribution ────────────────────────────────────────────────────
export type {
  LanguageContribution,
  LanguageSurface,
  LanguageCodegen,
} from "./language-contribution.js";

// ── Surface types (shared by contributions + renderers) ────────────────────
export type { SurfaceMode, InlineTokenDef } from "./surface-types.js";
export { InlineTokenBuilder, inlineToken, WIKILINK_TOKEN } from "./surface-types.js";

// ── Language Registry (replaces the legacy LanguageRegistry + DocumentSurfaceRegistry pair) ──
export type { ResolveOptions } from "./language-registry.js";
export { LanguageRegistry } from "./language-registry.js";
