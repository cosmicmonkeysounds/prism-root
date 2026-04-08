/**
 * ReportFacet Panel — FileMaker Pro-inspired grouped/summarized data view.
 *
 * Renders objects from the kernel grouped by a configurable field,
 * with summary calculations (count, sum, average, min, max) per group
 * and a grand summary across all groups.
 */

import { useState, useCallback, useMemo } from "react";
import { useObjects, useFacetDefinitions } from "../kernel/index.js";
import type { GraphObject } from "@prism/core/object-model";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
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
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
    marginLeft: "0.375rem",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  groupHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.5rem 0.75rem",
    background: "#2a2d35",
    borderRadius: "0.25rem",
    marginBottom: "0.25rem",
    marginTop: "0.75rem",
    cursor: "pointer",
    userSelect: "none" as const,
  },
  groupTitle: {
    fontSize: "0.875rem",
    fontWeight: 600,
    color: "#e5e5e5",
  },
  groupCount: {
    fontSize: "0.6875rem",
    color: "#888",
    background: "#333",
    padding: "0.125rem 0.5rem",
    borderRadius: "0.75rem",
  },
  row: {
    display: "grid",
    gridTemplateColumns: "1fr 80px 80px 100px",
    gap: "0.5rem",
    padding: "0.375rem 0.75rem",
    borderBottom: "1px solid #2a2a2a",
    fontSize: "0.8125rem",
    alignItems: "center",
  },
  rowName: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
    color: "#e5e5e5",
  },
  rowType: {
    fontSize: "0.6875rem",
    color: "#888",
  },
  rowStatus: {
    fontSize: "0.6875rem",
    color: "#4fc1ff",
  },
  rowNumeric: {
    fontSize: "0.75rem",
    color: "#aaa",
    textAlign: "right" as const,
  },
  summaryRow: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.25rem 0",
    fontSize: "0.75rem",
  },
  summaryLabel: {
    color: "#888",
    textTransform: "uppercase" as const,
    fontSize: "0.625rem",
    letterSpacing: "0.05em",
  },
  summaryValue: {
    color: "#4fc1ff",
    fontWeight: 600,
    fontSize: "0.8125rem",
  },
  grandSummary: {
    background: "#1a2332",
    border: "1px solid #1a3350",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginTop: "1rem",
  },
  controlBar: {
    display: "flex",
    gap: 8,
    marginBottom: "0.75rem",
    alignItems: "center",
    flexWrap: "wrap" as const,
  },
  controlGroup: {
    display: "flex",
    gap: 4,
    alignItems: "center",
  },
  controlLabel: {
    fontSize: "0.625rem",
    color: "#666",
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
  },
} as const;

// ── Types ───────────────────────────────────────────────────────────────────

type SortDir = "asc" | "desc";

type SummaryOp = "count" | "sum" | "average" | "min" | "max";

interface GroupData {
  key: string;
  objects: GraphObject[];
}

// ── Groupable / sortable fields from GraphObject ────────────────────────────

const GROUPABLE_FIELDS = ["type", "status", "tags", "parentId"] as const;
type GroupableField = (typeof GROUPABLE_FIELDS)[number];

const SORTABLE_FIELDS = ["name", "type", "status", "position", "createdAt", "updatedAt"] as const;

// ── Helpers ─────────────────────────────────────────────────────────────────

function getGroupValue(obj: GraphObject, field: GroupableField): string {
  const raw = obj[field];
  if (raw === null || raw === undefined) return "(none)";
  if (Array.isArray(raw)) return raw.length > 0 ? raw.join(", ") : "(none)";
  return String(raw);
}

function getNumericValue(obj: GraphObject, field: string): number | undefined {
  if (field === "position") return obj.position;
  const dataVal = obj.data?.[field];
  if (typeof dataVal === "number") return dataVal;
  if (typeof dataVal === "string") {
    const parsed = Number(dataVal);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
}

function getSortValue(obj: GraphObject, field: string): string {
  switch (field) {
    case "name": return obj.name;
    case "type": return obj.type;
    case "status": return obj.status ?? "";
    case "position": return String(obj.position);
    case "createdAt": return obj.createdAt;
    case "updatedAt": return obj.updatedAt;
    default: return String(obj.data?.[field] ?? "");
  }
}

function collectNumericFields(objects: ReadonlyArray<GraphObject>): string[] {
  const fields = new Set<string>();
  fields.add("position");
  for (const obj of objects) {
    if (obj.data) {
      for (const [key, val] of Object.entries(obj.data)) {
        if (typeof val === "number" || (typeof val === "string" && Number.isFinite(Number(val)))) {
          fields.add(key);
        }
      }
    }
  }
  return [...fields].sort();
}

function computeSummary(
  objects: ReadonlyArray<GraphObject>,
  numericField: string,
  operation: SummaryOp,
): number | null {
  const values = objects
    .map((o) => getNumericValue(o, numericField))
    .filter((v): v is number => v !== undefined);

  if (values.length === 0) return null;

  switch (operation) {
    case "count":
      return values.length;
    case "sum":
      return values.reduce((a, b) => a + b, 0);
    case "average":
      return values.reduce((a, b) => a + b, 0) / values.length;
    case "min":
      return Math.min(...values);
    case "max":
      return Math.max(...values);
  }
}

function formatNumber(value: number | null): string {
  if (value === null) return "--";
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(2);
}

// ── Summary Section ─────────────────────────────────────────────────────────

function SummarySection({
  objects,
  numericField,
  operation,
  testIdPrefix,
}: {
  objects: ReadonlyArray<GraphObject>;
  numericField: string;
  operation: SummaryOp;
  testIdPrefix: string;
}) {
  const result = useMemo(
    () => computeSummary(objects, numericField, operation),
    [objects, numericField, operation],
  );

  return (
    <div style={styles.summaryRow} data-testid={`${testIdPrefix}-summary`}>
      <span style={styles.summaryLabel}>
        {operation}({numericField})
      </span>
      <span style={styles.summaryValue} data-testid={`${testIdPrefix}-summary-value`}>
        {formatNumber(result)}
      </span>
    </div>
  );
}

// ── Object Row ──────────────────────────────────────────────────────────────

function ObjectRow({
  obj,
  numericField,
}: {
  obj: GraphObject;
  numericField: string;
}) {
  const numVal = getNumericValue(obj, numericField);

  return (
    <div style={styles.row} data-testid={`row-${obj.id}`}>
      <span style={styles.rowName}>{obj.name}</span>
      <span style={styles.rowType}>{obj.type}</span>
      <span style={styles.rowStatus}>{obj.status ?? "--"}</span>
      <span style={styles.rowNumeric}>{numVal !== undefined ? formatNumber(numVal) : "--"}</span>
    </div>
  );
}

// ── Group Section ───────────────────────────────────────────────────────────

function GroupSection({
  group,
  index,
  numericField,
  summaryOp,
  collapsed,
  onToggle,
}: {
  group: GroupData;
  index: number;
  numericField: string;
  summaryOp: SummaryOp;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <div data-testid={`group-${index}`}>
      <div
        style={styles.groupHeader}
        onClick={onToggle}
        data-testid={`group-header-${index}`}
      >
        <span style={styles.groupTitle}>
          {collapsed ? "\u25B6" : "\u25BC"} {group.key}
        </span>
        <span style={styles.groupCount}>{group.objects.length} items</span>
      </div>

      {!collapsed && (
        <>
          {/* Column headers */}
          <div
            style={{
              ...styles.row,
              borderBottom: "2px solid #333",
              fontSize: "0.6875rem",
              color: "#888",
              fontWeight: 600,
              textTransform: "uppercase" as React.CSSProperties["textTransform"],
              letterSpacing: "0.05em",
            }}
          >
            <span>Name</span>
            <span>Type</span>
            <span>Status</span>
            <span style={{ textAlign: "right" }}>{numericField}</span>
          </div>

          {group.objects.map((obj) => (
            <ObjectRow key={obj.id as string} obj={obj} numericField={numericField} />
          ))}

          {/* Group summary */}
          <div style={{ ...styles.card, marginTop: "0.25rem", padding: "0.5rem 0.75rem" }}>
            <SummarySection
              objects={group.objects}
              numericField={numericField}
              operation={summaryOp}
              testIdPrefix={`group-${index}`}
            />
          </div>
        </>
      )}
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export default function ReportFacetPanel() {
  const objects = useObjects();
  const { definitions } = useFacetDefinitions();

  // Find a report-type facet definition if one exists
  const reportDef = useMemo(
    () => definitions.find((d) => d.layout === "report"),
    [definitions],
  );

  // State
  const [groupByField, setGroupByField] = useState<GroupableField>(
    (reportDef?.groupByField as GroupableField | undefined) ?? "type",
  );
  const [sortField, setSortField] = useState<string>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [summaryNumericField, setSummaryNumericField] = useState<string>("position");
  const [summaryOp, setSummaryOp] = useState<SummaryOp>("count");
  const [collapsedGroups, setCollapsedGroups] = useState<Set<number>>(new Set());

  // Available numeric fields across all objects
  const numericFields = useMemo(() => collectNumericFields(objects), [objects]);

  // Sort objects
  const sortedObjects = useMemo(() => {
    const sorted = [...objects];
    sorted.sort((a, b) => {
      const aVal = String(getSortValue(a, sortField));
      const bVal = String(getSortValue(b, sortField));
      const cmp = aVal.localeCompare(bVal, undefined, { numeric: true });
      return sortDir === "asc" ? cmp : -cmp;
    });
    return sorted;
  }, [objects, sortField, sortDir]);

  // Group sorted objects
  const groups = useMemo(() => {
    const map = new Map<string, GraphObject[]>();
    for (const obj of sortedObjects) {
      const key = getGroupValue(obj, groupByField);
      const list = map.get(key) ?? [];
      list.push(obj);
      map.set(key, list);
    }
    const result: GroupData[] = [];
    for (const [key, objs] of map) {
      result.push({ key, objects: objs });
    }
    result.sort((a, b) => a.key.localeCompare(b.key));
    return result;
  }, [sortedObjects, groupByField]);

  // Handlers
  const handleToggleGroup = useCallback(
    (index: number) => {
      setCollapsedGroups((prev) => {
        const next = new Set(prev);
        if (next.has(index)) {
          next.delete(index);
        } else {
          next.add(index);
        }
        return next;
      });
    },
    [],
  );

  const handleExpandAll = useCallback(() => {
    setCollapsedGroups(new Set());
  }, []);

  const handleCollapseAll = useCallback(() => {
    setCollapsedGroups(new Set(groups.map((_, i) => i)));
  }, [groups]);

  return (
    <div style={styles.container} data-testid="report-facet-panel">
      {/* Report Header */}
      <div style={styles.header as React.CSSProperties}>
        <span>Report Facet</span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <span style={styles.badge}>{groups.length} groups</span>
          <span style={styles.meta}>{objects.length} objects</span>
        </div>
      </div>

      {/* Controls */}
      <div style={styles.controlBar as React.CSSProperties}>
        {/* Group-by selector */}
        <div style={styles.controlGroup as React.CSSProperties}>
          <span style={styles.controlLabel}>Group by</span>
          <select
            style={styles.select}
            value={groupByField}
            onChange={(e) => setGroupByField(e.target.value as GroupableField)}
            data-testid="group-by-select"
          >
            {GROUPABLE_FIELDS.map((f) => (
              <option key={f} value={f}>{f}</option>
            ))}
          </select>
        </div>

        {/* Sort controls */}
        <div style={styles.controlGroup as React.CSSProperties}>
          <span style={styles.controlLabel}>Sort</span>
          <select
            style={styles.select}
            value={sortField}
            onChange={(e) => setSortField(e.target.value)}
            data-testid="sort-field-select"
          >
            {SORTABLE_FIELDS.map((f) => (
              <option key={f} value={f}>{f}</option>
            ))}
          </select>
          <button
            style={styles.btn}
            onClick={() => setSortDir((d) => (d === "asc" ? "desc" : "asc"))}
            data-testid="sort-dir-btn"
          >
            {sortDir === "asc" ? "\u25B2 Asc" : "\u25BC Desc"}
          </button>
        </div>

        {/* Summary field selector */}
        <div style={styles.controlGroup as React.CSSProperties}>
          <span style={styles.controlLabel}>Summary</span>
          <select
            style={styles.select}
            value={summaryNumericField}
            onChange={(e) => setSummaryNumericField(e.target.value)}
            data-testid="summary-field-select"
          >
            {numericFields.map((f) => (
              <option key={f} value={f}>{f}</option>
            ))}
          </select>
          <select
            style={styles.select}
            value={summaryOp}
            onChange={(e) => setSummaryOp(e.target.value as SummaryOp)}
            data-testid="summary-op-select"
          >
            <option value="count">Count</option>
            <option value="sum">Sum</option>
            <option value="average">Average</option>
            <option value="min">Min</option>
            <option value="max">Max</option>
          </select>
        </div>

        {/* Expand/Collapse */}
        <div style={styles.controlGroup as React.CSSProperties}>
          <button
            style={styles.btn}
            onClick={handleExpandAll}
            data-testid="expand-all-btn"
          >
            Expand All
          </button>
          <button
            style={styles.btn}
            onClick={handleCollapseAll}
            data-testid="collapse-all-btn"
          >
            Collapse All
          </button>
        </div>
      </div>

      {/* Report Body — Grouped Rows */}
      {groups.length === 0 ? (
        <div style={{ ...styles.card, color: "#555", fontStyle: "italic", textAlign: "center" }}>
          No objects to display.
        </div>
      ) : (
        groups.map((group, idx) => (
          <GroupSection
            key={group.key}
            group={group}
            index={idx}
            numericField={summaryNumericField}
            summaryOp={summaryOp}
            collapsed={collapsedGroups.has(idx)}
            onToggle={() => handleToggleGroup(idx)}
          />
        ))
      )}

      {/* Grand Summary (trailing) */}
      {objects.length > 0 && (
        <div style={styles.grandSummary} data-testid="grand-summary">
          <div style={{ ...styles.sectionTitle, marginTop: 0 }}>Grand Summary</div>
          <SummarySection
            objects={objects}
            numericField={summaryNumericField}
            operation="count"
            testIdPrefix="grand-count"
          />
          <SummarySection
            objects={objects}
            numericField={summaryNumericField}
            operation="sum"
            testIdPrefix="grand-sum"
          />
          <SummarySection
            objects={objects}
            numericField={summaryNumericField}
            operation="average"
            testIdPrefix="grand-average"
          />
          <SummarySection
            objects={objects}
            numericField={summaryNumericField}
            operation="min"
            testIdPrefix="grand-min"
          />
          <SummarySection
            objects={objects}
            numericField={summaryNumericField}
            operation="max"
            testIdPrefix="grand-max"
          />
        </div>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const REPORT_FACET_LENS_ID = lensId("report-facet");

export const reportFacetLensManifest: LensManifest = {

  id: REPORT_FACET_LENS_ID,
  name: "Report",
  icon: "\uD83D\uDCCB",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-report-facet", name: "Switch to Report Facet", shortcut: ["o"], section: "Navigation" }],
  },
};

export const reportFacetLensBundle: LensBundle = defineLensBundle(
  reportFacetLensManifest,
  ReportFacetPanel,
);
