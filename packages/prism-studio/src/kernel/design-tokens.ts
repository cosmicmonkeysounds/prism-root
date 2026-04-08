/**
 * Design tokens — CSS variable registry for theming.
 *
 * Tokens live in three buckets (colors, spacing, fonts) and are materialised
 * as CSS custom properties on `:root`. Tokens are stored as plain values; a
 * helper produces a `<style>` block that the app can inject once at startup
 * and that the page exporter can embed into exported HTML.
 */

export interface DesignTokenBundle {
  colors: Record<string, string>;
  spacing: Record<string, number>;
  fonts: Record<string, string>;
}

export const DEFAULT_TOKENS: DesignTokenBundle = {
  colors: {
    primary: "#3b82f6",
    secondary: "#64748b",
    accent: "#a78bfa",
    success: "#22c55e",
    warning: "#f59e0b",
    danger: "#ef4444",
    bg: "#ffffff",
    text: "#0f172a",
    muted: "#64748b",
    border: "#e2e8f0",
  },
  spacing: {
    xs: 4,
    sm: 8,
    md: 16,
    lg: 24,
    xl: 40,
    xxl: 64,
  },
  fonts: {
    sans: "system-ui, -apple-system, sans-serif",
    serif: "Georgia, serif",
    mono: "'JetBrains Mono', Menlo, Consolas, monospace",
  },
};

/** Render a bundle to a `:root { --token: value; }` CSS string. */
export function tokensToCss(bundle: DesignTokenBundle): string {
  const lines: string[] = [];
  for (const [k, v] of Object.entries(bundle.colors)) {
    lines.push(`--color-${k}: ${v};`);
  }
  for (const [k, v] of Object.entries(bundle.spacing)) {
    lines.push(`--space-${k}: ${v}px;`);
  }
  for (const [k, v] of Object.entries(bundle.fonts)) {
    lines.push(`--font-${k}: ${v};`);
  }
  return `:root { ${lines.join(" ")} }`;
}

/** Look up a token by dotted path ("colors.primary"), returning undefined on miss. */
export function lookupToken(bundle: DesignTokenBundle, path: string): string | number | undefined {
  const [bucket, key] = path.split(".");
  if (!bucket || !key) return undefined;
  const b = (bundle as unknown as Record<string, Record<string, string | number>>)[bucket];
  if (!b) return undefined;
  return b[key];
}

/** Merge overrides into a base bundle (shallow per-bucket). */
export function mergeTokens(base: DesignTokenBundle, patch: Partial<DesignTokenBundle>): DesignTokenBundle {
  return {
    colors: { ...base.colors, ...(patch.colors ?? {}) },
    spacing: { ...base.spacing, ...(patch.spacing ?? {}) },
    fonts: { ...base.fonts, ...(patch.fonts ?? {}) },
  };
}

/** A minimal registry with observers so the panel can subscribe. */
export class DesignTokenRegistry {
  private bundle: DesignTokenBundle;
  private listeners = new Set<() => void>();

  constructor(initial: DesignTokenBundle = DEFAULT_TOKENS) {
    this.bundle = initial;
  }

  get(): DesignTokenBundle {
    return this.bundle;
  }

  set(bundle: DesignTokenBundle): void {
    this.bundle = bundle;
    for (const l of this.listeners) l();
  }

  patch(patch: Partial<DesignTokenBundle>): void {
    this.set(mergeTokens(this.bundle, patch));
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }
}

export function createDesignTokenRegistry(initial?: DesignTokenBundle): DesignTokenRegistry {
  return new DesignTokenRegistry(initial);
}
