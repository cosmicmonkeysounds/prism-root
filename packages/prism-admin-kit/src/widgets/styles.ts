/**
 * Shared inline style objects for admin-kit widgets.
 *
 * We keep the palette in one place so widgets drop into any Prism surface
 * (Studio panels, Puck canvas, puck-playground) with a consistent look
 * without forcing a CSS-in-JS runtime on consumers.
 */

import type { CSSProperties } from "react";

export const palette = {
  bg: "#1e1e1e",
  card: "#252526",
  cardAlt: "#2d2d30",
  border: "#333333",
  borderStrong: "#444444",
  textStrong: "#e5e5e5",
  text: "#cccccc",
  textDim: "#888888",
  textDimmer: "#555555",
  accent: "#6366f1",
  accentSoft: "#818cf8",
} as const;

export const widgetStyles: Record<string, CSSProperties> = {
  card: {
    background: palette.card,
    border: `1px solid ${palette.border}`,
    borderRadius: "0.5rem",
    padding: "0.875rem 1rem",
    color: palette.text,
    fontFamily: "system-ui, -apple-system, sans-serif",
  },
  cardTitle: {
    fontSize: "0.6875rem",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: palette.textDim,
    marginBottom: "0.5rem",
    fontWeight: 600,
  },
  metricValue: {
    fontSize: "1.75rem",
    fontWeight: 600,
    color: palette.textStrong,
    lineHeight: 1.1,
  },
  metricHint: {
    fontSize: "0.75rem",
    color: palette.textDim,
    marginTop: "0.25rem",
  },
  badge: {
    display: "inline-flex",
    alignItems: "center",
    gap: "0.375rem",
    fontSize: "0.6875rem",
    padding: "0.25rem 0.625rem",
    borderRadius: "999px",
    border: "1px solid",
    fontWeight: 500,
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0.375rem 0",
    borderBottom: `1px solid ${palette.border}`,
    fontSize: "0.8125rem",
  },
  rowLast: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0.375rem 0",
    fontSize: "0.8125rem",
  },
  timestamp: {
    fontSize: "0.6875rem",
    color: palette.textDimmer,
    fontFamily: "ui-monospace, monospace",
  },
  empty: {
    fontSize: "0.75rem",
    color: palette.textDimmer,
    fontStyle: "italic",
    textAlign: "center",
    padding: "0.75rem",
  },
};
