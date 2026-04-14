/**
 * Record-list provider — first consumer of PuckComponentRegistry.
 *
 * Turns the `record-list` entity type into a Puck component. The provider
 * owns kernel access (`kernel.store.allObjects()`), parses the string
 * `filterExpression` / `metaFields` props into structured config, and
 * delegates rendering to `RecordListRenderer`.
 *
 * Filter expression grammar (first pass — keep it compact and typo-friendly):
 *
 *   status eq open
 *   priority in high,urgent
 *   date gte 2026-01-01
 *   name contains urgent
 *
 * Multiple clauses joined with `;`. Unknown operators fall through to
 * `contains` so authors don't hit a dead end while typing.
 */

import type { ComponentConfig, Fields } from "@measured/puck";
import type {
  PuckComponentProvider,
  ProviderContext,
} from "@prism/core/puck";
import type {
  FilterConfig,
  FilterOp,
  SortConfig,
  ViewConfig,
} from "@prism/core/view";
import type { StudioKernel } from "../../kernel/studio-kernel.js";
import {
  RecordListRenderer,
  type RecordListTemplate,
  type TemplateField,
  type TemplateFieldKind,
} from "../../components/record-list-renderer.js";

// ── Props shape (matches the entity field ids) ────────────────────────────

interface RecordListProps {
  recordType?: string;
  titleField?: string;
  subtitleField?: string;
  metaFields?: string;
  filterExpression?: string;
  sortField?: string;
  sortDir?: "asc" | "desc";
  limit?: number;
  emptyMessage?: string;
}

// ── Parsers ───────────────────────────────────────────────────────────────

const VALID_OPS: ReadonlySet<FilterOp> = new Set<FilterOp>([
  "eq",
  "neq",
  "contains",
  "starts",
  "gt",
  "gte",
  "lt",
  "lte",
  "in",
  "nin",
  "empty",
  "notempty",
]);

const VALID_TEMPLATE_KINDS: ReadonlySet<TemplateFieldKind> = new Set<TemplateFieldKind>([
  "text",
  "date",
  "badge",
  "status",
  "tags",
]);

/**
 * Parse a compact filter expression into a `FilterConfig[]`. Exported so
 * the provider's grammar can be unit-tested without mounting React.
 *
 * Each clause is `field op value`. Clauses are separated by `;`. For
 * `in`/`nin` the value is comma-split. Empty input returns an empty
 * array.
 */
export function parseFilterExpression(input: string): FilterConfig[] {
  if (!input || !input.trim()) return [];
  const clauses = input.split(";").map((c) => c.trim()).filter(Boolean);
  const out: FilterConfig[] = [];
  for (const clause of clauses) {
    const parts = clause.split(/\s+/);
    if (parts.length < 2) continue;
    const field = parts[0] ?? "";
    const opRaw = (parts[1] ?? "").toLowerCase();
    if (!field) continue;
    const op = (VALID_OPS.has(opRaw as FilterOp) ? opRaw : "contains") as FilterOp;
    if (op === "empty" || op === "notempty") {
      out.push({ field, op });
      continue;
    }
    const valueRaw = parts.slice(2).join(" ");
    if (!valueRaw) continue;
    if (op === "in" || op === "nin") {
      out.push({
        field,
        op,
        value: valueRaw.split(",").map((v) => v.trim()).filter(Boolean),
      });
      continue;
    }
    out.push({ field, op, value: valueRaw });
  }
  return out;
}

/**
 * Parse a meta-fields string (`status:badge, date:date`) into a list of
 * `TemplateField`s. Exported for tests.
 */
export function parseMetaFields(input: string): TemplateField[] {
  if (!input || !input.trim()) return [];
  return input
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [fieldRaw, kindRaw] = entry.split(":").map((s) => s.trim());
      const field = fieldRaw ?? "";
      const kind =
        kindRaw && VALID_TEMPLATE_KINDS.has(kindRaw as TemplateFieldKind)
          ? (kindRaw as TemplateFieldKind)
          : "text";
      return { field, kind } satisfies TemplateField;
    })
    .filter((tf) => tf.field.length > 0);
}

/** Compose a RecordListTemplate from the raw props. Pure, exported. */
export function buildTemplate(props: RecordListProps): RecordListTemplate {
  const titleField = props.titleField?.trim() || "name";
  const subtitleField = props.subtitleField?.trim();
  const meta = parseMetaFields(props.metaFields ?? "");
  const template: RecordListTemplate = {
    title: { field: titleField },
  };
  if (subtitleField) template.subtitle = { field: subtitleField };
  if (meta.length > 0) template.meta = meta;
  return template;
}

/** Compose a ViewConfig from the raw props. Pure, exported. */
export function buildViewConfig(props: RecordListProps): ViewConfig {
  const filters = parseFilterExpression(props.filterExpression ?? "");
  const sorts: SortConfig[] = [];
  if (props.sortField && props.sortField.trim()) {
    sorts.push({
      field: props.sortField.trim(),
      dir: props.sortDir === "asc" ? "asc" : "desc",
    });
  }
  const config: ViewConfig = {};
  if (filters.length > 0) config.filters = filters;
  if (sorts.length > 0) config.sorts = sorts;
  if (typeof props.limit === "number" && props.limit > 0) {
    config.limit = props.limit;
  }
  return config;
}

// ── Provider ──────────────────────────────────────────────────────────────

function buildRecordListComponent(
  ctx: ProviderContext<StudioKernel>,
): ComponentConfig {
  const { kernel } = ctx;

  const fields: Fields = {
    recordType: { type: "text", label: "Record Type" } as Fields[string],
    titleField: { type: "text", label: "Title Field" } as Fields[string],
    subtitleField: { type: "text", label: "Subtitle Field" } as Fields[string],
    metaFields: { type: "textarea", label: "Meta Fields" } as Fields[string],
    filterExpression: {
      type: "textarea",
      label: "Filter Expression",
    } as Fields[string],
    sortField: { type: "text", label: "Sort Field" } as Fields[string],
    sortDir: {
      type: "select",
      label: "Sort Direction",
      options: [
        { label: "Descending", value: "desc" },
        { label: "Ascending", value: "asc" },
      ],
    } as Fields[string],
    limit: { type: "number", label: "Limit" } as Fields[string],
    emptyMessage: {
      type: "text",
      label: "Empty Message",
    } as Fields[string],
  };

  const defaultProps: RecordListProps = {
    recordType: "task",
    titleField: "name",
    subtitleField: "description",
    metaFields: "status:badge, date:date",
    filterExpression: "",
    sortField: "updatedAt",
    sortDir: "desc",
    limit: 50,
    emptyMessage: "No records to display.",
  };

  return {
    fields,
    defaultProps: defaultProps as unknown as Record<string, unknown>,
    render: (props) => {
      const p = props as RecordListProps;
      const recordType = (p.recordType ?? "").trim();
      const objects = kernel.store.allObjects().filter((o) => {
        if (o.deletedAt) return false;
        if (!recordType || recordType === "*") return true;
        return o.type === recordType;
      });
      const template = buildTemplate(p);
      const viewConfig = buildViewConfig(p);
      return (
        <div data-testid="puck-record-list" style={{ margin: "4px 0" }}>
          <RecordListRenderer
            objects={objects}
            template={template}
            viewConfig={viewConfig}
            emptyMessage={p.emptyMessage ?? "No records to display."}
          />
        </div>
      );
    },
  };
}

export const recordListProvider: PuckComponentProvider<StudioKernel> = {
  type: "record-list",
  buildConfig: buildRecordListComponent,
};
