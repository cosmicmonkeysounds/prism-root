/**
 * LuauFacet Panel — Luau render script editor with live React preview.
 *
 * Users write Luau code using a `ui` builder table. The panel parses
 * `ui.xxx(...)` calls from the source and renders them as React elements.
 * Like FileMaker Pro custom functions but for building UI.
 */

import {
  useState,
  useCallback,
  useMemo,
  useEffect,
  useRef,
  useSyncExternalStore,
} from "react";
import { useKernel, useSelection, useObject } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  createLuauDebugger,
  type DebugRunResult,
} from "@prism/core/layer1";
import {
  initLuauSyntax,
  isLuauParserReady,
  findUiCallsSync,
  type LuauUiCall,
} from "@prism/core/syntax";

// ── Types ──────────────────────────────────────────────────────────────────

export interface UINode {
  type: string;
  props: Record<string, string>;
  children: UINode[];
}

export interface ParseResult {
  nodes: UINode[];
  error: string | null;
}

// ── Luau parser readiness ──────────────────────────────────────────────────
//
// The Luau parser is backed by a WASM module that requires one-time async
// init. We kick it off at module load and expose a React hook that flips
// from `false` to `true` once the module is ready — callers re-render and
// their memoised `parseLuauUi` results pick up a real AST.

let parserReady = isLuauParserReady();
const parserReadyListeners = new Set<() => void>();

function notifyParserReady(): void {
  parserReady = true;
  for (const cb of parserReadyListeners) cb();
}

if (!parserReady) {
  void initLuauSyntax().then(notifyParserReady);
}

function subscribeParserReady(cb: () => void): () => void {
  parserReadyListeners.add(cb);
  return () => parserReadyListeners.delete(cb);
}

/**
 * Subscribe to Luau parser readiness. Returns `true` once the WASM module
 * has finished initializing. Safe to call in SSR / non-browser tests — the
 * server snapshot is always `false`.
 */
export function useLuauParserReady(): boolean {
  return useSyncExternalStore(
    subscribeParserReady,
    () => parserReady,
    () => false,
  );
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "1rem",
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  header: {
    fontSize: "1.25rem",
    fontWeight: 600,
    marginBottom: "1rem",
    color: "#e5e5e5",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  btn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnPrimary: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  textarea: {
    background: "#1a1a2e",
    border: "1px solid #333",
    borderRadius: "0.25rem",
    padding: "0.5rem",
    color: "#4fc1ff",
    fontSize: "0.75rem",
    fontFamily: "monospace",
    width: "100%",
    outline: "none",
    boxSizing: "border-box" as const,
    resize: "vertical" as const,
    lineHeight: 1.5,
    tabSize: 2,
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  error: {
    background: "#3b1010",
    border: "1px solid #7a1414",
    borderRadius: "0.25rem",
    padding: "0.5rem",
    color: "#f87171",
    fontSize: "0.75rem",
    fontFamily: "monospace",
    whiteSpace: "pre-wrap" as const,
  },
  preview: {
    background: "#1e1e1e",
    border: "1px solid #333",
    borderRadius: "0.25rem",
    padding: "0.75rem",
    minHeight: 60,
  },
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
  },
  ctxRow: {
    display: "flex",
    gap: "0.75rem",
    fontSize: "0.6875rem",
    color: "#666",
    marginBottom: "0.5rem",
  },
  ctxLabel: {
    color: "#888",
    fontWeight: 500,
  },
} as const;

// ── Sample Scripts ──────────────────────────────────────────────────────────

const SAMPLE_SCRIPTS: Record<string, string> = {
  "Hello World": `-- Hello World
return ui.column({
  ui.label("Hello from Luau!"),
  ui.spacer(),
  ui.button("Click Me"),
})`,
  Dashboard: `-- Dashboard
return ui.column({
  ui.section("Status", {
    ui.row({
      ui.badge("Online", "green"),
      ui.badge("v1.2.0", "blue"),
      ui.spacer(),
      ui.label("Last sync: 2m ago"),
    }),
  }),
  ui.divider(),
  ui.section("Controls", {
    ui.row({
      ui.input("Search...", ""),
      ui.button("Go"),
    }),
    ui.row({
      ui.button("Refresh"),
      ui.button("Export"),
      ui.button("Settings"),
    }),
  }),
})`,
  "Status Badge": `-- Status Badge
return ui.row({
  ui.badge("Active", "green"),
  ui.badge("Warning", "yellow"),
  ui.badge("Error", "red"),
  ui.badge("Info", "blue"),
  ui.badge("Default"),
})`,
};

const SAMPLE_NAMES = Object.keys(SAMPLE_SCRIPTS);

// ── Luau UI Parser ─────────────────────────────────────────────────────────

/**
 * Extract every `ui.*(...)` call from Luau source and map each one onto the
 * renderer's `UINode` shape. Backed by the full-moon AST exposed through
 * `@prism/core/syntax` — the previous hand-rolled regex parser could not
 * handle multi-line strings, nested expressions, or comments reliably.
 *
 * The underlying WASM parser requires one-time async init. Callers that
 * invoke `parseLuauUi` before init completes get `{ nodes: [], error: null }`
 * and can subscribe via `useLuauParserReady()` to re-render once it's ready.
 * Parser errors (syntactic) are surfaced on `error`; empty source is
 * reported as `{ nodes: [], error: null }` rather than an error.
 */
export function parseLuauUi(source: string): ParseResult {
  if (source.trim().length === 0) {
    return { nodes: [], error: null };
  }
  if (!isLuauParserReady()) {
    // Parser not yet initialized — the owning component should be using
    // `useLuauParserReady()` so a re-render will pick up the real AST
    // shortly. Return an empty result without an error so fallback UI
    // stays silent.
    return { nodes: [], error: null };
  }
  const raw = findUiCallsSync(source);
  return {
    nodes: raw.calls.map(uiCallToNode),
    error: raw.error,
  };
}

/**
 * Map a `LuauUiCall` from `@prism/core/syntax` to the renderer's `UINode`.
 * Positional args are unpacked onto named `props` based on the element kind,
 * matching the shape the old hand-rolled parser produced.
 */
function uiCallToNode(call: LuauUiCall): UINode {
  const props: Record<string, string> = {};
  const positional = call.args.filter((a) => a.key === undefined);

  switch (call.kind) {
    case "label":
    case "button": {
      const text = positional[0]?.value;
      if (text !== undefined) props["text"] = text;
      break;
    }
    case "badge": {
      const text = positional[0]?.value;
      const color = positional[1]?.value;
      if (text !== undefined) props["text"] = text;
      if (color !== undefined) props["color"] = color;
      break;
    }
    case "input": {
      const placeholder = positional[0]?.value;
      const value = positional[1]?.value;
      if (placeholder !== undefined) props["placeholder"] = placeholder;
      if (value !== undefined) props["value"] = value;
      break;
    }
    case "section": {
      const title = positional[0]?.value;
      if (title !== undefined) props["title"] = title;
      break;
    }
    default:
      break;
  }

  return {
    type: call.kind,
    props,
    children: call.children.map(uiCallToNode),
  };
}

// ── Badge Colors ────────────────────────────────────────────────────────────

const BADGE_COLORS: Record<string, { bg: string; fg: string }> = {
  green: { bg: "#1a4731", fg: "#22c55e" },
  red: { bg: "#3b1010", fg: "#f87171" },
  yellow: { bg: "#4a3b00", fg: "#f59e0b" },
  blue: { bg: "#0e3a5c", fg: "#38bdf8" },
  purple: { bg: "#2e1065", fg: "#a78bfa" },
  orange: { bg: "#4a2800", fg: "#fb923c" },
  default: { bg: "#333", fg: "#ccc" },
};

// ── UI Node Renderer ────────────────────────────────────────────────────────

export function renderUINode(node: UINode, key: number): React.ReactElement {
  switch (node.type) {
    case "label":
      return (
        <span key={key} data-testid={`ui-label-${key}`} style={{ color: "#ccc" }}>
          {node.props["text"] ?? ""}
        </span>
      );

    case "button":
      return (
        <button
          key={key}
          data-testid={`ui-button-${key}`}
          style={{
            padding: "4px 10px",
            fontSize: 11,
            background: "#333",
            border: "1px solid #444",
            borderRadius: 3,
            color: "#ccc",
            cursor: "pointer",
          }}
        >
          {node.props["text"] ?? ""}
        </button>
      );

    case "badge": {
      const colorName = node.props["color"] ?? "default";
      const colors = BADGE_COLORS[colorName] ?? BADGE_COLORS["default"];
      const badgeColors = colors ?? { bg: "#333", fg: "#ccc" };
      return (
        <span
          key={key}
          data-testid={`ui-badge-${key}`}
          style={{
            display: "inline-block",
            fontSize: "0.625rem",
            padding: "0.125rem 0.375rem",
            borderRadius: "0.25rem",
            background: badgeColors.bg,
            color: badgeColors.fg,
          }}
        >
          {node.props["text"] ?? ""}
        </span>
      );
    }

    case "input":
      return (
        <input
          key={key}
          data-testid={`ui-input-${key}`}
          placeholder={node.props["placeholder"] ?? ""}
          defaultValue={node.props["value"] ?? ""}
          style={{
            background: "#333",
            border: "1px solid #444",
            borderRadius: "0.25rem",
            padding: "0.25rem 0.375rem",
            color: "#e5e5e5",
            fontSize: "0.75rem",
            outline: "none",
          }}
        />
      );

    case "section":
      return (
        <div
          key={key}
          data-testid={`ui-section-${key}`}
          style={{ marginBottom: "0.5rem" }}
        >
          <div
            style={{
              fontSize: "0.75rem",
              fontWeight: 600,
              textTransform: "uppercase",
              letterSpacing: "0.05em",
              color: "#888",
              marginBottom: "0.375rem",
            }}
          >
            {node.props["title"] ?? ""}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "0.375rem" }}>
            {node.children.map((child, i) => renderUINode(child, i))}
          </div>
        </div>
      );

    case "row":
      return (
        <div
          key={key}
          data-testid={`ui-row-${key}`}
          style={{ display: "flex", gap: "0.375rem", alignItems: "center" }}
        >
          {node.children.map((child, i) => renderUINode(child, i))}
        </div>
      );

    case "column":
      return (
        <div
          key={key}
          data-testid={`ui-column-${key}`}
          style={{ display: "flex", flexDirection: "column", gap: "0.375rem" }}
        >
          {node.children.map((child, i) => renderUINode(child, i))}
        </div>
      );

    case "spacer":
      return <div key={key} data-testid={`ui-spacer-${key}`} style={{ flex: 1 }} />;

    case "divider":
      return (
        <hr
          key={key}
          data-testid={`ui-divider-${key}`}
          style={{ border: "none", borderTop: "1px solid #333", margin: "0.375rem 0" }}
        />
      );

    default:
      return (
        <span key={key} style={{ color: "#f87171", fontSize: "0.75rem" }}>
          [unknown: {node.type}]
        </span>
      );
  }
}

// ── Error Boundary ──────────────────────────────────────────────────────────

function PreviewErrorFallback({ error }: { error: string }) {
  return (
    <div style={styles.error} data-testid="preview-error">
      Parse error: {error}
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

const DEFAULT_SOURCE = SAMPLE_SCRIPTS["Hello World"] ?? "";

export default function LuauFacetPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selectedObj = useObject(selectedId);

  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [copied, setCopied] = useState(false);
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Debugger state — parallels visual-script-panel.
  const [debugResult, setDebugResult] = useState<DebugRunResult | null>(null);
  const [activeFrameIdx, setActiveFrameIdx] = useState(0);
  const [isDebugging, setIsDebugging] = useState(false);
  const [breakpointLines, setBreakpointLines] = useState<Set<number>>(() => new Set());

  // Track whether we're editing a luau-block object
  const isLuauBlock = selectedObj?.type === "luau-block";
  const objectSource = isLuauBlock
    ? ((selectedObj.data as Record<string, unknown>)["source"] as string) ?? ""
    : null;

  // When a luau-block is selected, load its source
  useEffect(() => {
    if (objectSource !== null) {
      setSource(objectSource);
    }
  }, [selectedId, objectSource]);

  // Debounced save back to kernel when editing a luau-block
  const handleSourceChange = useCallback(
    (newSource: string) => {
      setSource(newSource);
      if (isLuauBlock && selectedId) {
        if (syncTimer.current) clearTimeout(syncTimer.current);
        syncTimer.current = setTimeout(() => {
          kernel.updateObject(selectedId, {
            data: { ...((selectedObj?.data as Record<string, unknown>) ?? {}), source: newSource },
          });
        }, 400);
      }
    },
    [isLuauBlock, selectedId, selectedObj, kernel],
  );

  // Clean up timer
  useEffect(() => {
    return () => {
      if (syncTimer.current) clearTimeout(syncTimer.current);
    };
  }, []);

  // Panel context info
  const viewId = selectedId ?? "(none)";
  const instanceKey = `luau-facet-${viewId}`;
  const isActive = true;

  // Parse UI tree from Luau source. Depend on parser readiness so the preview
  // re-renders once the WASM module finishes async init.
  const parserReadyState = useLuauParserReady();
  const parseResult: ParseResult = useMemo(
    () => parseLuauUi(source),
    // `parserReadyState` is intentionally included so we re-parse once the
    // WASM loader flips from not-ready to ready.
    [source, parserReadyState],
  );

  const handleCopy = useCallback(() => {
    void navigator.clipboard?.writeText(source);
    kernel.notifications.add({ title: "Luau code copied to clipboard", kind: "info" });
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [source, kernel]);

  const handleDebug = useCallback(async () => {
    if (!source.trim()) {
      kernel.notifications.add({ title: "Nothing to debug", kind: "warning" });
      return;
    }
    setIsDebugging(true);
    try {
      const dbg = await createLuauDebugger();
      try {
        for (const line of breakpointLines) dbg.setBreakpoint(line);
        const result = await dbg.run(source);
        setDebugResult(result);
        setActiveFrameIdx(0);
        kernel.notifications.add({
          title: result.success
            ? `Debug: ${result.frames.length} frame${result.frames.length === 1 ? "" : "s"} captured`
            : `Debug error: ${result.error ?? "unknown"}`,
          kind: result.success ? "success" : "error",
        });
      } finally {
        await dbg.dispose();
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      kernel.notifications.add({ title: `Debug failed: ${message}`, kind: "error" });
    } finally {
      setIsDebugging(false);
    }
  }, [source, breakpointLines, kernel]);

  const toggleLineBreakpoint = useCallback((line: number) => {
    setBreakpointLines((prev) => {
      const next = new Set(prev);
      if (next.has(line)) next.delete(line);
      else next.add(line);
      return next;
    });
  }, []);

  const sourceLines = useMemo(() => source.split("\n"), [source]);
  const activeFrame = debugResult?.frames[activeFrameIdx];

  const handleLoadSample = useCallback(
    (name: string) => {
      const script = SAMPLE_SCRIPTS[name];
      if (script) {
        setSource(script);
        kernel.notifications.add({ title: `Loaded "${name}" sample`, kind: "info" });
      }
    },
    [kernel],
  );

  const handleSelectChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const name = e.target.value;
      if (name) {
        handleLoadSample(name);
      }
    },
    [handleLoadSample],
  );

  const nodeCount = parseResult.nodes.length;

  return (
    <div style={styles.container} data-testid="luau-facet-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Luau Facet</span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <span style={styles.badge}>Luau</span>
          <span style={styles.meta}>{nodeCount} node{nodeCount !== 1 ? "s" : ""}</span>
        </div>
      </div>

      {/* Context display */}
      <div style={styles.ctxRow as React.CSSProperties} data-testid="ctx-display">
        <span>
          <span style={styles.ctxLabel}>viewId:</span> {viewId}
        </span>
        <span>
          <span style={styles.ctxLabel}>instanceKey:</span> {instanceKey}
        </span>
        <span>
          <span style={styles.ctxLabel}>isActive:</span> {String(isActive)}
        </span>
      </div>

      {/* Controls */}
      <div style={{ display: "flex", gap: 6, marginBottom: "0.75rem", alignItems: "center" }}>
        <select
          style={styles.select}
          onChange={handleSelectChange}
          defaultValue=""
          data-testid="sample-select"
        >
          <option value="" disabled>
            Load sample...
          </option>
          {SAMPLE_NAMES.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
        <button
          style={copied ? styles.btnPrimary : styles.btn}
          onClick={handleCopy}
          data-testid="copy-luau-btn"
        >
          {copied ? "Copied" : "Copy"}
        </button>
        <button
          style={styles.btn}
          onClick={() => void handleDebug()}
          disabled={isDebugging}
          data-testid="luau-debug-btn"
          title="Step-through debug the Luau source"
        >
          {isDebugging ? "\u25B6 Running\u2026" : "\u25B6 Debug"}
        </button>
      </div>

      {/* Object binding indicator */}
      {isLuauBlock && (
        <div
          style={{
            ...styles.card,
            borderColor: "#06b6d4",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
          data-testid="luau-object-binding"
        >
          <span style={{ ...styles.badge, background: "#083344", color: "#06b6d4" }}>
            Bound
          </span>
          <span style={{ fontSize: "0.75rem", color: "#aaa" }}>
            Editing &ldquo;{selectedObj?.name}&rdquo; &mdash; changes auto-save to kernel
          </span>
        </div>
      )}

      {/* Luau editor */}
      <div style={styles.card}>
        <div style={styles.sectionTitle}>Luau Source</div>
        <textarea
          style={{ ...styles.textarea, height: 180 } as React.CSSProperties}
          value={source}
          onChange={(e) => handleSourceChange(e.target.value)}
          spellCheck={false}
          data-testid="luau-editor"
        />
        {/* Line gutter with breakpoint toggles + current-line highlight. */}
        <div
          data-testid="luau-line-gutter"
          style={{
            marginTop: 4,
            fontFamily: "monospace",
            fontSize: 11,
            display: "flex",
            flexWrap: "wrap",
            gap: 4,
          }}
        >
          {sourceLines.map((_, i) => {
            const line = i + 1;
            const hasBp = breakpointLines.has(line);
            const isCurrent = activeFrame?.line === line;
            return (
              <button
                key={line}
                data-testid={`luau-line-btn-${line}`}
                data-breakpoint={hasBp ? "true" : "false"}
                data-current={isCurrent ? "true" : "false"}
                onClick={() => toggleLineBreakpoint(line)}
                style={{
                  width: 22,
                  height: 18,
                  fontSize: 10,
                  padding: 0,
                  border: "1px solid #444",
                  borderRadius: 2,
                  background: hasBp ? "#e74c3c" : isCurrent ? "#ffcc00" : "#333",
                  color: hasBp || isCurrent ? "#000" : "#888",
                  cursor: "pointer",
                }}
                title={`Line ${line}${hasBp ? " (breakpoint)" : ""}`}
              >
                {line}
              </button>
            );
          })}
        </div>
      </div>

      {/* Debug frames panel (visible after a debug run) */}
      {debugResult && (
        <div
          data-testid="luau-debug-frames-panel"
          style={{
            ...styles.card,
            borderColor: debugResult.success ? "#2e7d32" : "#c62828",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              marginBottom: 4,
            }}
          >
            <strong style={{ color: debugResult.success ? "#6c6" : "#f88" }}>
              {debugResult.success ? "✓ Debug" : "✗ Debug"}
            </strong>
            <span style={{ color: "#888", fontSize: 11 }}>
              {debugResult.frames.length} frame
              {debugResult.frames.length === 1 ? "" : "s"}
            </span>
            {!debugResult.success && debugResult.error && (
              <span style={{ color: "#f88", fontSize: 11 }}>
                · {debugResult.error}
              </span>
            )}
            <div style={{ flex: 1 }} />
            <button
              data-testid="luau-debug-prev-frame"
              style={styles.btn}
              onClick={() => setActiveFrameIdx((i) => Math.max(0, i - 1))}
              disabled={activeFrameIdx === 0}
            >
              ◀ Prev
            </button>
            <button
              data-testid="luau-debug-next-frame"
              style={styles.btn}
              onClick={() =>
                setActiveFrameIdx((i) =>
                  Math.min(debugResult.frames.length - 1, i + 1),
                )
              }
              disabled={activeFrameIdx >= debugResult.frames.length - 1}
            >
              Next ▶
            </button>
            <button
              data-testid="luau-debug-close"
              style={styles.btn}
              onClick={() => setDebugResult(null)}
            >
              ✕
            </button>
          </div>
          <div style={{ display: "flex", gap: 8, fontSize: 11 }}>
            <div
              data-testid="luau-debug-frame-list"
              style={{
                width: 140,
                maxHeight: 140,
                overflow: "auto",
                borderRight: "1px solid #333",
                paddingRight: 6,
              }}
            >
              {debugResult.frames.map((f, i) => (
                <div
                  key={i}
                  data-testid={`luau-debug-frame-${i}`}
                  onClick={() => setActiveFrameIdx(i)}
                  style={{
                    padding: "2px 4px",
                    cursor: "pointer",
                    background: i === activeFrameIdx ? "#2a3040" : "transparent",
                    color: f.breakpoint ? "#f88" : "#ccc",
                  }}
                >
                  #{i} · line {f.line}
                  {f.breakpoint ? " ●" : ""}
                </div>
              ))}
            </div>
            <div
              data-testid="luau-debug-locals"
              style={{
                flex: 1,
                maxHeight: 140,
                overflow: "auto",
                fontFamily: "monospace",
              }}
            >
              {activeFrame === undefined ? (
                <div style={{ color: "#666" }}>no frame selected</div>
              ) : Object.keys(activeFrame.locals).length === 0 ? (
                <div style={{ color: "#666" }}>
                  (no locals at line {activeFrame.line})
                </div>
              ) : (
                Object.entries(activeFrame.locals).map(([k, v]) => (
                  <div key={k}>
                    <span style={{ color: "#9cdcfe" }}>{k}</span>
                    <span style={{ color: "#888" }}> = </span>
                    <span style={{ color: "#ce9178" }}>{v}</span>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* Preview */}
      <div style={styles.card}>
        <div style={styles.sectionTitle}>Preview</div>
        {parseResult.error ? (
          <PreviewErrorFallback error={parseResult.error} />
        ) : parseResult.nodes.length === 0 ? (
          <div
            style={{ color: "#555", fontStyle: "italic", padding: "0.5rem 0" }}
            data-testid="preview-empty"
          >
            No ui elements detected. Write ui.xxx(...) calls to see a preview.
          </div>
        ) : (
          <div style={styles.preview} data-testid="preview-pane">
            {parseResult.nodes.map((node, i) => renderUINode(node, i))}
          </div>
        )}
      </div>
    </div>
  );
}

export { LuauFacetPanel };


// ── Lens registration ──────────────────────────────────────────────────────

export const LUAU_FACET_LENS_ID = lensId("luau-facet");

export const luauFacetLensManifest: LensManifest = {

  id: LUAU_FACET_LENS_ID,
  name: "Luau Facet",
  icon: "\uD83C\uDF19",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-luau-facet", name: "Switch to Luau Facet", shortcut: ["u"], section: "Navigation" }],
  },
};

export const luauFacetLensBundle: LensBundle = defineLensBundle(
  luauFacetLensManifest,
  LuauFacetPanel,
);
