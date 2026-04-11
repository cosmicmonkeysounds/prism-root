/**
 * Tab Bar — horizontal tab management with close and pin controls.
 */

import { useLensContext, useShellStore } from "./lens-context.js";
import type { TabEntry } from "@prism/core/lens";

function Tab({
  tab,
  isActive,
  onActivate,
  onClose,
  onPin,
}: {
  tab: TabEntry;
  isActive: boolean;
  onActivate: () => void;
  onClose: () => void;
  onPin: () => void;
}) {
  return (
    <div
      data-testid={`tab-${tab.lensId}`}
      data-pinned={tab.pinned ? "true" : "false"}
      onClick={onActivate}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          onClose();
        }
      }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 8px",
        background: isActive ? "#1e1e1e" : "#2d2d2d",
        color: isActive ? "#fff" : "#999",
        borderRight: "1px solid #333",
        cursor: "pointer",
        fontSize: 12,
        whiteSpace: "nowrap",
        userSelect: "none",
      }}
    >
      {tab.pinned && (
        <span style={{ fontSize: 10, color: "#007acc" }} title="Pinned">
          *
        </span>
      )}
      <span>{tab.label}</span>
      <button
        data-testid={`tab-pin-${tab.lensId}`}
        title={tab.pinned ? "Unpin tab" : "Pin tab"}
        onClick={(e) => {
          e.stopPropagation();
          onPin();
        }}
        style={{
          background: "none",
          border: "none",
          color: tab.pinned ? "#007acc" : "#666",
          cursor: "pointer",
          fontSize: 10,
          padding: "0 2px",
          lineHeight: 1,
        }}
      >
        {tab.pinned ? "\u25C9" : "\u25CB"}
      </button>
      <button
        data-testid={`tab-close-${tab.lensId}`}
        title="Close tab"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        style={{
          background: "none",
          border: "none",
          color: "#666",
          cursor: "pointer",
          fontSize: 12,
          padding: "0 2px",
          lineHeight: 1,
        }}
      >
        \u00d7
      </button>
    </div>
  );
}

export function TabBar() {
  const { store } = useLensContext();
  const { tabs, activeTabId } = useShellStore();

  const sortedTabs = [...tabs].sort((a, b) => a.order - b.order);

  return (
    <div
      data-testid="tab-bar"
      style={{
        display: "flex",
        background: "#2d2d2d",
        borderBottom: "1px solid #333",
        minHeight: 32,
        overflow: "hidden",
      }}
    >
      {sortedTabs.map((tab) => (
        <Tab
          key={tab.id}
          tab={tab}
          isActive={tab.id === activeTabId}
          onActivate={() => store.getState().setActiveTab(tab.id)}
          onClose={() => store.getState().closeTab(tab.id)}
          onPin={() =>
            tab.pinned
              ? store.getState().unpinTab(tab.id)
              : store.getState().pinTab(tab.id)
          }
        />
      ))}
    </div>
  );
}
