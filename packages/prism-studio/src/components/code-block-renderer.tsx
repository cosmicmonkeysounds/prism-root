/**
 * Code block renderer — static, syntax-highlight-friendly code fragment.
 *
 * Uses a dependency-free <pre><code> surface with React text children so the
 * browser handles escaping. Language label, caption, line numbers, and wrap
 * are inspector-configurable. For editable code the canonical surface is the
 * Editor Lens (CodeMirror 6) operating on the selected object; this renderer
 * is for rendered preview.
 */

import type { CSSProperties } from "react";

export interface CodeBlockProps {
  /** Raw source code. */
  source?: string | undefined;
  /** Language label shown in the header (informational only). */
  language?: string | undefined;
  /** Optional caption displayed above the code block. */
  caption?: string | undefined;
  /** When true, line numbers are rendered in a gutter column. */
  lineNumbers?: boolean | undefined;
  /** When true, long lines wrap instead of horizontally scrolling. */
  wrap?: boolean | undefined;
}

/** Normalize source into lines, preserving empty trailing line behavior. */
export function splitCodeLines(source: string): string[] {
  if (source === "") return [""];
  return source.split(/\r?\n/);
}

/** Compute the gutter width (in characters) for a given line count. */
export function gutterWidth(lineCount: number): number {
  if (lineCount < 1) return 1;
  return String(lineCount).length;
}

const container: CSSProperties = {
  margin: "0 0 8px 0",
  border: "1px solid #1f2937",
  borderRadius: 6,
  background: "#0f172a",
  color: "#e2e8f0",
  overflow: "hidden",
  fontSize: 12,
};

const headerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "6px 10px",
  borderBottom: "1px solid #1f2937",
  background: "#111827",
  fontFamily: "system-ui, sans-serif",
  fontSize: 11,
  color: "#94a3b8",
};

const preBase: CSSProperties = {
  margin: 0,
  padding: "10px 12px",
  fontFamily:
    "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Menlo, Consolas, monospace",
  fontSize: 12,
  lineHeight: 1.55,
};

export function CodeBlockRenderer(props: CodeBlockProps) {
  const {
    source = "",
    language,
    caption,
    lineNumbers = false,
    wrap = false,
  } = props;

  const lines = splitCodeLines(source);
  const width = gutterWidth(lines.length);

  const preStyle: CSSProperties = {
    ...preBase,
    whiteSpace: wrap ? "pre-wrap" : "pre",
    overflowX: wrap ? "visible" : "auto",
  };

  return (
    <div data-testid="code-block" style={container}>
      {(language || caption) && (
        <div style={headerStyle}>
          {language && (
            <span
              data-testid="code-block-lang"
              style={{
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "#a78bfa",
                fontWeight: 600,
              }}
            >
              {language}
            </span>
          )}
          {caption && (
            <span data-testid="code-block-caption" style={{ flex: 1 }}>
              {caption}
            </span>
          )}
        </div>
      )}
      <pre style={preStyle}>
        {lineNumbers ? (
          <code>
            {lines.map((line, i) => (
              <div
                key={i}
                data-testid={`code-block-line-${i}`}
                style={{ display: "flex" }}
              >
                <span
                  aria-hidden="true"
                  style={{
                    display: "inline-block",
                    width: `${width + 1}ch`,
                    paddingRight: 8,
                    textAlign: "right",
                    color: "#475569",
                    userSelect: "none",
                    flex: "0 0 auto",
                  }}
                >
                  {i + 1}
                </span>
                <span style={{ flex: 1, whiteSpace: "inherit" }}>
                  {line === "" ? "\u00a0" : line}
                </span>
              </div>
            ))}
          </code>
        ) : (
          <code>{source}</code>
        )}
      </pre>
    </div>
  );
}
