/**
 * Import Panel — CSV / JSON bulk importer for Studio.
 *
 * Users drop a file (or paste content), pick a target object type from the
 * ObjectRegistry, map source columns to fields, then click Import. The panel
 * creates one kernel object per source row via `kernel.createObject`.
 *
 * Registered as Lens #28 via `lenses/index.tsx`.
 */

import { useCallback, useMemo, useState } from "react";
import type { ChangeEvent, DragEvent } from "react";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Helpers (exported for unit tests) ───────────────────────────────────────

export type ImportFormat = "csv" | "json";

/** Guess the format from filename and content. */
export function detectImportFormat(filename: string, content: string): ImportFormat {
  const lower = filename.toLowerCase();
  if (lower.endsWith(".csv") || lower.endsWith(".tsv")) return "csv";
  if (lower.endsWith(".json")) return "json";
  const trimmed = content.trimStart();
  if (trimmed.startsWith("[") || trimmed.startsWith("{")) return "json";
  return "csv";
}

/**
 * Minimal CSV parser supporting quoted fields with escaped quotes (""),
 * configurable delimiter (tab auto-detected). Returns a 2D array where the
 * first row is the header and subsequent rows are data.
 */
export function parseImportCsv(text: string): { header: string[]; rows: string[][] } {
  const delimiter = text.includes("\t") && !text.includes(",") ? "\t" : ",";
  const out: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let inQuotes = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (inQuotes) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        cell += ch;
      }
    } else {
      if (ch === '"' && cell === "") {
        inQuotes = true;
      } else if (ch === delimiter) {
        row.push(cell);
        cell = "";
      } else if (ch === "\n" || ch === "\r") {
        if (ch === "\r" && text[i + 1] === "\n") i++;
        row.push(cell);
        cell = "";
        if (row.some((v) => v.length > 0) || row.length > 1) out.push(row);
        row = [];
      } else {
        cell += ch;
      }
    }
  }
  if (cell.length > 0 || row.length > 0) {
    row.push(cell);
    if (row.some((v) => v.length > 0) || row.length > 1) out.push(row);
  }
  const header = out[0] ?? [];
  const rows = out.slice(1);
  return { header, rows };
}

/** Parse JSON array (or object with `records` array). Returns header + rows. */
export function parseImportJson(text: string): { header: string[]; rows: string[][] } {
  const parsed: unknown = JSON.parse(text);
  let records: unknown[] = [];
  if (Array.isArray(parsed)) {
    records = parsed;
  } else if (parsed && typeof parsed === "object" && Array.isArray((parsed as { records?: unknown[] }).records)) {
    records = (parsed as { records: unknown[] }).records;
  } else {
    throw new Error("JSON must be an array or object with a 'records' array");
  }
  const headerSet = new Set<string>();
  for (const r of records) {
    if (r && typeof r === "object") {
      for (const k of Object.keys(r)) headerSet.add(k);
    }
  }
  const header = Array.from(headerSet);
  const rows: string[][] = records.map((r) => {
    const obj = r as Record<string, unknown>;
    return header.map((h) => (obj[h] == null ? "" : String(obj[h])));
  });
  return { header, rows };
}

/**
 * Apply a column-to-field mapping to parsed rows, producing ready-to-create
 * object data blobs. Empty mapping entries are skipped.
 */
export function mapRowsToObjects(
  header: string[],
  rows: string[][],
  mapping: Record<string, string>,
): Record<string, unknown>[] {
  return rows.map((row) => {
    const data: Record<string, unknown> = {};
    header.forEach((col, i) => {
      const target = mapping[col];
      if (!target) return;
      data[target] = row[i] ?? "";
    });
    return data;
  });
}

// ── Component ───────────────────────────────────────────────────────────────

export function ImportPanel() {
  const kernel = useKernel();
  const [filename, setFilename] = useState("");
  const [rawContent, setRawContent] = useState("");
  const [format, setFormat] = useState<ImportFormat>("csv");
  const [targetType, setTargetType] = useState("");
  const [mapping, setMapping] = useState<Record<string, string>>({});
  const [parseError, setParseError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<string | null>(null);

  const entityTypes = useMemo(() => kernel.registry.allDefs(), [kernel.registry]);
  const targetDef = useMemo(
    () => entityTypes.find((d) => d.type === targetType),
    [entityTypes, targetType],
  );
  const targetFields = useMemo(
    () => (targetDef?.fields ?? []).map((f) => f.id),
    [targetDef],
  );

  const parsed = useMemo(() => {
    if (!rawContent.trim()) return null;
    try {
      setParseError(null);
      return format === "json" ? parseImportJson(rawContent) : parseImportCsv(rawContent);
    } catch (err) {
      setParseError(err instanceof Error ? err.message : String(err));
      return null;
    }
  }, [rawContent, format]);

  const header = parsed?.header ?? [];
  const rows = parsed?.rows ?? [];
  const previewRows = rows.slice(0, 10);

  const handleFile = useCallback((file: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? "");
      setRawContent(text);
      setFilename(file.name);
      setFormat(detectImportFormat(file.name, text));
      setMapping({});
      setImportResult(null);
    };
    reader.readAsText(file);
  }, []);

  const handleFileInput = (e: ChangeEvent<HTMLInputElement>): void => {
    const file = e.target.files?.[0];
    if (file) handleFile(file);
  };

  const handleDrop = (e: DragEvent): void => {
    e.preventDefault();
    const file = e.dataTransfer.files[0];
    if (file) handleFile(file);
  };

  const handleDragOver = (e: DragEvent): void => {
    e.preventDefault();
  };

  const handleImport = (): void => {
    if (!targetDef || !parsed) return;
    const dataBlobs = mapRowsToObjects(header, rows, mapping);
    let created = 0;
    for (const data of dataBlobs) {
      const name = String(data["name"] ?? data["title"] ?? `Imported ${created + 1}`);
      kernel.createObject({
        type: targetDef.type,
        name,
        parentId: null,
        position: created,
        data,
      });
      created++;
    }
    setImportResult(`Imported ${created} ${targetDef.pluralLabel}`);
    kernel.notifications.add({
      title: "Import complete",
      body: `Created ${created} ${targetDef.pluralLabel}`,
      kind: "success",
    });
  };

  return (
    <div data-testid="import-panel" style={styles.container}>
      <h2 style={styles.title}>Import Data</h2>

      <div
        data-testid="import-dropzone"
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        style={styles.dropzone}
      >
        <p>Drop a CSV, TSV, or JSON file here</p>
        <label style={styles.fileLabel}>
          or browse
          <input
            type="file"
            accept=".csv,.tsv,.json,.txt"
            onChange={handleFileInput}
            style={{ display: "none" }}
            data-testid="import-file-input"
          />
        </label>
        {filename ? <div style={styles.filename}>{filename}</div> : null}
      </div>

      {parseError ? <div style={styles.error}>Parse error: {parseError}</div> : null}

      {parsed && header.length > 0 ? (
        <>
          <section style={styles.section}>
            <label style={styles.label}>
              Target Type
              <select
                data-testid="import-target-type"
                value={targetType}
                onChange={(e) => {
                  setTargetType(e.target.value);
                  setMapping({});
                }}
                style={styles.select}
              >
                <option value="">— Select —</option>
                {entityTypes.map((d) => (
                  <option key={d.type} value={d.type}>
                    {d.label}
                  </option>
                ))}
              </select>
            </label>
          </section>

          {targetDef ? (
            <section style={styles.section}>
              <h3 style={styles.subtitle}>Column Mapping</h3>
              <table style={styles.table}>
                <thead>
                  <tr>
                    <th style={styles.th}>Source Column</th>
                    <th style={styles.th}>→</th>
                    <th style={styles.th}>Target Field</th>
                  </tr>
                </thead>
                <tbody>
                  {header.map((col) => (
                    <tr key={col}>
                      <td style={styles.td}>{col}</td>
                      <td style={styles.td}>→</td>
                      <td style={styles.td}>
                        <select
                          data-testid={`import-mapping-${col}`}
                          value={mapping[col] ?? ""}
                          onChange={(e) => setMapping({ ...mapping, [col]: e.target.value })}
                          style={styles.select}
                        >
                          <option value="">(skip)</option>
                          {targetFields.map((f) => (
                            <option key={f} value={f}>
                              {f}
                            </option>
                          ))}
                        </select>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          ) : null}

          <section style={styles.section}>
            <h3 style={styles.subtitle}>Preview ({rows.length} rows)</h3>
            <div style={{ overflowX: "auto" }}>
              <table style={styles.table}>
                <thead>
                  <tr>
                    {header.map((h) => (
                      <th key={h} style={styles.th}>
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {previewRows.map((row, i) => (
                    <tr key={i}>
                      {row.map((cell, j) => (
                        <td key={j} style={styles.td}>
                          {cell}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <button
            type="button"
            data-testid="import-run"
            onClick={handleImport}
            disabled={!targetDef || rows.length === 0}
            style={styles.button}
          >
            Import {rows.length} Row{rows.length === 1 ? "" : "s"}
          </button>

          {importResult ? (
            <div data-testid="import-result" style={styles.success}>
              {importResult}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: 16,
    color: "#e2e8f0",
    background: "#0f172a",
    height: "100%",
    overflowY: "auto" as const,
  },
  title: {
    fontSize: 18,
    fontWeight: 600,
    marginTop: 0,
    marginBottom: 12,
  },
  subtitle: {
    fontSize: 13,
    fontWeight: 600,
    margin: "12px 0 6px 0",
    color: "#94a3b8",
  },
  dropzone: {
    border: "2px dashed #334155",
    borderRadius: 6,
    padding: 24,
    textAlign: "center" as const,
    background: "#1e293b",
    marginBottom: 12,
  },
  fileLabel: {
    display: "inline-block",
    padding: "6px 12px",
    background: "#3b82f6",
    color: "#fff",
    borderRadius: 4,
    cursor: "pointer",
    fontSize: 12,
    marginTop: 8,
  },
  filename: {
    marginTop: 8,
    fontSize: 12,
    color: "#94a3b8",
  },
  section: {
    marginBottom: 12,
  },
  label: {
    display: "flex",
    flexDirection: "column" as const,
    gap: 4,
    fontSize: 12,
  },
  select: {
    background: "#1e293b",
    border: "1px solid #334155",
    color: "#e2e8f0",
    padding: "4px 8px",
    borderRadius: 4,
    fontSize: 12,
  },
  table: {
    width: "100%",
    borderCollapse: "collapse" as const,
    fontSize: 12,
  },
  th: {
    textAlign: "left" as const,
    padding: "4px 8px",
    borderBottom: "1px solid #334155",
    color: "#94a3b8",
    fontWeight: 600 as const,
  },
  td: {
    padding: "4px 8px",
    borderBottom: "1px solid #1e293b",
  },
  button: {
    background: "#3b82f6",
    color: "#fff",
    border: "none",
    borderRadius: 4,
    padding: "8px 16px",
    fontSize: 13,
    fontWeight: 600 as const,
    cursor: "pointer",
  },
  error: {
    background: "#7f1d1d",
    color: "#fecaca",
    padding: 8,
    borderRadius: 4,
    marginBottom: 12,
    fontSize: 12,
  },
  success: {
    background: "#14532d",
    color: "#bbf7d0",
    padding: 8,
    borderRadius: 4,
    marginTop: 12,
    fontSize: 12,
  },
};


// ── Lens registration ──────────────────────────────────────────────────────

export const IMPORT_LENS_ID = lensId("import");

export const importLensManifest: LensManifest = {

  id: IMPORT_LENS_ID,
  name: "Import",
  icon: "\u{1F4E5}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-import", name: "Switch to Import", shortcut: ["shift+y"], section: "Navigation" }],
  },
};

export const importLensBundle: LensBundle = defineLensBundle(
  importLensManifest,
  ImportPanel,
);
