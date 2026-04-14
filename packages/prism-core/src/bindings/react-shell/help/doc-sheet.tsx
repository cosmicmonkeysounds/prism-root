import { useEffect, useRef, useState } from "react";
import { HelpMarkdown, slugify } from "./help-markdown.js";

export interface DocSheetProps {
  /** Logical path to the documentation (displayed in header, passed to fetchDoc). */
  docPath: string;
  /** Optional heading slug to scroll to after content loads. */
  anchor?: string;
  /** Close handler. */
  onClose: () => void;
  /**
   * Resolve a doc path to its markdown source. Callers provide this so the
   * sheet is not tied to any specific backend — Studio uses a bundled map
   * of `?raw` imports, a server app would fetch HTTP, etc.
   */
  fetchDoc: (path: string) => Promise<string>;
}

/**
 * DocSheet — slide-in markdown panel for full documentation.
 *
 * Fixed position, right-aligned, 640 px wide with a semi-opaque backdrop.
 * Loads the markdown source via the caller-provided `fetchDoc`, tokenizes
 * it with `parseMarkdown` from `@prism/core/forms` (via HelpMarkdown), and
 * scrolls to the optional `anchor` after render.
 *
 * Ported from $legacy-inspiration-only/helm/components/src/help/doc-sheet.tsx
 * per ADR-005. Differences: no SlidingPane dependency — the panel is a
 * self-contained fixed div; markdown rendering uses the canonical Prism
 * tokenizer instead of a private `MarkdownViewer`.
 */
export function DocSheet({ docPath, anchor, onClose, fetchDoc }: DocSheetProps) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setContent(null);
    fetchDoc(docPath)
      .then((text) => {
        if (!cancelled) setContent(text);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [docPath, fetchDoc]);

  useEffect(() => {
    if (!content || !anchor) return;
    const el = scrollRef.current;
    if (!el) return;
    const slug = slugify(anchor);
    const target =
      el.querySelector(`[data-anchor="${slug}"]`) ??
      el.querySelector(`#${CSS.escape(slug)}`);
    if (target) {
      requestAnimationFrame(() => {
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      });
    }
  }, [content, anchor]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const segments = docPath.split("/").filter(Boolean);
  const filename = segments[segments.length - 1] ?? docPath;

  return (
    <>
      <div
        data-testid="prism-doc-sheet-backdrop"
        onClick={onClose}
        style={{
          position: "fixed",
          inset: 0,
          background: "rgba(15, 23, 42, 0.35)",
          zIndex: 9997,
        }}
      />
      <aside
        role="dialog"
        aria-label={`Documentation: ${filename}`}
        data-testid="prism-doc-sheet"
        style={{
          position: "fixed",
          top: 0,
          right: 0,
          bottom: 0,
          width: "min(640px, 92vw)",
          background: "white",
          color: "#0f172a",
          borderLeft: "1px solid #e2e8f0",
          boxShadow: "-20px 0 40px -20px rgba(15, 23, 42, 0.3)",
          zIndex: 9998,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <header
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "12px 20px",
            borderBottom: "1px solid #e2e8f0",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
            <span
              style={{
                fontSize: 11,
                color: "#64748b",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
              }}
            >
              Documentation
            </span>
            <span
              style={{
                fontSize: 14,
                fontWeight: 600,
                color: "#0f172a",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
              title={docPath}
            >
              {filename}
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close documentation"
            style={{
              border: "1px solid #e2e8f0",
              background: "white",
              borderRadius: 6,
              width: 28,
              height: 28,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
              color: "#475569",
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </header>
        <div
          ref={scrollRef}
          className="prism-help-doc-body"
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "20px 28px 32px",
            fontSize: 14,
            lineHeight: 1.6,
          }}
        >
          {loading ? (
            <p style={{ color: "#64748b" }}>Loading documentation…</p>
          ) : null}
          {error ? (
            <div
              style={{
                border: "1px solid #fecaca",
                background: "#fef2f2",
                borderRadius: 6,
                padding: "10px 14px",
              }}
            >
              <p style={{ color: "#b91c1c", margin: 0, fontWeight: 600 }}>
                Failed to load documentation
              </p>
              <p style={{ color: "#dc2626", margin: "4px 0 0", fontSize: 13 }}>
                {error}
              </p>
            </div>
          ) : null}
          {content ? <HelpMarkdown source={content} /> : null}
        </div>
      </aside>
    </>
  );
}
