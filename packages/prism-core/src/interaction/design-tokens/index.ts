/**
 * @prism/core/design-tokens — CSS variable registry for theming.
 *
 * Tokens live in three buckets (colors, spacing, fonts) and are
 * materialised as CSS custom properties on `:root`. Pure data + a small
 * subscribable registry — no React, no DOM.
 */

export type { DesignTokenBundle } from "./design-tokens.js";
export {
  DEFAULT_TOKENS,
  DesignTokenRegistry,
  createDesignTokenRegistry,
  tokensToCss,
  lookupToken,
  mergeTokens,
} from "./design-tokens.js";
