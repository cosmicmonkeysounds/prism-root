/**
 * ReportWidgetRenderer — grouped/summarized data view.
 *
 * Groups objects by a field value and shows row count per group plus a
 * grand total. Each row in a group displays title + status. Pure
 * presentation; aggregation is a separate exported helper.
 */

import { useMemo } from "react";
import type { GraphObject } from "@prism/core/object-model";
import { readListField } from "./list-widget-renderer.js";

export interface ReportWidgetProps {
  objects: GraphObject[];
  groupField?: string;
  titleField?: string;
  valueField?: string | undefined;
  aggregation?: ReportAggregation;
  onSelectObject?: (id: string) => void;
}

export type ReportAggregation = "count" | "sum" | "avg" | "min" | "max";

export interface ReportGroup {
  key: string;
  objects: GraphObject[];
  aggregate: number;
}

/** Pure helper: group objects by a field value and compute an aggregate. */
export function buildReportGroups(
  objects: GraphObject[],
  groupField: string,
  aggregation: ReportAggregation = "count",
  valueField?: string | undefined,
): ReportGroup[] {
  const map = new Map<string, GraphObject[]>();
  for (const obj of objects) {
    const key = readListField(obj, groupField) || "—";
    const list = map.get(key) ?? [];
    list.push(obj);
    map.set(key, list);
  }
  const groups: ReportGroup[] = [];
  for (const [key, objs] of map) {
    groups.push({
      key,
      objects: objs,
      aggregate: computeAggregate(objs, aggregation, valueField),
    });
  }
  groups.sort((a, b) => a.key.localeCompare(b.key));
  return groups;
}

/** Compute a numeric aggregate over a set of objects. */
export function computeAggregate(
  objects: GraphObject[],
  aggregation: ReportAggregation,
  valueField?: string | undefined,
): number {
  if (aggregation === "count") return objects.length;
  if (!valueField || objects.length === 0) return 0;

  const nums: number[] = [];
  for (const obj of objects) {
    const raw = readListField(obj, valueField);
    const n = Number(raw);
    if (Number.isFinite(n)) nums.push(n);
  }
  if (nums.length === 0) return 0;

  switch (aggregation) {
    case "sum":
      return nums.reduce((a, b) => a + b, 0);
    case "avg":
      return nums.reduce((a, b) => a + b, 0) / nums.length;
    case "min":
      return Math.min(...nums);
    case "max":
      return Math.max(...nums);
    default:
      return 0;
  }
}

/** Format a number for display in the report header. */
export function formatAggregate(value: number, aggregation: ReportAggregation): string {
  if (aggregation === "count") return String(Math.round(value));
  if (!Number.isFinite(value)) return "—";
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(2);
}

export function ReportWidgetRenderer(props: ReportWidgetProps) {
  const {
    objects,
    groupField = "type",
    titleField = "name",
    valueField,
    aggregation = "count",
    onSelectObject,
  } = props;

  const groups = useMemo(
    () => buildReportGroups(objects, groupField, aggregation, valueField),
    [objects, groupField, aggregation, valueField],
  );

  const grandTotal = useMemo(
    () => computeAggregate(objects, aggregation, valueField),
    [objects, aggregation, valueField],
  );

  if (objects.length === 0) {
    return (
      <div
        data-testid="report-widget-empty"
        style={{
          padding: 24,
          color: "#94a3b8",
          fontSize: 12,
          fontStyle: "italic",
          textAlign: "center",
          border: "1px solid #334155",
          borderRadius: 6,
          background: "#0f172a",
        }}
      >
        No records to display.
      </div>
    );
  }

  return (
    <div
      data-testid="report-widget"
      style={{
        border: "1px solid #334155",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
        padding: 8,
      }}
    >
      {groups.map((group, idx) => (
        <div key={group.key} data-testid={`report-widget-group-${idx}`}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              padding: "6px 10px",
              background: "#1e293b",
              borderRadius: 4,
              marginTop: idx === 0 ? 0 : 8,
            }}
          >
            <span style={{ fontSize: 13, fontWeight: 600, color: "#e2e8f0" }}>{group.key}</span>
            <span
              data-testid={`report-widget-group-aggregate-${idx}`}
              style={{
                fontSize: 11,
                color: "#0ea5e9",
                background: "#0b1a2b",
                padding: "2px 8px",
                borderRadius: 10,
              }}
            >
              {formatAggregate(group.aggregate, aggregation)}
            </span>
          </div>
          {group.objects.map((obj) => (
            <div
              key={obj.id}
              data-testid={`report-widget-row-${obj.id}`}
              onClick={() => onSelectObject?.(obj.id)}
              style={{
                padding: "4px 10px",
                borderBottom: "1px solid #1e293b",
                fontSize: 12,
                display: "flex",
                justifyContent: "space-between",
                cursor: onSelectObject ? "pointer" : "default",
              }}
            >
              <span style={{ color: "#e2e8f0" }}>{readListField(obj, titleField) || obj.id}</span>
              <span style={{ color: "#0ea5e9", fontSize: 11 }}>{obj.status ?? "--"}</span>
            </div>
          ))}
        </div>
      ))}

      <div
        data-testid="report-widget-grand-total"
        style={{
          marginTop: 12,
          padding: "8px 10px",
          background: "#0b1a2b",
          border: "1px solid #1a3350",
          borderRadius: 4,
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
          <span style={{ color: "#94a3b8" }}>Grand Total</span>
          <span style={{ color: "#0ea5e9", fontWeight: 600 }}>
            {formatAggregate(grandTotal, aggregation)}
          </span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11, marginTop: 4 }}>
          <span style={{ color: "#94a3b8" }}>Groups</span>
          <span style={{ color: "#0ea5e9", fontWeight: 600 }}>{groups.length}</span>
        </div>
      </div>
    </div>
  );
}
