/**
 * ReportSurface — grouped / summarised rendering of tabular data.
 *
 * Parses JSON or YAML document text, expects an array of records
 * (or an object with a `records` array), groups by an optional
 * `groupBy` field, and computes per-group summaries and a grand
 * total. Renders as structured HTML with print-ready CSS.
 *
 * Read-only. For editing, switch the document surface to `form` or
 * `spreadsheet` mode.
 */

import { useMemo, type CSSProperties } from "react";
import type { PrintConfig } from "@prism/core/facet";
import { triggerBrowserPrint } from "./print-renderer.js";
import { parseValues, detectFormat } from "@prism/core/facet";

// ── Props ───────────────────────────────────────────────────────────────────

export interface ReportSurfaceProps {
  /** Current text contents of the document. */
  value: string;
  /** Optional print configuration. */
  printConfig?: PrintConfig | undefined;
  /** File path — used as a hint only. */
  filePath?: string | undefined;
  /** Field name to group records by. */
  groupBy?: string | undefined;
  /** Field name to sum in summaries. */
  summaryField?: string | undefined;
  /** Document title shown above the report. */
  title?: string | undefined;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles: Record<string, CSSProperties> = {
  root: {
    padding: 24,
    height: "100%",
    overflow: "auto",
    background: "#fafafa",
    color: "#111",
    fontFamily: "system-ui, -apple-system, sans-serif",
    fontSize: 13,
  },
  toolbar: {
    display: "flex",
    justifyContent: "flex-end",
    gap: 8,
    marginBottom: 16,
  },
  button: {
    padding: "6px 12px",
    background: "#fff",
    border: "1px solid #d4d4d8",
    borderRadius: 4,
    color: "#111",
    fontSize: 12,
    cursor: "pointer",
  },
  document: {
    background: "#fff",
    border: "1px solid #e4e4e7",
    borderRadius: 4,
    padding: 32,
    maxWidth: 800,
    margin: "0 auto",
  },
  title: { fontSize: 20, fontWeight: 600, marginBottom: 16 },
  group: { marginBottom: 20 },
  groupHeader: {
    fontSize: 14,
    fontWeight: 600,
    background: "#f3f3f3",
    padding: "6px 10px",
  },
  row: {
    display: "flex",
    gap: 12,
    padding: "4px 10px",
    borderBottom: "0.5px solid #e4e4e7",
  },
  summary: {
    padding: "6px 10px",
    fontStyle: "italic",
    background: "#fafafa",
    borderTop: "0.5px solid #a1a1aa",
  },
  grandTotal: {
    marginTop: 16,
    padding: 10,
    fontWeight: 600,
    borderTop: "1px solid #000",
  },
  empty: { color: "#71717a", fontStyle: "italic", padding: 16 },
  error: { color: "#b91c1c", padding: 16 },
};

// ── Data extraction ─────────────────────────────────────────────────────────

/** Pull an array of records from parsed document values. */
export function extractRecords(value: string): Record<string, unknown>[] {
  const trimmed = value.trim();
  if (!trimmed) return [];

  // Try JSON array first — parseValues only understands key/value objects.
  if (trimmed.startsWith("[")) {
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (Array.isArray(parsed)) return parsed as Record<string, unknown>[];
    } catch {
      return [];
    }
    return [];
  }

  // Otherwise parse as key-value and look for a `records` array.
  try {
    const format = detectFormat(trimmed);
    const values = parseValues(trimmed, format);
    const records = values["records"];
    if (Array.isArray(records)) return records as Record<string, unknown>[];
  } catch {
    // fallthrough
  }
  return [];
}

// ── Grouping + aggregation ──────────────────────────────────────────────────

export interface ReportGroup {
  key: string;
  rows: Record<string, unknown>[];
  count: number;
  sum: number | null;
  min: number | null;
  max: number | null;
  avg: number | null;
}

export function groupRecords(
  records: Record<string, unknown>[],
  groupBy: string | undefined,
  summaryField: string | undefined,
): ReportGroup[] {
  const buckets = new Map<string, Record<string, unknown>[]>();

  if (!groupBy) {
    buckets.set("All Records", records);
  } else {
    for (const r of records) {
      const raw = r[groupBy];
      const key = raw == null || raw === "" ? "(none)" : String(raw);
      const bucket = buckets.get(key);
      if (bucket) bucket.push(r);
      else buckets.set(key, [r]);
    }
  }

  return Array.from(buckets.entries()).map(([key, rows]) =>
    computeGroup(key, rows, summaryField),
  );
}

function computeGroup(
  key: string,
  rows: Record<string, unknown>[],
  summaryField: string | undefined,
): ReportGroup {
  if (!summaryField) {
    return { key, rows, count: rows.length, sum: null, min: null, max: null, avg: null };
  }
  let sum = 0;
  let min = Infinity;
  let max = -Infinity;
  let n = 0;
  for (const r of rows) {
    const v = Number(r[summaryField]);
    if (!Number.isFinite(v)) continue;
    sum += v;
    if (v < min) min = v;
    if (v > max) max = v;
    n++;
  }
  return {
    key,
    rows,
    count: rows.length,
    sum: n > 0 ? sum : null,
    min: n > 0 ? min : null,
    max: n > 0 ? max : null,
    avg: n > 0 ? sum / n : null,
  };
}

// ── HTML body generation (shared with print) ────────────────────────────────

export function renderReportBody(
  records: Record<string, unknown>[],
  options: { title?: string | undefined; groupBy?: string | undefined; summaryField?: string | undefined },
): string {
  const groups = groupRecords(records, options.groupBy, options.summaryField);
  const parts: string[] = [];

  if (options.title) {
    parts.push(`<div class="report-title">${escapeHtml(options.title)}</div>`);
  }

  for (const g of groups) {
    parts.push('<div class="report-group">');
    parts.push(`<div class="report-group-header">${escapeHtml(g.key)} (${g.count})</div>`);
    for (const r of g.rows) {
      const cells = Object.entries(r)
        .map(([k, v]) => `<span><strong>${escapeHtml(k)}:</strong> ${escapeHtml(String(v ?? ""))}</span>`)
        .join(" ");
      parts.push(`<div class="report-row">${cells}</div>`);
    }
    if (options.summaryField && g.sum != null) {
      const avg = g.avg ?? 0;
      parts.push(
        `<div class="report-summary">Sum ${escapeHtml(options.summaryField)}: ${g.sum.toFixed(2)} · Avg: ${avg.toFixed(2)} · Min: ${g.min} · Max: ${g.max}</div>`,
      );
    }
    parts.push("</div>");
  }

  // Grand total
  const grandCount = records.length;
  let grandSum: number | null = null;
  if (options.summaryField) {
    grandSum = 0;
    for (const r of records) {
      const v = Number(r[options.summaryField]);
      if (Number.isFinite(v)) grandSum += v;
    }
  }
  parts.push(
    `<div class="report-grand-total">Total records: ${grandCount}${grandSum != null ? ` · Total ${escapeHtml(options.summaryField ?? "")}: ${grandSum.toFixed(2)}` : ""}</div>`,
  );

  return parts.join("\n");
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ── Component ───────────────────────────────────────────────────────────────

export function ReportSurface({
  value,
  printConfig,
  filePath: _filePath,
  groupBy,
  summaryField,
  title,
}: ReportSurfaceProps) {
  const records = useMemo(() => extractRecords(value), [value]);

  const groups = useMemo(
    () => groupRecords(records, groupBy, summaryField),
    [records, groupBy, summaryField],
  );

  const handlePrint = () => {
    const body = renderReportBody(records, { title, groupBy, summaryField });
    triggerBrowserPrint(body, printConfig ?? { pageSize: "letter" });
  };

  if (records.length === 0) {
    return (
      <div style={styles.root} data-testid="report-surface">
        <div style={styles.empty}>
          No records to display. Provide a JSON array or YAML with a <code>records</code> key.
        </div>
      </div>
    );
  }

  const grandCount = records.length;
  const grandSum = summaryField
    ? records.reduce((acc, r) => {
        const v = Number(r[summaryField]);
        return Number.isFinite(v) ? acc + v : acc;
      }, 0)
    : null;

  return (
    <div style={styles.root} data-testid="report-surface">
      <div style={styles.toolbar}>
        <button style={styles.button} onClick={handlePrint} data-testid="report-print">
          Print
        </button>
      </div>
      <div style={styles.document}>
        {title && <div style={styles.title}>{title}</div>}
        {groups.map((g) => (
          <div key={g.key} style={styles.group} data-testid={`report-group-${g.key}`}>
            <div style={styles.groupHeader}>
              {g.key} ({g.count})
            </div>
            {g.rows.map((row, i) => (
              <div key={i} style={styles.row}>
                {Object.entries(row).map(([k, v]) => (
                  <span key={k}>
                    <strong>{k}:</strong> {String(v ?? "")}
                  </span>
                ))}
              </div>
            ))}
            {summaryField && g.sum != null && (
              <div style={styles.summary}>
                Sum {summaryField}: {g.sum.toFixed(2)} · Avg: {(g.avg ?? 0).toFixed(2)} · Min: {g.min} · Max: {g.max}
              </div>
            )}
          </div>
        ))}
        <div style={styles.grandTotal}>
          Total records: {grandCount}
          {grandSum != null && ` · Total ${summaryField}: ${grandSum.toFixed(2)}`}
        </div>
      </div>
    </div>
  );
}
