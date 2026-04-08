/**
 * TabContainerRenderer — simple tab bar widget for layouts.
 *
 * Takes a comma-separated or JSON array list of tab labels and displays
 * a horizontal tab bar. The active tab is stored in local state so users
 * can preview switching inside both the Puck builder and the canvas.
 * Child content is rendered via the `children` slot (Puck layout tree).
 */

import { useState, type ReactNode } from "react";

export interface TabContainerProps {
  tabs: string;
  activeTab?: number;
  children?: ReactNode;
}

/** Parse the `tabs` prop — accepts JSON array or comma-separated labels. */
export function parseTabs(raw: string): string[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("[")) {
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (Array.isArray(parsed)) return parsed.map((v) => String(v));
    } catch {
      // Fall through to CSV parse.
    }
  }
  return trimmed.split(",").map((s) => s.trim()).filter(Boolean);
}

export function TabContainerRenderer(props: TabContainerProps) {
  const { tabs: tabsRaw, activeTab = 0, children } = props;
  const tabs = parseTabs(tabsRaw);
  const [active, setActive] = useState(() => Math.max(0, Math.min(activeTab, tabs.length - 1)));

  return (
    <div
      data-testid="tab-container"
      style={{
        border: "1px solid #f97316",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
      }}
    >
      <div
        role="tablist"
        style={{
          display: "flex",
          borderBottom: "1px solid #f97316",
          background: "#1e293b",
        }}
      >
        {tabs.length === 0 ? (
          <div style={{ padding: "8px 12px", fontSize: 12, color: "#94a3b8" }}>
            No tabs configured.
          </div>
        ) : (
          tabs.map((label, i) => (
            <button
              key={`${label}-${i}`}
              type="button"
              role="tab"
              aria-selected={i === active}
              data-testid={`tab-button-${i}`}
              onClick={() => setActive(i)}
              style={{
                background: i === active ? "#0f172a" : "transparent",
                color: i === active ? "#f97316" : "#94a3b8",
                border: "none",
                borderBottom: i === active ? "2px solid #f97316" : "2px solid transparent",
                padding: "6px 12px",
                fontSize: 12,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              {label}
            </button>
          ))
        )}
      </div>
      <div data-testid="tab-content" style={{ padding: 8, minHeight: 60 }}>
        {children}
      </div>
    </div>
  );
}
