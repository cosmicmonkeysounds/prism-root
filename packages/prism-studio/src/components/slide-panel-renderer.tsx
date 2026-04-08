/**
 * SlidePanelRenderer — collapsible accordion panel for layouts.
 *
 * CSS height transition with toggle. Useful for optional content
 * inside dense forms or layout compositions.
 */

import { useState } from "react";

export interface SlidePanelProps {
  label: string;
  content: string;
  collapsed?: boolean;
}

export function SlidePanelRenderer(props: SlidePanelProps) {
  const { label, content, collapsed = false } = props;
  const [open, setOpen] = useState(!collapsed);

  return (
    <div
      data-testid="slide-panel"
      style={{
        border: "1px solid #6366f1",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
        margin: "4px 0",
      }}
    >
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        data-testid="slide-panel-toggle"
        aria-expanded={open}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          background: "#1e293b",
          color: "#e2e8f0",
          border: "none",
          borderRadius: "6px 6px 0 0",
          padding: "6px 12px",
          width: "100%",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
          textAlign: "left",
        }}
      >
        <span style={{ color: "#6366f1" }}>{open ? "\u25BC" : "\u25B6"}</span>
        <span>{label}</span>
      </button>
      {open ? (
        <div
          data-testid="slide-panel-content"
          style={{
            padding: 10,
            fontSize: 12,
            borderTop: "1px solid #334155",
            whiteSpace: "pre-wrap",
          }}
        >
          {content || <span style={{ color: "#94a3b8" }}>No content.</span>}
        </div>
      ) : null}
    </div>
  );
}
