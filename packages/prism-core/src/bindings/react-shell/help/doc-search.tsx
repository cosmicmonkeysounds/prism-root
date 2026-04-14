import { useCallback, useMemo, useState } from "react";
import { HelpRegistry } from "./help-registry.js";
import { useHelp } from "./help-context.js";
import type { HelpEntry } from "./types.js";

export interface DocSearchProps {
  placeholder?: string;
  maxResults?: number;
  className?: string;
  /** Called after the user picks a result (useful to close a surrounding popover). */
  onPick?: (entry: HelpEntry) => void;
  autoFocus?: boolean;
}

/**
 * DocSearch — search input + result list over the global HelpRegistry.
 *
 * Filters by title/summary via `HelpRegistry.search()` — the same AND-word
 * substring logic used everywhere else. Clicking a result with a
 * `docPath` calls `useHelp().openDoc()`; results without a docPath render
 * as disabled (summary-only) items. `onPick` fires for every selection so
 * the caller can dismiss an anchoring popover.
 */
export function DocSearch({
  placeholder = "Search documentation…",
  maxResults = 50,
  className,
  onPick,
  autoFocus,
}: DocSearchProps) {
  const [query, setQuery] = useState("");
  const { openDoc } = useHelp();

  const results = useMemo(() => {
    if (!query.trim()) return [];
    return HelpRegistry.search(query).slice(0, maxResults);
  }, [query, maxResults]);

  const handleSelect = useCallback(
    (entry: HelpEntry) => {
      if (entry.docPath) openDoc(entry.docPath, entry.docAnchor);
      onPick?.(entry);
    },
    [openDoc, onPick],
  );

  return (
    <div
      className={className}
      data-testid="prism-doc-search"
      style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 280 }}
    >
      <div style={{ position: "relative" }}>
        <span
          aria-hidden
          style={{
            position: "absolute",
            left: 10,
            top: "50%",
            transform: "translateY(-50%)",
            color: "#64748b",
            display: "inline-flex",
            pointerEvents: "none",
          }}
        >
          <SearchIcon />
        </span>
        <input
          type="text"
          value={query}
          autoFocus={autoFocus}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={placeholder}
          style={{
            width: "100%",
            padding: "8px 10px 8px 32px",
            border: "1px solid #e2e8f0",
            borderRadius: 6,
            background: "white",
            color: "#0f172a",
            fontSize: 13,
            outline: "none",
          }}
        />
      </div>
      {query.trim() ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 2,
            maxHeight: 320,
            overflowY: "auto",
          }}
        >
          {results.length === 0 ? (
            <p
              style={{
                fontSize: 12,
                color: "#64748b",
                margin: 0,
                padding: "8px 4px",
              }}
            >
              No entries matching "{query}".
            </p>
          ) : (
            results.map((entry) => (
              <DocSearchResult
                key={entry.id}
                entry={entry}
                onSelect={handleSelect}
              />
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}

function DocSearchResult({
  entry,
  onSelect,
}: {
  entry: HelpEntry;
  onSelect: (entry: HelpEntry) => void;
}) {
  const clickable = !!entry.docPath;
  return (
    <button
      type="button"
      onClick={() => onSelect(entry)}
      disabled={!clickable}
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 10,
        borderRadius: 6,
        padding: "8px 10px",
        border: "none",
        background: "transparent",
        textAlign: "left",
        cursor: clickable ? "pointer" : "default",
        opacity: clickable ? 1 : 0.6,
      }}
      onMouseOver={(e) => {
        if (clickable) {
          (e.currentTarget as HTMLButtonElement).style.background = "#f1f5f9";
        }
      }}
      onMouseOut={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
      }}
    >
      <span
        style={{
          marginTop: 2,
          color: "#a78bfa",
          display: "inline-flex",
          flexShrink: 0,
        }}
      >
        {entry.icon ?? <BookIcon />}
      </span>
      <span style={{ minWidth: 0, flex: 1 }}>
        <span
          style={{
            display: "block",
            fontSize: 13,
            fontWeight: 600,
            color: "#0f172a",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {entry.title}
        </span>
        <span
          style={{
            display: "block",
            fontSize: 12,
            color: "#64748b",
            marginTop: 2,
            lineHeight: 1.4,
          }}
        >
          {entry.summary}
        </span>
      </span>
    </button>
  );
}

function SearchIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="11" cy="11" r="8" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </svg>
  );
}

function BookIcon() {
  return (
    <svg
      width="14"
      height="14"
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
