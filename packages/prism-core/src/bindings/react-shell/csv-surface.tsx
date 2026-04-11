/**
 * CsvSurface — grid editor for CSV/TSV text.
 *
 * Parses comma- or tab-delimited text into rows/cells, renders a
 * contentEditable <table>, and serialises edits back to CSV on cell
 * blur. Supports quoted fields (double-quoted, "" escape).
 *
 * The first row is treated as the header row (bolded). Toolbar
 * actions: add row, add column, delete row, delete column.
 */

import { useCallback, useMemo, useState, type CSSProperties } from "react";

// ── Props ───────────────────────────────────────────────────────────────────

export interface CsvSurfaceProps {
  /** Current CSV/TSV source text. */
  value: string;
  /** Called with new serialised text when the grid changes. */
  onChange?: ((value: string) => void) | undefined;
  /** Disable editing. */
  readOnly?: boolean | undefined;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles: Record<string, CSSProperties> = {
  root: {
    padding: 16,
    height: "100%",
    overflow: "auto",
    background: "#0b0b0e",
    color: "#e5e7eb",
    fontFamily: "system-ui, sans-serif",
    fontSize: 13,
  },
  toolbar: {
    display: "flex",
    gap: 8,
    marginBottom: 12,
  },
  button: {
    padding: "6px 10px",
    background: "#18181b",
    border: "1px solid #2a2a30",
    borderRadius: 4,
    color: "#e5e7eb",
    fontSize: 12,
    cursor: "pointer",
  },
  table: {
    borderCollapse: "collapse",
    width: "100%",
    fontSize: 13,
  },
  headerCell: {
    padding: "6px 10px",
    border: "1px solid #2a2a30",
    background: "#18181b",
    color: "#e5e7eb",
    fontWeight: 600,
    textAlign: "left",
    minWidth: 80,
  },
  cell: {
    padding: "6px 10px",
    border: "1px solid #2a2a30",
    background: "#0f0f12",
    color: "#e5e7eb",
    minWidth: 80,
    outline: "none",
  },
  empty: { color: "#71717a", fontStyle: "italic", padding: 16 },
};

// ── Parse / serialize ───────────────────────────────────────────────────────

/** Detect delimiter — tab beats comma if any tab exists in the first line. */
export function detectDelimiter(value: string): "," | "\t" {
  const firstLine = value.split(/\r?\n/, 1)[0] ?? "";
  return firstLine.includes("\t") ? "\t" : ",";
}

/**
 * Parse CSV/TSV text into a 2D array of rows × cells.
 * Supports quoted fields with "" escapes.
 */
export function parseCsv(value: string, delimiter: "," | "\t" = ","): string[][] {
  if (!value) return [];
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let i = 0;
  let inQuotes = false;

  while (i < value.length) {
    const ch = value[i] as string;

    if (inQuotes) {
      if (ch === '"') {
        if (value[i + 1] === '"') {
          cell += '"';
          i += 2;
          continue;
        }
        inQuotes = false;
        i++;
        continue;
      }
      cell += ch;
      i++;
      continue;
    }

    if (ch === '"') {
      inQuotes = true;
      i++;
      continue;
    }

    if (ch === delimiter) {
      row.push(cell);
      cell = "";
      i++;
      continue;
    }

    if (ch === "\r") { i++; continue; }

    if (ch === "\n") {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
      i++;
      continue;
    }

    cell += ch;
    i++;
  }

  // Flush trailing cell/row
  if (cell.length > 0 || row.length > 0) {
    row.push(cell);
    rows.push(row);
  }

  return rows;
}

/** Quote a cell if it contains delimiter, quote, or newline. */
function escapeCell(cell: string, delimiter: "," | "\t"): string {
  if (cell.includes('"') || cell.includes(delimiter) || cell.includes("\n")) {
    return '"' + cell.replace(/"/g, '""') + '"';
  }
  return cell;
}

/** Serialise a 2D array back to CSV/TSV text. */
export function serializeCsv(rows: string[][], delimiter: "," | "\t" = ","): string {
  return rows
    .map((row) => row.map((c) => escapeCell(c, delimiter)).join(delimiter))
    .join("\n");
}

/** Normalize ragged rows — pad short rows with empty cells. */
function normalizeRows(rows: string[][]): string[][] {
  if (rows.length === 0) return rows;
  const width = Math.max(...rows.map((r) => r.length));
  return rows.map((r) => {
    if (r.length === width) return r;
    const padded = [...r];
    while (padded.length < width) padded.push("");
    return padded;
  });
}

// ── Component ───────────────────────────────────────────────────────────────

export function CsvSurface({ value, onChange, readOnly = false }: CsvSurfaceProps) {
  const delimiter = useMemo(() => detectDelimiter(value), [value]);

  const rows = useMemo(() => normalizeRows(parseCsv(value, delimiter)), [value, delimiter]);

  // Local state mirrors rows so cell edits feel immediate.
  const [localRows, setLocalRows] = useState<string[][]>(rows);

  // Re-sync when the incoming value changes.
  useMemo(() => {
    setLocalRows(rows);
  }, [rows]);

  const commit = useCallback(
    (next: string[][]) => {
      setLocalRows(next);
      if (onChange && !readOnly) onChange(serializeCsv(next, delimiter));
    },
    [onChange, readOnly, delimiter],
  );

  const handleCellBlur = useCallback(
    (rowIdx: number, colIdx: number, text: string) => {
      if (readOnly) return;
      const current = localRows[rowIdx]?.[colIdx];
      if (current === text) return;
      const next = localRows.map((r, ri) =>
        ri === rowIdx ? r.map((c, ci) => (ci === colIdx ? text : c)) : r,
      );
      commit(next);
    },
    [localRows, commit, readOnly],
  );

  const addRow = useCallback(() => {
    if (readOnly) return;
    const width = localRows[0]?.length ?? 1;
    const newRow = Array(width).fill("") as string[];
    commit([...localRows, newRow]);
  }, [localRows, commit, readOnly]);

  const addColumn = useCallback(() => {
    if (readOnly) return;
    const next = localRows.map((r, i) => [...r, i === 0 ? "Column" : ""]);
    commit(next);
  }, [localRows, commit, readOnly]);

  const deleteRow = useCallback(
    (rowIdx: number) => {
      if (readOnly || rowIdx <= 0) return; // don't delete header
      commit(localRows.filter((_, i) => i !== rowIdx));
    },
    [localRows, commit, readOnly],
  );

  const deleteColumn = useCallback(
    (colIdx: number) => {
      if (readOnly) return;
      commit(localRows.map((r) => r.filter((_, i) => i !== colIdx)));
    },
    [localRows, commit, readOnly],
  );

  if (localRows.length === 0) {
    return (
      <div style={styles.root} data-testid="csv-surface">
        <div style={styles.toolbar}>
          <button style={styles.button} onClick={() => commit([["Column"], [""]])} disabled={readOnly}>
            Start Table
          </button>
        </div>
        <div style={styles.empty}>Empty document. Click Start Table to begin.</div>
      </div>
    );
  }

  const [header, ...body] = localRows;

  return (
    <div style={styles.root} data-testid="csv-surface">
      <div style={styles.toolbar}>
        <button style={styles.button} onClick={addRow} disabled={readOnly} data-testid="csv-add-row">
          + Row
        </button>
        <button style={styles.button} onClick={addColumn} disabled={readOnly} data-testid="csv-add-column">
          + Column
        </button>
      </div>
      <table style={styles.table}>
        <thead>
          <tr>
            {header?.map((cell, ci) => (
              <th
                key={ci}
                style={styles.headerCell}
                contentEditable={!readOnly}
                suppressContentEditableWarning
                onBlur={(e) => handleCellBlur(0, ci, e.currentTarget.textContent ?? "")}
                data-testid={`csv-cell-0-${ci}`}
              >
                {cell}
              </th>
            ))}
            {!readOnly && header && header.length > 0 && (
              <th style={{ ...styles.headerCell, minWidth: 40 }}>
                <button
                  style={styles.button}
                  onClick={() => deleteColumn(header.length - 1)}
                  data-testid="csv-delete-column"
                  title="Delete last column"
                >
                  −
                </button>
              </th>
            )}
          </tr>
        </thead>
        <tbody>
          {body.map((row, ri) => {
            const rowIdx = ri + 1;
            return (
              <tr key={rowIdx}>
                {row.map((cell, ci) => (
                  <td
                    key={ci}
                    style={styles.cell}
                    contentEditable={!readOnly}
                    suppressContentEditableWarning
                    onBlur={(e) => handleCellBlur(rowIdx, ci, e.currentTarget.textContent ?? "")}
                    data-testid={`csv-cell-${rowIdx}-${ci}`}
                  >
                    {cell}
                  </td>
                ))}
                {!readOnly && (
                  <td style={{ ...styles.cell, minWidth: 40, textAlign: "center" }}>
                    <button
                      style={styles.button}
                      onClick={() => deleteRow(rowIdx)}
                      data-testid={`csv-delete-row-${rowIdx}`}
                      title="Delete row"
                    >
                      −
                    </button>
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
