/**
 * LuaFacet Panel — Lua render script editor with live React preview.
 *
 * Users write Lua code using a `ui` builder table. The panel parses
 * `ui.xxx(...)` calls from the source and renders them as React elements.
 * Like FileMaker Pro custom functions but for building UI.
 */

import { useState, useCallback, useMemo } from "react";
import { useKernel } from "../kernel/index.js";

// ── Types ──────────────────────────────────────────────────────────────────

interface UINode {
  type: string;
  props: Record<string, string>;
  children: UINode[];
}

interface ParseResult {
  nodes: UINode[];
  error: string | null;
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
  ui.label("Hello from Lua!"),
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

// ── Lua UI Parser ──────────────────────────────────────────────────────────

/**
 * Simple parser that finds `ui.xxx(...)` patterns in Lua source and builds
 * a tree of UINode objects. Handles nested calls for container elements.
 */
function parseLuaUi(source: string): ParseResult {
  const trimmed = source.trim();
  if (trimmed.length === 0) {
    return { nodes: [], error: null };
  }

  try {
    const nodes = parseNodeList(trimmed, 0).nodes;
    return { nodes, error: null };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { nodes: [], error: message };
  }
}

interface ParseOutput {
  nodes: UINode[];
  endIndex: number;
}

function parseNodeList(source: string, startIndex: number): ParseOutput {
  const nodes: UINode[] = [];
  let i = startIndex;

  while (i < source.length) {
    // Skip whitespace, commas, comments
    i = skipWhitespaceAndComments(source, i);
    if (i >= source.length) break;

    // Check for closing brace/paren (end of container children)
    if (source[i] === "}" || source[i] === ")") {
      break;
    }

    // Look for `return` keyword — skip it
    if (source.substring(i, i + 6) === "return") {
      const afterReturn = source[i + 6];
      if (afterReturn === " " || afterReturn === "\n" || afterReturn === "\t") {
        i += 7;
        i = skipWhitespaceAndComments(source, i);
        continue;
      }
    }

    // Look for `ui.xxx(` pattern
    const callMatch = source.substring(i).match(/^ui\.(\w+)\s*\(/);
    if (callMatch) {
      const callName = callMatch[1];
      if (callName === undefined) {
        throw new Error(`Unexpected parse state at position ${i}`);
      }
      const argsStart = i + callMatch[0].length;
      const parsed = parseCall(callName, source, argsStart);
      nodes.push(parsed.node);
      i = parsed.endIndex;
      continue;
    }

    // Skip unknown characters
    i++;
  }

  return { nodes, endIndex: i };
}

function skipWhitespaceAndComments(source: string, i: number): number {
  while (i < source.length) {
    const ch = source[i];
    if (ch === " " || ch === "\t" || ch === "\n" || ch === "\r" || ch === ",") {
      i++;
      continue;
    }
    // Lua single-line comment
    if (ch === "-" && source[i + 1] === "-") {
      const lineEnd = source.indexOf("\n", i);
      if (lineEnd === -1) {
        i = source.length;
      } else {
        i = lineEnd + 1;
      }
      continue;
    }
    break;
  }
  return i;
}

interface CallOutput {
  node: UINode;
  endIndex: number;
}

function parseCall(callName: string, source: string, argsStart: number): CallOutput {
  switch (callName) {
    case "label":
    case "button":
    case "badge":
    case "input":
      return parseLeafCall(callName, source, argsStart);
    case "section":
      return parseSectionCall(source, argsStart);
    case "row":
    case "column":
      return parseContainerCall(callName, source, argsStart);
    case "spacer":
    case "divider":
      return parseVoidCall(callName, source, argsStart);
    default:
      throw new Error(`Unknown ui element: ui.${callName}`);
  }
}

function parseVoidCall(callName: string, source: string, argsStart: number): CallOutput {
  // Find closing paren
  const i = skipWhitespaceAndComments(source, argsStart);
  if (source[i] === ")") {
    return { node: { type: callName, props: {}, children: [] }, endIndex: i + 1 };
  }
  throw new Error(`Expected closing ) for ui.${callName}`);
}

function parseLeafCall(callName: string, source: string, argsStart: number): CallOutput {
  const props: Record<string, string> = {};
  let i = skipWhitespaceAndComments(source, argsStart);

  // Parse first string argument (text/placeholder)
  if (source[i] === '"' || source[i] === "'") {
    const strResult = parseString(source, i);
    if (callName === "input") {
      props["placeholder"] = strResult.value;
    } else {
      props["text"] = strResult.value;
    }
    i = skipWhitespaceAndComments(source, strResult.endIndex);
  }

  // Parse optional second argument
  if (source[i] === ",") {
    i = skipWhitespaceAndComments(source, i + 1);
    if (source[i] === '"' || source[i] === "'") {
      const strResult = parseString(source, i);
      if (callName === "badge") {
        props["color"] = strResult.value;
      } else if (callName === "input") {
        props["value"] = strResult.value;
      }
      i = skipWhitespaceAndComments(source, strResult.endIndex);
    } else {
      // Skip non-string arg (e.g. onClick function ref)
      i = skipToClosingParen(source, i);
    }
  }

  // Find closing paren
  if (source[i] === ")") {
    return { node: { type: callName, props, children: [] }, endIndex: i + 1 };
  }
  throw new Error(`Expected closing ) for ui.${callName}, got "${source[i] ?? "EOF"}" at position ${i}`);
}

function parseSectionCall(source: string, argsStart: number): CallOutput {
  const props: Record<string, string> = {};
  let i = skipWhitespaceAndComments(source, argsStart);

  // Parse title string
  if (source[i] === '"' || source[i] === "'") {
    const strResult = parseString(source, i);
    props["title"] = strResult.value;
    i = skipWhitespaceAndComments(source, strResult.endIndex);
  }

  let children: UINode[] = [];

  // Parse children in { ... }
  if (source[i] === ",") {
    i = skipWhitespaceAndComments(source, i + 1);
    if (source[i] === "{") {
      const childResult = parseNodeList(source, i + 1);
      children = childResult.nodes;
      i = skipWhitespaceAndComments(source, childResult.endIndex);
      if (source[i] === "}") {
        i++;
      }
      i = skipWhitespaceAndComments(source, i);
    }
  }

  if (source[i] === ")") {
    return { node: { type: "section", props, children }, endIndex: i + 1 };
  }
  throw new Error(`Expected closing ) for ui.section, got "${source[i] ?? "EOF"}" at position ${i}`);
}

function parseContainerCall(callName: string, source: string, argsStart: number): CallOutput {
  let i = skipWhitespaceAndComments(source, argsStart);
  let children: UINode[] = [];

  if (source[i] === "{") {
    const childResult = parseNodeList(source, i + 1);
    children = childResult.nodes;
    i = skipWhitespaceAndComments(source, childResult.endIndex);
    if (source[i] === "}") {
      i++;
    }
    i = skipWhitespaceAndComments(source, i);
  }

  if (source[i] === ")") {
    return { node: { type: callName, props: {}, children }, endIndex: i + 1 };
  }
  throw new Error(`Expected closing ) for ui.${callName}, got "${source[i] ?? "EOF"}" at position ${i}`);
}

interface StringResult {
  value: string;
  endIndex: number;
}

function parseString(source: string, startIndex: number): StringResult {
  const quote = source[startIndex];
  if (quote !== '"' && quote !== "'") {
    throw new Error(`Expected string at position ${startIndex}`);
  }
  let i = startIndex + 1;
  let value = "";
  while (i < source.length) {
    const ch = source[i];
    if (ch === "\\") {
      const next = source[i + 1];
      if (next === quote || next === "\\") {
        value += next;
        i += 2;
        continue;
      }
      if (next === "n") {
        value += "\n";
        i += 2;
        continue;
      }
      value += next ?? "";
      i += 2;
      continue;
    }
    if (ch === quote) {
      return { value, endIndex: i + 1 };
    }
    value += ch;
    i++;
  }
  throw new Error(`Unterminated string starting at position ${startIndex}`);
}

function skipToClosingParen(source: string, startIndex: number): number {
  let depth = 0;
  let i = startIndex;
  while (i < source.length) {
    const ch = source[i];
    if (ch === "(") depth++;
    if (ch === ")") {
      if (depth === 0) return i;
      depth--;
    }
    if (ch === '"' || ch === "'") {
      const strResult = parseString(source, i);
      i = strResult.endIndex;
      continue;
    }
    i++;
  }
  return i;
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

function renderUINode(node: UINode, key: number): React.ReactElement {
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

export default function LuaFacetPanel() {
  const kernel = useKernel();

  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [copied, setCopied] = useState(false);

  // Panel context info
  const viewId = kernel.atoms.getState().selectedId ?? "(none)";
  const instanceKey = `lua-facet-${viewId}`;
  const isActive = true;

  // Parse UI tree from Lua source
  const parseResult: ParseResult = useMemo(() => parseLuaUi(source), [source]);

  const handleCopy = useCallback(() => {
    void navigator.clipboard?.writeText(source);
    kernel.notifications.add({ title: "Lua code copied to clipboard", kind: "info" });
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [source, kernel]);

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
    <div style={styles.container} data-testid="lua-facet-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Lua Facet</span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <span style={styles.badge}>Lua</span>
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
          data-testid="copy-lua-btn"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>

      {/* Lua editor */}
      <div style={styles.card}>
        <div style={styles.sectionTitle}>Lua Source</div>
        <textarea
          style={{ ...styles.textarea, height: 180 } as React.CSSProperties}
          value={source}
          onChange={(e) => setSource(e.target.value)}
          spellCheck={false}
          data-testid="lua-editor"
        />
      </div>

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

export { LuaFacetPanel };
