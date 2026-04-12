/**
 * Design Tokens panel — CSS variable registry editor.
 *
 * Edits `kernel.designTokens` (colors / spacing / fonts) and shows a live
 * preview of `:root` as it will be injected into the document or exported
 * into an HTML page. Tier 3E of `docs/dev/studio-checklist.md`.
 */

import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { useKernel } from "../kernel/index.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  tokensToCss,
  type DesignTokenBundle,
} from "@prism/core/design-tokens";

const panelStyle: CSSProperties = {
  height: "100%",
  display: "flex",
  flexDirection: "column",
  background: "#1e1e1e",
  color: "#d4d4d4",
  fontFamily: "system-ui, sans-serif",
};

const headerStyle: CSSProperties = {
  padding: "12px 16px",
  borderBottom: "1px solid #333",
  background: "#252526",
  fontSize: 14,
  fontWeight: 600,
};

const bodyStyle: CSSProperties = {
  flex: 1,
  overflow: "auto",
  padding: 16,
  display: "grid",
  gridTemplateColumns: "1fr 1fr",
  gap: 16,
};

const sectionStyle: CSSProperties = {
  background: "#252526",
  border: "1px solid #333",
  borderRadius: 6,
  padding: 12,
};

const rowStyle: CSSProperties = {
  display: "grid",
  gridTemplateColumns: "100px 1fr auto",
  gap: 8,
  alignItems: "center",
  marginBottom: 6,
};

const inputStyle: CSSProperties = {
  background: "#1e1e1e",
  border: "1px solid #444",
  borderRadius: 4,
  color: "#d4d4d4",
  padding: "4px 6px",
  fontSize: 12,
};

const swatchStyle = (color: string): CSSProperties => ({
  width: 18,
  height: 18,
  borderRadius: 4,
  border: "1px solid #444",
  background: color,
});

const preStyle: CSSProperties = {
  gridColumn: "1 / -1",
  background: "#0f172a",
  color: "#94a3b8",
  padding: 12,
  borderRadius: 6,
  fontSize: 11,
  fontFamily: "monospace",
  overflow: "auto",
  maxHeight: 240,
};

export function DesignTokensPanel() {
  const kernel = useKernel();
  const [bundle, setBundle] = useState<DesignTokenBundle>(() => kernel.designTokens.get());

  useEffect(() => {
    return kernel.designTokens.subscribe(() => setBundle(kernel.designTokens.get()));
  }, [kernel.designTokens]);

  const updateColor = useCallback(
    (key: string, value: string) => {
      kernel.designTokens.patch({ colors: { [key]: value } });
    },
    [kernel.designTokens],
  );

  const updateSpacing = useCallback(
    (key: string, value: number) => {
      kernel.designTokens.patch({ spacing: { [key]: value } });
    },
    [kernel.designTokens],
  );

  const updateFont = useCallback(
    (key: string, value: string) => {
      kernel.designTokens.patch({ fonts: { [key]: value } });
    },
    [kernel.designTokens],
  );

  const css = tokensToCss(bundle);

  return (
    <div style={panelStyle} data-testid="design-tokens-panel">
      <div style={headerStyle}>Design Tokens</div>
      <div style={bodyStyle}>
        <div style={sectionStyle}>
          <div style={{ marginBottom: 8, fontWeight: 600 }}>Colors</div>
          {Object.entries(bundle.colors).map(([k, v]) => (
            <div key={k} style={rowStyle}>
              <label style={{ fontSize: 12, color: "#94a3b8" }}>{k}</label>
              <input
                type="text"
                value={v}
                onChange={(e) => updateColor(k, e.target.value)}
                style={inputStyle}
                data-testid={`token-color-${k}`}
              />
              <span style={swatchStyle(v)} aria-hidden="true" />
            </div>
          ))}
        </div>
        <div style={sectionStyle}>
          <div style={{ marginBottom: 8, fontWeight: 600 }}>Spacing (px)</div>
          {Object.entries(bundle.spacing).map(([k, v]) => (
            <div key={k} style={rowStyle}>
              <label style={{ fontSize: 12, color: "#94a3b8" }}>{k}</label>
              <input
                type="number"
                value={v}
                onChange={(e) => updateSpacing(k, Number(e.target.value))}
                style={inputStyle}
                data-testid={`token-space-${k}`}
              />
              <span style={{ fontSize: 11, color: "#64748b" }}>px</span>
            </div>
          ))}
        </div>
        <div style={{ ...sectionStyle, gridColumn: "1 / -1" }}>
          <div style={{ marginBottom: 8, fontWeight: 600 }}>Fonts</div>
          {Object.entries(bundle.fonts).map(([k, v]) => (
            <div key={k} style={{ ...rowStyle, gridTemplateColumns: "100px 1fr" }}>
              <label style={{ fontSize: 12, color: "#94a3b8" }}>{k}</label>
              <input
                type="text"
                value={v}
                onChange={(e) => updateFont(k, e.target.value)}
                style={inputStyle}
                data-testid={`token-font-${k}`}
              />
            </div>
          ))}
        </div>
        <pre style={preStyle} data-testid="tokens-css-preview">
          {css}
        </pre>
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const DESIGN_TOKENS_LENS_ID = lensId("design-tokens");

export const designTokensLensManifest: LensManifest = {

  id: DESIGN_TOKENS_LENS_ID,
  name: "Design Tokens",
  icon: "\u{1F3A8}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-design-tokens", name: "Switch to Design Tokens", shortcut: ["shift+t"], section: "Navigation" },
    ],
  },
};

export const designTokensLensBundle: LensBundle = defineLensBundle(
  designTokensLensManifest,
  DesignTokensPanel,
);
