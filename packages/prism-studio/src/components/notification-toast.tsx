/**
 * Notification Toast — floating overlay showing recent notifications.
 *
 * Subscribes to the kernel's NotificationStore and displays toasts
 * for info/success/warning/error kinds with auto-dismiss.
 */

import { useState, useEffect, useCallback } from "react";
import { useKernel } from "../kernel/index.js";
import type { Notification } from "@prism/core/notification";

const TOAST_KINDS = ["info", "success", "warning", "error"] as const;
const AUTO_DISMISS_MS = 4000;

const defaultColors = { bg: "#1e3a5f", border: "#3b82f6", text: "#93c5fd" };
const kindColors: Record<string, { bg: string; border: string; text: string }> = {
  info: { bg: "#1e3a5f", border: "#3b82f6", text: "#93c5fd" },
  success: { bg: "#14532d", border: "#22c55e", text: "#86efac" },
  warning: { bg: "#422006", border: "#f59e0b", text: "#fcd34d" },
  error: { bg: "#450a0a", border: "#ef4444", text: "#fca5a5" },
};

export function NotificationToast() {
  const kernel = useKernel();
  const [visible, setVisible] = useState<Notification[]>([]);

  useEffect(() => {
    const unsub = kernel.notifications.subscribe((change) => {
      if (change.type !== "add") return;
      const item = change.notification;
      if (!item) return;
      if (!TOAST_KINDS.includes(item.kind as (typeof TOAST_KINDS)[number])) return;

      setVisible((prev) => [...prev, item]);

      // Auto-dismiss
      setTimeout(() => {
        setVisible((prev) => prev.filter((n) => n.id !== item.id));
        kernel.notifications.dismiss(item.id);
      }, AUTO_DISMISS_MS);
    });
    return unsub;
  }, [kernel]);

  const dismissToast = useCallback(
    (id: string) => {
      setVisible((prev) => prev.filter((n) => n.id !== id));
      kernel.notifications.dismiss(id);
    },
    [kernel],
  );

  if (visible.length === 0) return null;

  return (
    <div
      data-testid="notification-container"
      style={{
        position: "fixed",
        bottom: 16,
        right: 16,
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: 360,
      }}
    >
      {visible.map((n) => {
        const colors = kindColors[n.kind] ?? defaultColors;
        return (
          <div
            key={n.id}
            data-testid="notification-toast"
            style={{
              background: colors.bg,
              border: `1px solid ${colors.border}`,
              borderRadius: 6,
              padding: "10px 14px",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "flex-start",
              gap: 8,
              boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
            }}
          >
            <div>
              <div style={{ color: colors.text, fontSize: 13, fontWeight: 500 }}>
                {n.title}
              </div>
              {n.body && (
                <div style={{ color: colors.text, fontSize: 11, opacity: 0.8, marginTop: 2 }}>
                  {n.body}
                </div>
              )}
            </div>
            <button
              onClick={() => dismissToast(n.id)}
              style={{
                background: "none",
                border: "none",
                color: colors.text,
                cursor: "pointer",
                fontSize: 14,
                opacity: 0.6,
                padding: 0,
                lineHeight: 1,
              }}
            >
              \u00d7
            </button>
          </div>
        );
      })}
    </div>
  );
}
