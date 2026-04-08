/**
 * PopoverWidgetRenderer — button trigger that reveals floating content.
 *
 * Pure CSS absolute positioning, no floating-ui dependency. Good enough
 * for layout previews; production popovers should swap in a collision-
 * aware positioner later.
 */

import { useState } from "react";

export interface PopoverWidgetProps {
  triggerLabel: string;
  content: string;
}

export function PopoverWidgetRenderer(props: PopoverWidgetProps) {
  const { triggerLabel, content } = props;
  const [open, setOpen] = useState(false);

  return (
    <div data-testid="popover-widget" style={{ position: "relative", display: "inline-block", margin: "4px 0" }}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        data-testid="popover-trigger"
        aria-expanded={open}
        style={{
          background: "#ec4899",
          color: "#0f172a",
          border: "none",
          borderRadius: 4,
          padding: "6px 12px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        {triggerLabel}
      </button>
      {open ? (
        <div
          role="dialog"
          data-testid="popover-content"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            marginTop: 4,
            zIndex: 10,
            minWidth: 220,
            background: "#0f172a",
            border: "1px solid #ec4899",
            borderRadius: 4,
            padding: 10,
            color: "#e2e8f0",
            fontSize: 12,
            boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
          }}
        >
          {content || <span style={{ color: "#94a3b8" }}>No content.</span>}
        </div>
      ) : null}
    </div>
  );
}
