/**
 * Shell Mode Menu — top-bar dropdown for switching shell modes.
 *
 * The authoritative UI affordance for Cmd+Shift+E. Renders in the
 * `topBar` slot of the `build` and `admin` shell trees and shows:
 *
 *   - The active mode label (Use / Build / Admin).
 *   - A badge for the boot-resolved `permission` (user / dev).
 *   - A click-to-open menu listing all three modes with a tick beside
 *     the active one.
 *
 * Switching modes calls `kernel.setShellMode()` via the `useShellMode`
 * hook, which swaps the Puck tree in-place — `StudioShell` re-renders
 * from the new tree and the user stays on whatever tab they had open.
 *
 * Permission is frozen at boot (see `boot/load-boot-config.ts`) and is
 * displayed read-only; escalating from `user` → `dev` requires a real
 * restart with different boot config / daemon flags and is a security
 * boundary rather than a UI affordance.
 */

import { useEffect, useRef, useState } from "react";
import type { ShellMode } from "@prism/core/lens";
import { useShellMode } from "../kernel/kernel-context.js";

const MODE_LABELS: Record<ShellMode, string> = {
  use: "Use",
  build: "Build",
  admin: "Admin",
};

const MODE_DESCRIPTIONS: Record<ShellMode, string> = {
  use: "Published app view",
  build: "Authoring palette",
  admin: "Full IDE",
};

export function ShellModeMenu() {
  const { mode, permission, setMode } = useShellMode();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Close on outside click / Escape so the dropdown stays disposable.
  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (!rootRef.current) return;
      if (!rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const buttonStyle: React.CSSProperties = {
    background: open ? "#2a2a2a" : "transparent",
    border: "1px solid #3a3a3a",
    borderRadius: 4,
    color: "#ccc",
    cursor: "pointer",
    fontSize: 12,
    fontFamily: "inherit",
    padding: "3px 10px",
    lineHeight: "22px",
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
  };

  const badgeStyle: React.CSSProperties = {
    background: permission === "dev" ? "#2d4d8b" : "#3a3a3a",
    color: "#eee",
    borderRadius: 3,
    fontSize: 10,
    padding: "1px 5px",
    textTransform: "uppercase",
    letterSpacing: 0.4,
  };

  const menuStyle: React.CSSProperties = {
    position: "absolute",
    top: "calc(100% + 2px)",
    left: 0,
    minWidth: 180,
    background: "#1e1e1e",
    border: "1px solid #3a3a3a",
    borderRadius: 4,
    boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
    padding: 4,
    zIndex: 100,
  };

  const itemStyle = (active: boolean): React.CSSProperties => ({
    display: "flex",
    alignItems: "center",
    gap: 8,
    width: "100%",
    padding: "6px 8px",
    background: active ? "#2d4d8b" : "transparent",
    border: "none",
    borderRadius: 3,
    color: active ? "#fff" : "#ccc",
    cursor: "pointer",
    fontSize: 12,
    fontFamily: "inherit",
    textAlign: "left",
  });

  function handlePick(next: ShellMode) {
    setOpen(false);
    if (next !== mode) setMode(next);
  }

  return (
    <div ref={rootRef} style={{ position: "relative" }}>
      <button
        type="button"
        data-testid="shell-mode-menu-button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        title="Switch shell mode (Cmd+Shift+E)"
        style={buttonStyle}
      >
        <span>{MODE_LABELS[mode]}</span>
        <span style={badgeStyle}>{permission}</span>
        <span style={{ fontSize: 9, opacity: 0.7 }}>{"\u25BE"}</span>
      </button>
      {open ? (
        <div role="menu" data-testid="shell-mode-menu" style={menuStyle}>
          {(Object.keys(MODE_LABELS) as ShellMode[]).map((m) => (
            <button
              key={m}
              type="button"
              role="menuitemradio"
              aria-checked={m === mode}
              data-testid={`shell-mode-menu-item-${m}`}
              onClick={() => handlePick(m)}
              style={itemStyle(m === mode)}
            >
              <span style={{ width: 12, textAlign: "center" }}>
                {m === mode ? "\u2713" : ""}
              </span>
              <span style={{ flex: 1 }}>{MODE_LABELS[m]}</span>
              <span style={{ fontSize: 10, opacity: 0.7 }}>
                {MODE_DESCRIPTIONS[m]}
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
