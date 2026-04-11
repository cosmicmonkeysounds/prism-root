// ── LanguageContribution ────────────────────────────────────────────────────
export type {
  LanguageContribution,
  LanguageSurface,
  LanguageCodegen,
} from "./language-contribution.js";

// ── Compat bridge (retired in Phase 4 per ADR-002) ──────────────────────────
export type { ContributionResolveOptions } from "./compat.js";
export { contributionFromLegacy, resolveContribution } from "./compat.js";
