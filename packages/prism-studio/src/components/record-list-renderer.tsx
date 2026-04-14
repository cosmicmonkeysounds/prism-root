/**
 * RecordListRenderer — parametric list over kernel records.
 *
 * The first dynamic-content primitive from ADR-004. Replaces a fleet of
 * hand-wired record widgets (tasks/events/notes/goals/…) with one widget
 * driven by a `ViewConfig` filter/sort/group spec and a lightweight row
 * template. The renderer is pure — it takes a pre-resolved list of
 * GraphObjects plus config and emits a list view. Kernel querying lives
 * one layer up in the Puck provider so this file stays unit-testable
 * without a kernel.
 */

import type { CSSProperties } from "react";
import type { GraphObject } from "@prism/core/object-model";
import { applyViewConfig, type ViewConfig } from "@prism/core/view";

export type TemplateFieldKind = "text" | "date" | "badge" | "status" | "tags";

/** A single rendered cell within a record row. */
export interface TemplateField {
  /** Shell field name (`name`, `status`, `tags`, `description`) or `data.*` key. */
  field: string;
  /** How to render the value. Defaults to `"text"`. */
  kind?: TemplateFieldKind;
  /** Optional prefix label shown before the value. */
  label?: string;
}

/** Lightweight row template — primary label + optional subtitle + meta chips. */
export interface RecordListTemplate {
  title: TemplateField;
  subtitle?: TemplateField;
  meta?: TemplateField[];
}

export interface RecordListRendererProps {
  /** Pre-filtered records (by record type) — provider does the kernel query. */
  objects: GraphObject[];
  /** Filter / sort / group / limit — consumed verbatim by applyViewConfig. */
  viewConfig?: ViewConfig;
  /** Row template. Defaults to `{ title: { field: "name" } }`. */
  template?: RecordListTemplate;
  /** Shown when the resolved list is empty. */
  emptyMessage?: string;
  selectedId?: string | null;
  onSelectObject?: (id: string) => void;
}

const DEFAULT_TEMPLATE: RecordListTemplate = {
  title: { field: "name" },
  subtitle: { field: "type" },
};

const SHELL_FIELDS: ReadonlySet<string> = new Set([
  "id",
  "type",
  "name",
  "parentId",
  "status",
  "description",
  "createdAt",
  "updatedAt",
  "date",
  "endDate",
  "color",
  "image",
  "pinned",
]);

/** Resolve a field on a GraphObject to a string, traversing shell + data. */
function readRecordField(obj: GraphObject, field: string): string {
  if (!field) return "";
  if (field === "tags") return obj.tags.join(", ");
  if (SHELL_FIELDS.has(field)) {
    const raw = (obj as unknown as Record<string, unknown>)[field];
    if (raw === null || raw === undefined) return "";
    return String(raw);
  }
  const raw = (obj.data as Record<string, unknown>)[field];
  if (raw === null || raw === undefined) return "";
  return String(raw);
}

function formatDate(iso: string): string {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  const d = new Date(ms);
  return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  })}`;
}

/** Resolve a template field to its user-facing text form. Pure, exported for tests. */
export function resolveTemplateField(
  obj: GraphObject,
  tf: TemplateField,
): string {
  const raw = readRecordField(obj, tf.field);
  if (!raw) return "";
  switch (tf.kind ?? "text") {
    case "date":
      return formatDate(raw);
    case "tags":
      return raw;
    case "badge":
    case "status":
    case "text":
    default:
      return raw;
  }
}

/** Apply view config to objects, reusing @prism/core/view's pipeline. Pure, exported for tests. */
export function applyRecordListView(
  objects: GraphObject[],
  viewConfig?: ViewConfig,
): GraphObject[] {
  if (!viewConfig) {
    return objects.filter((o) => !o.deletedAt);
  }
  return applyViewConfig(objects, viewConfig);
}

// ── Styling ─────────────────────────────────────────────────────────────────

const containerStyle: CSSProperties = {
  border: "1px solid #334155",
  borderRadius: 6,
  background: "#0f172a",
  color: "#e2e8f0",
  overflow: "hidden",
};

const rowStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  padding: "8px 12px",
  borderBottom: "1px solid #1e293b",
  cursor: "pointer",
};

const titleStyle: CSSProperties = {
  fontSize: 13,
  fontWeight: 500,
  color: "#e2e8f0",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

const subtitleStyle: CSSProperties = {
  fontSize: 11,
  color: "#94a3b8",
};

const metaChipStyle = (kind: TemplateFieldKind): CSSProperties => {
  const base: CSSProperties = {
    fontSize: 10,
    padding: "2px 6px",
    borderRadius: 3,
  };
  switch (kind) {
    case "status":
      return { ...base, background: "#1a4731", color: "#22c55e" };
    case "badge":
      return { ...base, background: "#1e293b", color: "#93c5fd" };
    case "date":
      return { ...base, background: "transparent", color: "#64748b", padding: 0 };
    case "tags":
      return { ...base, background: "#1e293b", color: "#c4b5fd" };
    default:
      return { ...base, background: "transparent", color: "#94a3b8", padding: 0 };
  }
};

const emptyStyle: CSSProperties = {
  padding: 24,
  color: "#94a3b8",
  fontSize: 12,
  fontStyle: "italic",
  textAlign: "center",
  border: "1px solid #334155",
  borderRadius: 6,
  background: "#0f172a",
};

export function RecordListRenderer(props: RecordListRendererProps) {
  const {
    objects,
    viewConfig,
    template = DEFAULT_TEMPLATE,
    emptyMessage = "No records to display.",
    selectedId = null,
    onSelectObject,
  } = props;

  const rows = applyRecordListView(objects, viewConfig);

  if (rows.length === 0) {
    return (
      <div data-testid="record-list-empty" style={emptyStyle}>
        {emptyMessage}
      </div>
    );
  }

  return (
    <div data-testid="record-list" style={containerStyle}>
      {rows.map((obj) => {
        const isSelected = selectedId === obj.id;
        const titleText = resolveTemplateField(obj, template.title) || obj.id;
        const subtitleText = template.subtitle
          ? resolveTemplateField(obj, template.subtitle)
          : "";
        return (
          <div
            key={obj.id}
            data-testid={`record-list-row-${obj.id}`}
            onClick={() => onSelectObject?.(obj.id)}
            style={{
              ...rowStyle,
              background: isSelected ? "#1a3350" : "transparent",
            }}
          >
            <div style={{ minWidth: 0, flex: 1 }}>
              <div style={titleStyle}>{titleText}</div>
              {subtitleText ? <div style={subtitleStyle}>{subtitleText}</div> : null}
            </div>
            {template.meta && template.meta.length > 0 ? (
              <div
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                  flexShrink: 0,
                }}
              >
                {template.meta.map((m, idx) => {
                  const value = resolveTemplateField(obj, m);
                  if (!value) return null;
                  const kind = m.kind ?? "text";
                  return (
                    <span
                      key={`${m.field}-${idx}`}
                      data-testid={`record-list-meta-${m.field}`}
                      style={metaChipStyle(kind)}
                    >
                      {m.label ? `${m.label} ` : ""}
                      {value}
                    </span>
                  );
                })}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
