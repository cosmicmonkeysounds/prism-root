import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { useHelp } from "./help-context.js";
import type { HelpEntry } from "./types.js";

/**
 * HelpTooltip — hover popover showing context-sensitive help.
 *
 * Wraps any element. On hover (after a 380 ms delay) shows a popover
 * containing the HelpEntry summary. Portal-rendered to `document.body`
 * so it is never clipped by `overflow: hidden` ancestors. Singleton: a
 * new tooltip dismisses any other currently-visible one.
 *
 * When the entry has a `docPath`, a "View full docs" button appears at
 * the bottom and calls `useHelp().openDoc(path, anchor)` — which the app
 * wires to a DocSheet.
 *
 * Ported from $legacy-inspiration-only/helm/components/src/help/help-tooltip.tsx
 * per ADR-005. Differences from legacy:
 * - Icons are optional ReactNode on HelpEntry (no lucide-react dep).
 * - Inline className strings (no `cn` helper).
 * - Inline SVG fallback chrome.
 */

const SHOW_DELAY_MS = 380;
const HIDE_DELAY_MS = 120;
const TOOLTIP_WIDTH = 276;

// Singleton dismiss map — any new tooltip about to show dismisses all
// other currently-visible ones, so the screen never has two at once.
let _nextId = 0;
const _dismissCallbacks = new Map<number, () => void>();

function _registerTooltip(onDismiss: () => void): {
  id: number;
  unregister: () => void;
} {
  const id = _nextId++;
  _dismissCallbacks.set(id, onDismiss);
  return { id, unregister: () => _dismissCallbacks.delete(id) };
}

function _dismissAllExcept(keepId: number): void {
  _dismissCallbacks.forEach((dismiss, id) => {
    if (id !== keepId) dismiss();
  });
}

export interface HelpTooltipProps {
  entry: HelpEntry;
  children: ReactNode;
  /** If true, shows a small "?" indicator after the children. */
  showIcon?: boolean;
  className?: string;
}

export function HelpTooltip({
  entry,
  children,
  showIcon = false,
  className,
}: HelpTooltipProps) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const showTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const registration = useRef<{ id: number; unregister: () => void } | null>(
    null,
  );
  const { openDoc } = useHelp();

  const clearTimers = useCallback(() => {
    if (showTimer.current) {
      clearTimeout(showTimer.current);
      showTimer.current = null;
    }
    if (hideTimer.current) {
      clearTimeout(hideTimer.current);
      hideTimer.current = null;
    }
  }, []);

  const dismiss = useCallback(() => {
    setPos(null);
    clearTimers();
  }, [clearTimers]);

  useEffect(() => {
    const reg = _registerTooltip(dismiss);
    registration.current = reg;
    return () => {
      reg.unregister();
      dismiss();
    };
  }, [dismiss]);

  const scheduleShow = useCallback(() => {
    clearTimers();
    showTimer.current = setTimeout(() => {
      const el = wrapperRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const fitsRight = r.right + 8 + TOOLTIP_WIDTH <= window.innerWidth;
      const x = fitsRight ? r.right + 8 : Math.max(8, r.left - TOOLTIP_WIDTH - 8);
      const y = Math.min(r.top, window.innerHeight - 220);
      if (registration.current) _dismissAllExcept(registration.current.id);
      setPos({ x, y });
    }, SHOW_DELAY_MS);
  }, [clearTimers]);

  const scheduleHide = useCallback(() => {
    clearTimers();
    hideTimer.current = setTimeout(() => setPos(null), HIDE_DELAY_MS);
  }, [clearTimers]);

  useEffect(() => {
    if (!pos) return;
    const onScroll = () => setPos(null);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPos(null);
    };
    window.addEventListener("scroll", onScroll, {
      capture: true,
      passive: true,
    });
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("scroll", onScroll, { capture: true });
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [pos]);

  useEffect(() => () => clearTimers(), [clearTimers]);

  return (
    <span
      ref={wrapperRef}
      className={className}
      style={{ position: "relative", display: "inline-flex", alignItems: "center", gap: 4 }}
      onMouseEnter={scheduleShow}
      onMouseLeave={scheduleHide}
      onFocus={scheduleShow}
      onBlur={scheduleHide}
    >
      {children}
      {showIcon ? (
        <span
          aria-hidden
          className="prism-help-indicator"
          style={{
            display: "inline-flex",
            width: 12,
            height: 12,
            borderRadius: "50%",
            background: "rgba(100, 116, 139, 0.18)",
            color: "#475569",
            fontSize: 9,
            lineHeight: "12px",
            fontWeight: 700,
            textAlign: "center",
            justifyContent: "center",
            alignItems: "center",
          }}
        >
          ?
        </span>
      ) : null}
      {pos !== null && typeof document !== "undefined"
        ? createPortal(
            <HelpPopover
              entry={entry}
              pos={pos}
              onMouseEnter={clearTimers}
              onMouseLeave={scheduleHide}
              onOpenDoc={(path, anchor) => {
                setPos(null);
                openDoc(path, anchor);
              }}
            />,
            document.body,
          )
        : null}
    </span>
  );
}

function HelpPopover({
  entry,
  pos,
  onMouseEnter,
  onMouseLeave,
  onOpenDoc,
}: {
  entry: HelpEntry;
  pos: { x: number; y: number };
  onMouseEnter: () => void;
  onMouseLeave: () => void;
  onOpenDoc: (path: string, anchor?: string) => void;
}) {
  return (
    <div
      role="tooltip"
      aria-label={entry.title}
      data-testid="prism-help-tooltip"
      style={{
        position: "fixed",
        left: pos.x,
        top: pos.y,
        width: TOOLTIP_WIDTH,
        zIndex: 9999,
        overflow: "hidden",
        borderRadius: 10,
        border: "1px solid rgb(39, 39, 42)",
        background: "rgb(24, 24, 27)",
        color: "rgb(228, 228, 231)",
        boxShadow: "0 20px 40px -12px rgba(0, 0, 0, 0.6)",
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          borderBottom: "1px solid rgb(39, 39, 42)",
          padding: "8px 12px",
        }}
      >
        {entry.icon ? (
          <span style={{ color: "rgb(167, 139, 250)", display: "inline-flex" }}>
            {entry.icon}
          </span>
        ) : (
          <BookIcon />
        )}
        <span style={{ fontSize: 12, fontWeight: 600 }}>{entry.title}</span>
      </div>
      <p
        style={{
          fontSize: 12,
          lineHeight: 1.5,
          padding: "10px 12px",
          margin: 0,
          color: "rgb(161, 161, 170)",
        }}
      >
        {entry.summary}
      </p>
      {entry.docPath ? (
        <div
          style={{
            borderTop: "1px solid rgb(39, 39, 42)",
            padding: "8px 12px",
          }}
        >
          <button
            type="button"
            onClick={() => {
              if (entry.docPath) onOpenDoc(entry.docPath, entry.docAnchor);
            }}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              border: "none",
              background: "transparent",
              color: "rgb(167, 139, 250)",
              fontSize: 12,
              cursor: "pointer",
              padding: 0,
            }}
          >
            <BookIcon />
            View full docs
          </button>
        </div>
      ) : null}
    </div>
  );
}

function BookIcon() {
  return (
    <svg
      width="11"
      height="11"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M4 19.5a2.5 2.5 0 0 1 2.5-2.5H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
    </svg>
  );
}
