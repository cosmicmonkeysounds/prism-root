/**
 * Studio entity → Puck `ComponentConfig` builder.
 *
 * A single pass over `kernel.registry.allDefs()` produces the full
 * `Record<string, ComponentConfig>` map that Studio's panels (layout,
 * canvas, …) feed into Puck. The function is invoked once at kernel
 * boot from `studio-kernel.ts`; each entry is then `registerDirect`'d
 * onto `kernel.puckComponents`, which is the single source of truth
 * for Puck components (bundle-driven lens/shell-widget entries or
 * studio-driven entries from this file).
 *
 * Most widgets live in the `WIDGET_TABLE` as declarative `widget()`
 * calls built on top of `./widget-builder.tsx`. Anything that needs to
 * transform an `EntityDef`'s field list first (heading, text-block,
 * button, card, hero/app-shell/etc.) flows through
 * `entityToPuckComponent` and is handled in a small set of explicit
 * branches. Universal style fields (className/padding/font/…) are
 * merged into every component at the end by `attachStyleFieldsInPlace`.
 */

import { lazy, Suspense, type ReactNode } from "react";
import type { ComponentConfig, Fields } from "@measured/puck";
import type { FacetLayout } from "@prism/core/facet";
import {
  computeBlockStyle,
  extractBlockStyle,
  STYLE_FIELD_DEFS,
  type BlockStyleData,
} from "@prism/core/page-builder";
import type { EntityDef } from "@prism/core/object-model";
import { parseLuauUi, renderUINode, useLuauParserReady } from "../panels/luau-facet-panel.js";
import {
  getShellSlots,
  isShellType,
  kebabToPascal,
  PAGE_SLOTS,
} from "../panels/layout-panel-data.js";
import type { ObjectId } from "@prism/core/object-model";
import { FacetViewRenderer } from "../components/facet-view-renderer.js";
import { SpatialCanvasRenderer } from "../components/spatial-canvas-renderer.js";
import { DataPortalRenderer } from "../components/data-portal-renderer.js";
import { KanbanWidgetRenderer } from "../components/kanban-widget-renderer.js";
import { ListWidgetRenderer } from "../components/list-widget-renderer.js";
import {
  TableWidgetRenderer,
  parseTableColumns,
  type TableSortDir,
} from "../components/table-widget-renderer.js";
import { CardGridWidgetRenderer } from "../components/card-grid-widget-renderer.js";
import {
  ReportWidgetRenderer,
  type ReportAggregation,
} from "../components/report-widget-renderer.js";
import { CalendarWidgetRenderer } from "../components/calendar-widget-renderer.js";
import {
  ChartWidgetRenderer,
  type ChartType,
  type ChartAggregation,
} from "../components/chart-widget-renderer.js";
// `MapWidgetRenderer` pulls in `leaflet` which touches `window` at module
// scope, breaking node-environment vitest runs that transitively import
// `entity-puck-config`. Lazy import keeps the map library out of the boot
// graph — it's only fetched when the map widget actually renders in a
// browser. The named-export → default-export shim satisfies `React.lazy`'s
// default-export requirement.
const MapWidgetRenderer = lazy(() =>
  import("../components/map-widget-renderer.js").then((mod) => ({
    default: mod.MapWidgetRenderer,
  })),
);
import {
  TasksWidgetRenderer,
  RemindersWidgetRenderer,
  ContactsWidgetRenderer,
  EventsWidgetRenderer,
  NotesWidgetRenderer,
  GoalsWidgetRenderer,
  HabitTrackerWidgetRenderer,
  BookmarksWidgetRenderer,
  TimerWidgetRenderer,
  CaptureInboxWidgetRenderer,
  type TaskFilter,
  type ReminderFilter,
  type ContactFilter,
  type EventRange,
  type NoteFilter,
  type GoalFilter,
} from "../components/dynamic-widget-renderers.js";
import { TabContainerRenderer } from "../components/tab-container-renderer.js";
import { PopoverWidgetRenderer } from "../components/popover-widget-renderer.js";
import { SlidePanelRenderer } from "../components/slide-panel-renderer.js";
import {
  TextInputRenderer,
  TextareaInputRenderer,
  SelectInputRenderer,
  CheckboxInputRenderer,
  NumberInputRenderer,
  DateInputRenderer,
} from "../components/form-input-renderers.js";
import {
  ColumnsRenderer,
  DividerRenderer,
  SpacerRenderer,
} from "../components/layout-primitive-renderers.js";
import {
  StatWidgetRenderer,
  BadgeRenderer,
  AlertRenderer,
  ProgressBarRenderer,
  type StatAggregation,
  type BadgeTone,
} from "../components/data-display-renderers.js";
import {
  MarkdownWidgetRenderer,
  IframeWidgetRenderer,
  renderMarkdown,
} from "../components/content-renderers.js";
import { CodeBlockRenderer } from "../components/code-block-renderer.js";
import {
  ButtonRenderer,
  type ButtonVariant,
  type ButtonSize,
  type ButtonRounded,
  type ButtonShadow,
  type ButtonHoverEffect,
  type ButtonIconPosition,
  type ButtonTarget,
  type ButtonType as ButtonHtmlType,
} from "../components/button-renderer.js";
import {
  CardRenderer,
  type CardVariant,
  type CardLayout,
  type CardHoverEffect,
  type CardMediaFit,
} from "../components/card-renderer.js";
import {
  VideoWidgetRenderer,
  AudioWidgetRenderer,
} from "../components/media-renderers.js";
import {
  PageShellRenderer,
  AppShellRenderer,
  SiteHeaderRenderer,
  SiteFooterRenderer,
  SideBarRenderer,
  NavBarRenderer,
  HeroRenderer,
} from "../components/layout-shell-renderers.js";
import {
  colorField,
  alignField,
  sliderField,
  urlField,
  classNameField,
  customCssField,
  fontPickerField,
} from "../components/puck-custom-fields.js";
import { mediaUploadField } from "../components/vfs-media-field.js";
import { facetPickerField } from "../components/facet-picker-field.js";
import { useResolvedMediaUrl } from "../components/vfs-media-url.js";
import {
  RecordListRenderer,
  type RecordListTemplate,
  type TemplateField,
  type TemplateFieldKind,
} from "../components/record-list-renderer.js";
import type {
  FilterConfig,
  FilterOp,
  SortConfig,
  ViewConfig,
} from "@prism/core/view";
import type { StudioKernel } from "./studio-kernel.js";
import { f, asBool, widget } from "./widget-builder.js";

// ── Generic entity → Puck mapper ───────────────────────────────────────────

const ALIGN_FIELD_IDS = new Set(["align", "textAlign", "mobileTextAlign"]);
const SPACING_FIELD_PATTERN =
  /^(paddingX|paddingY|marginX|marginY|mobilePaddingX|mobilePaddingY|borderWidth|borderRadius|fontSize|mobileFontSize|letterSpacing|gap)$/;

interface MappableField {
  id: string;
  type: string;
  label?: string;
  required?: boolean;
  default?: unknown;
  enumOptions?: ReadonlyArray<{ value: string; label: string }>;
  ui?: { multiline?: boolean; placeholder?: string };
}

interface MappableDef {
  type: string;
  label: string;
  icon?: string;
  fields: ReadonlyArray<MappableField>;
}

/**
 * Convert an `EntityDef`'s field list into a Puck `ComponentConfig`.
 *
 * Special-case IDs (`className`/`customCss`/`fontFamily`) use curated
 * custom fields; numeric spacing fields become sliders; enum fields with
 * align semantics use the `alignField` toggle. The default render returns
 * a debug preview card — callers that want a real renderer overwrite
 * `render` after calling this function.
 */
function entityToPuckComponent(def: MappableDef): ComponentConfig {
  const puckFields: Record<string, unknown> = {};
  const defaultProps: Record<string, unknown> = {};
  const withLabel = (label: string | undefined) =>
    label !== undefined ? { label } : {};

  for (const field of def.fields) {
    const lbl = withLabel(field.label);
    if (field.id === "className") {
      puckFields[field.id] = classNameField(lbl);
      if (field.default !== undefined) defaultProps[field.id] = field.default;
      continue;
    }
    if (field.id === "customCss") {
      puckFields[field.id] = customCssField(lbl);
      if (field.default !== undefined) defaultProps[field.id] = field.default;
      continue;
    }
    if (field.id === "fontFamily") {
      puckFields[field.id] = fontPickerField(lbl);
      if (field.default !== undefined) defaultProps[field.id] = field.default;
      continue;
    }
    switch (field.type) {
      case "string":
        puckFields[field.id] = { type: "text", ...lbl };
        if (field.default !== undefined) defaultProps[field.id] = field.default;
        break;
      case "url":
        puckFields[field.id] = urlField(lbl);
        if (field.default !== undefined) defaultProps[field.id] = field.default;
        break;
      case "color":
        puckFields[field.id] = colorField(lbl);
        if (field.default !== undefined) defaultProps[field.id] = field.default;
        break;
      case "text":
        puckFields[field.id] = { type: "textarea", ...lbl };
        if (field.default !== undefined) defaultProps[field.id] = field.default;
        break;
      case "bool":
        puckFields[field.id] = {
          type: "radio",
          ...lbl,
          options: [
            { label: "Yes", value: "true" },
            { label: "No", value: "false" },
          ],
        };
        defaultProps[field.id] =
          field.default !== undefined ? String(field.default) : "false";
        break;
      case "int":
      case "float":
        if (field.type === "int" && SPACING_FIELD_PATTERN.test(field.id)) {
          const max = /fontSize/i.test(field.id)
            ? 96
            : /radius|letterSpacing|borderWidth/i.test(field.id)
              ? 64
              : 128;
          puckFields[field.id] = sliderField({
            ...lbl,
            min: 0,
            max,
            step: 1,
            unit: "px",
          });
        } else {
          puckFields[field.id] = { type: "number", ...lbl };
        }
        if (field.default !== undefined) defaultProps[field.id] = field.default;
        break;
      case "enum":
        if (field.enumOptions && field.enumOptions.length > 0) {
          if (ALIGN_FIELD_IDS.has(field.id)) {
            puckFields[field.id] = alignField({
              ...lbl,
              options: field.enumOptions.map((o) => ({
                value: o.value,
                label: o.label,
              })),
            });
          } else {
            puckFields[field.id] = {
              type: "select",
              ...lbl,
              options: field.enumOptions.map((o) => ({
                label: o.label,
                value: o.value,
              })),
            };
          }
          defaultProps[field.id] = field.default ?? field.enumOptions[0]?.value;
        } else {
          puckFields[field.id] = { type: "text", ...lbl };
        }
        break;
      default:
        puckFields[field.id] = { type: "text", ...lbl };
        if (field.default !== undefined) defaultProps[field.id] = field.default;
    }
  }

  return {
    fields: puckFields as Fields,
    defaultProps,
    render: (props) => {
      const p = props as Record<string, unknown>;
      const style = computeBlockStyle(extractBlockStyle(p) as BlockStyleData);
      const className =
        typeof p["className"] === "string" ? (p["className"] as string) : undefined;
      const alignOverride =
        typeof p["align"] === "string" && p["align"] !== ""
          ? (p["align"] as string)
          : undefined;
      if (alignOverride) style.textAlign = alignOverride;

      if (def.type === "heading") {
        const level = String(p["level"] ?? "h2") as "h1" | "h2" | "h3" | "h4";
        const text = (p["text"] as string) ?? "";
        const Tag = level;
        return (
          <Tag style={style} {...(className ? { className } : {})}>
            {text || "Heading"}
          </Tag>
        );
      }

      if (def.type === "text-block") {
        const content = (p["content"] as string) ?? "";
        const format = (p["format"] as string) ?? "markdown";
        if (format === "markdown") {
          return (
            <div
              style={style}
              {...(className ? { className } : {})}
              dangerouslySetInnerHTML={{ __html: renderMarkdown(content) }}
            />
          );
        }
        return (
          <div
            style={{ whiteSpace: "pre-wrap", ...style }}
            {...(className ? { className } : {})}
          >
            {content}
          </div>
        );
      }

      const previewFields = def.fields
        .filter((field) => !isBlockStyleFieldId(field.id))
        .slice(0, 3);
      const wrapperClass = className ?? "";
      const hasWrapperClass = wrapperClass.trim().length > 0;
      return (
        <div
          {...(hasWrapperClass ? { className: wrapperClass } : {})}
          style={
            hasWrapperClass
              ? style
              : {
                  border: "1px dashed #cbd5e1",
                  borderRadius: 6,
                  padding: 12,
                  margin: "4px 0",
                  background: "#fafafa",
                  ...style,
                }
          }
        >
          {!hasWrapperClass ? (
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "#64748b",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                marginBottom: 6,
              }}
            >
              {def.icon ?? ""} {def.label}
            </div>
          ) : null}
          {previewFields.map((field) => {
            const val = p[field.id];
            if (val === undefined || val === null || val === "") return null;
            return (
              <div key={field.id} style={{ fontSize: 13, marginBottom: 2 }}>
                <span style={{ color: "#94a3b8", marginRight: 6 }}>
                  {field.label ?? field.id}:
                </span>
                {String(val)}
              </div>
            );
          })}
        </div>
      );
    },
  };
}

const BLOCK_STYLE_FIELD_IDS = new Set<string>([
  "className",
  "customCss",
  "background",
  "textColor",
  "paddingX",
  "paddingY",
  "marginX",
  "marginY",
  "borderWidth",
  "borderColor",
  "borderRadius",
  "shadow",
  "fontFamily",
  "fontSize",
  "fontWeight",
  "lineHeight",
  "letterSpacing",
  "textAlign",
  "display",
  "flexDirection",
  "gap",
  "alignItems",
  "justifyContent",
  "position",
  "top",
  "left",
  "right",
  "bottom",
  "zIndex",
  "visibleWhen",
  "hiddenMobile",
  "hiddenTablet",
  "mobilePaddingX",
  "mobilePaddingY",
  "mobileFontSize",
  "mobileTextAlign",
]);

function isBlockStyleFieldId(id: string): boolean {
  return BLOCK_STYLE_FIELD_IDS.has(id);
}

// ── Universal style field injection ────────────────────────────────────────

let STYLE_PUCK_FIELDS_CACHE:
  | { fields: Fields; defaults: Record<string, unknown> }
  | undefined;

function buildStylePuckFields(): {
  fields: Fields;
  defaults: Record<string, unknown>;
} {
  if (STYLE_PUCK_FIELDS_CACHE) return STYLE_PUCK_FIELDS_CACHE;
  const synthetic = entityToPuckComponent({
    type: "__style__",
    label: "__style__",
    fields: STYLE_FIELD_DEFS.map((f) => {
      const o: MappableField = { id: f.id, type: f.type };
      if (f.label !== undefined) o.label = f.label;
      if ((f as { default?: unknown }).default !== undefined) {
        o.default = (f as { default?: unknown }).default;
      }
      if (
        (f as { enumOptions?: ReadonlyArray<{ value: string; label: string }> })
          .enumOptions
      ) {
        o.enumOptions = (
          f as { enumOptions: ReadonlyArray<{ value: string; label: string }> }
        ).enumOptions;
      }
      return o;
    }),
  });
  STYLE_PUCK_FIELDS_CACHE = {
    fields: (synthetic.fields ?? {}) as Fields,
    defaults: (synthetic.defaultProps ?? {}) as Record<string, unknown>,
  };
  return STYLE_PUCK_FIELDS_CACHE;
}

/**
 * Post-process every `ComponentConfig` so authors get the universal style
 * fields (font/color/align/padding/…) and the inner render is wrapped in a
 * styled div. Components that already declare a `fontFamily` field (because
 * they ran through `entityToPuckComponent` on a `STYLE_FIELD_DEFS`-spreading
 * def) are left alone — they're already styled by the generic renderer.
 */
function attachStyleFieldsInPlace(
  components: Record<string, ComponentConfig>,
): void {
  const { fields: styleFields, defaults: styleDefaults } = buildStylePuckFields();
  for (const [name, cfg] of Object.entries(components)) {
    const existingFields = (cfg.fields ?? {}) as Record<string, unknown>;
    if ("fontFamily" in existingFields) continue;
    const originalRender = cfg.render;
    const mergedFields = { ...existingFields, ...styleFields } as Fields;
    const mergedDefaults = {
      ...(cfg.defaultProps ?? {}),
      ...styleDefaults,
    };
    components[name] = {
      ...cfg,
      fields: mergedFields,
      defaultProps: mergedDefaults,
      render: ((props: unknown) => {
        const p = props as Record<string, unknown>;
        const style = computeBlockStyle(extractBlockStyle(p) as BlockStyleData);
        const cls =
          typeof p["className"] === "string" && p["className"] !== ""
            ? (p["className"] as string)
            : undefined;
        const inner = originalRender
          ? (originalRender as (x: unknown) => ReactNode)(props)
          : null;
        const hasStyle = Object.keys(style).length > 0;
        if (!hasStyle && !cls) return <>{inner}</>;
        return (
          <div style={style} {...(cls ? { className: cls } : {})}>
            {inner}
          </div>
        );
      }) as ComponentConfig["render"],
    };
  }
}

// ── Hook-requiring inner renderers ─────────────────────────────────────────

/**
 * Luau UI block renderer. Factored out of the inline widget render because
 * `useLuauParserReady()` is a hook — Puck invokes `render` as a component,
 * so this function component can host the hook legally.
 */
function PuckLuauBlockRender({
  source,
  title,
}: {
  source: string;
  title: string;
}) {
  useLuauParserReady();
  const result = parseLuauUi(source);
  return (
    <div
      style={{
        border: "1px solid #06b6d4",
        borderRadius: 6,
        padding: 12,
        margin: "4px 0",
        background: "#0a1929",
      }}
      data-testid="puck-luau-block"
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          color: "#06b6d4",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          marginBottom: 6,
          display: "flex",
          alignItems: "center",
          gap: 4,
        }}
      >
        {"\uD83C\uDF19"} {title}
      </div>
      {result.error ? (
        <div style={{ color: "#f87171", fontSize: 12 }}>Error: {result.error}</div>
      ) : (
        <div>{result.nodes.map((node, i) => renderUINode(node, i))}</div>
      )}
    </div>
  );
}

/**
 * Image block renderer. Resolves `vfs://` sources via `useResolvedMediaUrl`
 * (a hook) and renders an actual `<img>` with caption. Lives as a component
 * so it can own the hook.
 */
function PuckImageBlockRender({
  src,
  alt,
  caption,
  width,
  height,
  kernel,
}: {
  src: string;
  alt: string;
  caption: string;
  width: number | undefined;
  height: number | undefined;
  kernel: StudioKernel;
}) {
  const { url, loading } = useResolvedMediaUrl(src || null, kernel.vfs);

  if (!src) {
    return (
      <div
        data-testid="puck-image-empty"
        style={{
          border: "1px dashed #94a3b8",
          borderRadius: 6,
          padding: 18,
          margin: "4px 0",
          background: "#f8fafc",
          color: "#64748b",
          textAlign: "center",
          fontSize: 12,
        }}
      >
        Upload an image or paste a URL.
      </div>
    );
  }
  if (loading && !url) {
    return (
      <div
        data-testid="puck-image-loading"
        style={{ padding: 12, color: "#94a3b8", fontSize: 12 }}
      >
        Loading image…
      </div>
    );
  }
  if (!url) {
    return (
      <div
        data-testid="puck-image-missing"
        style={{
          border: "1px dashed #ef4444",
          borderRadius: 6,
          padding: 12,
          color: "#b91c1c",
          fontSize: 12,
        }}
      >
        Image source could not be resolved.
      </div>
    );
  }
  return (
    <figure data-testid="puck-image" style={{ margin: "0 0 8px 0" }}>
      <img
        src={url}
        alt={alt || "Image"}
        {...(width ? { width } : {})}
        {...(height ? { height } : {})}
        style={{ maxWidth: "100%", display: "block", borderRadius: 6 }}
      />
      {caption ? (
        <figcaption
          style={{
            marginTop: 6,
            fontSize: 12,
            color: "#64748b",
            fontStyle: "italic",
            textAlign: "center",
          }}
        >
          {caption}
        </figcaption>
      ) : null}
    </figure>
  );
}

// ── Record-list component (migrated from panels/puck-providers) ────────────

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

const VALID_TEMPLATE_KINDS: ReadonlySet<TemplateFieldKind> =
  new Set<TemplateFieldKind>(["text", "date", "badge", "status", "tags"]);

interface RecordListProps extends Record<string, unknown> {
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

/** Parse a compact filter expression into `FilterConfig[]`. Exported for tests. */
export function parseFilterExpression(input: string): FilterConfig[] {
  if (!input || !input.trim()) return [];
  const clauses = input
    .split(";")
    .map((c) => c.trim())
    .filter(Boolean);
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
        value: valueRaw
          .split(",")
          .map((v) => v.trim())
          .filter(Boolean),
      });
      continue;
    }
    out.push({ field, op, value: valueRaw });
  }
  return out;
}

/** Parse a meta-fields string into `TemplateField[]`. Exported for tests. */
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

/** Compose a `RecordListTemplate` from the raw widget props. */
export function buildTemplate(props: RecordListProps): RecordListTemplate {
  const titleField = props.titleField?.trim() || "name";
  const subtitleField = props.subtitleField?.trim();
  const meta = parseMetaFields(props.metaFields ?? "");
  const template: RecordListTemplate = { title: { field: titleField } };
  if (subtitleField) template.subtitle = { field: subtitleField };
  if (meta.length > 0) template.meta = meta;
  return template;
}

/** Compose a `ViewConfig` (filters/sorts/limit) from the raw widget props. */
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

function buildRecordListComponent(kernel: StudioKernel): ComponentConfig {
  return widget<RecordListProps>(kernel, {
    type: "record-list",
    fields: {
      recordType: f.text("Record Type"),
      titleField: f.text("Title Field"),
      subtitleField: f.text("Subtitle Field"),
      metaFields: f.area("Meta Fields"),
      filterExpression: f.area("Filter Expression"),
      sortField: f.text("Sort Field"),
      sortDir: f.select(
        [
          ["desc", "Descending"],
          ["asc", "Ascending"],
        ],
        "Sort Direction",
      ),
      limit: f.num("Limit"),
      emptyMessage: f.text("Empty Message"),
    },
    defaults: {
      recordType: "task",
      titleField: "name",
      subtitleField: "description",
      metaFields: "status:badge, date:date",
      filterExpression: "",
      sortField: "updatedAt",
      sortDir: "desc",
      limit: 50,
      emptyMessage: "No records to display.",
    },
    render: ({ props, kernel: k }) => {
      const recordType = (props.recordType ?? "").trim();
      const objects = k.store.allObjects().filter((o) => {
        if (o.deletedAt) return false;
        if (!recordType || recordType === "*") return true;
        return o.type === recordType;
      });
      return (
        <RecordListRenderer
          objects={objects}
          template={buildTemplate(props)}
          viewConfig={buildViewConfig(props)}
          emptyMessage={props.emptyMessage ?? "No records to display."}
        />
      );
    },
  });
}

// ── Transform-style entities (shells / button / card / image / hero) ──────

type SlotFn = (props?: Record<string, unknown>) => ReactNode;

/**
 * Build a layout-shell component (page-shell / app-shell / site-header /
 * site-footer / side-bar / nav-bar / hero). Fields flow through
 * `entityToPuckComponent` so they get className/padding/font treatment,
 * then Puck `slot` fields are appended for each region so authors can drag
 * real components into header/sidebar/main/… zones.
 */
function buildShellComponent(def: EntityDef<string, unknown>): ComponentConfig {
  const base = entityToPuckComponent({
    type: def.type,
    label: def.label ?? def.type,
    icon: typeof def.icon === "string" ? def.icon : "",
    fields: def.fields ?? [],
  });
  const slotNames = getShellSlots(def.type);
  const extraFields: Record<string, Fields[string]> = {};
  const extraDefaults: Record<string, unknown> = {};
  for (const slot of slotNames) {
    extraFields[slot] = { type: "slot" } as unknown as Fields[string];
    extraDefaults[slot] = [];
  }
  const layoutRender = (props: unknown) => {
    const p = props as Record<string, unknown>;
    const className =
      typeof p["className"] === "string" && p["className"] !== ""
        ? (p["className"] as string)
        : undefined;
    const slotNodes: Record<string, ReactNode> = {};
    for (const slot of slotNames) {
      const Slot = p[slot] as SlotFn | undefined;
      slotNodes[slot] = typeof Slot === "function" ? <Slot /> : null;
    }
    const passthrough: Record<string, unknown> = {
      ...p,
      ...(className ? { className } : {}),
      ...slotNodes,
    };
    switch (def.type) {
      case "page-shell":
        return <PageShellRenderer {...(passthrough as object)} />;
      case "app-shell":
        return <AppShellRenderer {...(passthrough as object)} />;
      case "site-header":
        return <SiteHeaderRenderer {...(passthrough as object)} />;
      case "site-footer":
        return <SiteFooterRenderer {...(passthrough as object)} />;
      case "side-bar":
        return <SideBarRenderer {...(passthrough as object)} />;
      case "nav-bar":
        return <NavBarRenderer {...(passthrough as object)} />;
      case "hero":
      default:
        return <HeroRenderer {...(passthrough as object)} />;
    }
  };
  return {
    ...base,
    fields: { ...(base.fields ?? {}), ...extraFields } as Fields,
    defaultProps: { ...(base.defaultProps ?? {}), ...extraDefaults },
    render: layoutRender,
  };
}

/** Real interactive button preview. */
function buildButtonComponent(def: EntityDef<string, unknown>): ComponentConfig {
  const cfg = entityToPuckComponent({
    type: def.type,
    label: def.label ?? def.type,
    icon: typeof def.icon === "string" ? def.icon : "",
    fields: def.fields ?? [],
  });
  cfg.render = (props) => {
    const p = props as Record<string, unknown>;
    return (
      <ButtonRenderer
        label={(p["label"] as string) || "Button"}
        href={(p["href"] as string) || undefined}
        variant={(p["variant"] as ButtonVariant) || "primary"}
        size={(p["size"] as ButtonSize) || "md"}
        icon={(p["icon"] as string) || undefined}
        iconPosition={(p["iconPosition"] as ButtonIconPosition) || "left"}
        fullWidth={asBool(p["fullWidth"])}
        disabled={asBool(p["disabled"])}
        loading={asBool(p["loading"])}
        rounded={(p["rounded"] as ButtonRounded) || "md"}
        shadow={(p["shadow"] as ButtonShadow) || "none"}
        hoverEffect={(p["hoverEffect"] as ButtonHoverEffect) || "none"}
        target={(p["target"] as ButtonTarget) || "_self"}
        rel={(p["rel"] as string) || undefined}
        buttonType={(p["buttonType"] as ButtonHtmlType) || "button"}
        ariaLabel={(p["ariaLabel"] as string) || undefined}
      />
    );
  };
  return cfg;
}

/** Real card preview with variant/layout/hover and VFS-aware imagery. */
function buildCardComponent(
  kernel: StudioKernel,
  def: EntityDef<string, unknown>,
): ComponentConfig {
  const cfg = entityToPuckComponent({
    type: def.type,
    label: def.label ?? def.type,
    icon: typeof def.icon === "string" ? def.icon : "",
    fields: def.fields ?? [],
  });
  if (cfg.fields) {
    const nextFields = { ...(cfg.fields as Record<string, unknown>) };
    nextFields["imageUrl"] = mediaUploadField(kernel, {
      label: "Image",
      accept: "image",
    });
    cfg.fields = nextFields as Fields;
  }
  cfg.render = (props) => {
    const p = props as Record<string, unknown>;
    const opacity =
      typeof p["overlayOpacity"] === "number"
        ? (p["overlayOpacity"] as number)
        : undefined;
    return (
      <CardRenderer
        title={(p["title"] as string) || undefined}
        body={(p["body"] as string) || undefined}
        imageUrl={(p["imageUrl"] as string) || undefined}
        linkUrl={(p["linkUrl"] as string) || undefined}
        variant={(p["variant"] as CardVariant) || "elevated"}
        layout={(p["layout"] as CardLayout) || "vertical"}
        hoverEffect={(p["hoverEffect"] as CardHoverEffect) || "lift"}
        mediaFit={(p["mediaFit"] as CardMediaFit) || "cover"}
        mediaAspectRatio={(p["mediaAspectRatio"] as string) || undefined}
        eyebrow={(p["eyebrow"] as string) || undefined}
        ctaLabel={(p["ctaLabel"] as string) || undefined}
        ctaVariant={(p["ctaVariant"] as ButtonVariant) || "primary"}
        overlayOpacity={opacity}
      />
    );
  };
  return cfg;
}

/** Image block backed by `PuckImageBlockRender` for VFS resolution. */
function buildImageComponent(kernel: StudioKernel): ComponentConfig {
  return widget<{
    src: string;
    alt: string;
    caption: string;
    width: number;
    height: number;
  }>(kernel, {
    type: "image",
    bare: true,
    fields: {
      src: mediaUploadField(kernel, {
        label: "Source",
        accept: "image",
      }) as unknown as Fields[string],
      alt: f.text(),
      caption: f.text(),
      width: f.num(),
      height: f.num(),
    },
    defaults: { src: "", alt: "", caption: "", width: 0, height: 0 },
    render: ({ props, kernel: k }) => {
      const width = props.width > 0 ? props.width : undefined;
      const height = props.height > 0 ? props.height : undefined;
      return (
        <PuckImageBlockRender
          src={props.src}
          alt={props.alt}
          caption={props.caption}
          width={width}
          height={height}
          kernel={k}
        />
      );
    },
  });
}

// ── Declarative widget table ───────────────────────────────────────────────

/**
 * Every entity type whose Puck config is fully expressible as a `widget()`
 * call lives here. Order and grouping mirror the sidebar categories in
 * `layout-panel-data.ts` for human scanning. Runs once at kernel boot; the
 * closures capture `kernel` so render functions can read the store and
 * mutate via `ctx.select`/`ctx.update`/`ctx.create`.
 */
function buildWidgetTable(
  kernel: StudioKernel,
): Record<string, ComponentConfig> {
  const K = kernel;
  return {
    // ── Dynamic / scripted ────────────────────────────────────────────────
    "luau-block": widget<{ source: string; title: string }>(K, {
      type: "luau-block",
      bare: true,
      fields: { source: f.area(), title: f.text() },
      defaults: {
        source: 'return ui.label("Hello from Luau!")',
        title: "Luau Block",
      },
      render: ({ props }) => (
        <PuckLuauBlockRender source={props.source} title={props.title} />
      ),
    }),
    "popover-widget": widget<{ triggerLabel: string; content: string }>(K, {
      type: "popover-widget",
      fields: { triggerLabel: f.text(), content: f.area() },
      defaults: { triggerLabel: "Open", content: "" },
      render: ({ props }) => (
        <PopoverWidgetRenderer
          triggerLabel={props.triggerLabel}
          content={props.content}
        />
      ),
    }),
    "slide-panel": widget<{ label: string; content: string; collapsed: string }>(
      K,
      {
        type: "slide-panel",
        fields: {
          label: f.text(),
          content: f.area(),
          collapsed: f.stringBool({
            trueLabel: "Collapsed",
            falseLabel: "Open",
          }),
        },
        defaults: { label: "Details", content: "", collapsed: "false" },
        render: ({ props }) => (
          <SlidePanelRenderer
            label={props.label}
            content={props.content}
            collapsed={asBool(props.collapsed)}
          />
        ),
      },
    ),

    // ── Data-aware views (facet + spatial + relationship) ────────────────
    "facet-view": widget<{ facetId: string; viewMode: string; maxRows: number }>(
      K,
      {
        type: "facet-view",
        fields: {
          facetId: facetPickerField(K, { label: "Facet" }) as Fields[string],
          viewMode: f.select([
            ["form", "Form"],
            ["list", "List"],
            ["table", "Table"],
            ["report", "Report"],
            ["card", "Card"],
          ]),
          maxRows: f.num(),
        },
        defaults: { facetId: "", viewMode: "form", maxRows: 25 },
        render: ({ props, kernel: k }) => {
          const facetDef = props.facetId
            ? k.getFacetDefinition(props.facetId)
            : undefined;
          const objects = facetDef
            ? k.store
                .allObjects()
                .filter((o) => o.type === facetDef.objectType && !o.deletedAt)
            : [];
          return (
            <FacetViewRenderer
              definition={facetDef}
              objects={objects}
              viewMode={props.viewMode as FacetLayout}
              maxRows={props.maxRows}
            />
          );
        },
      },
    ),
    "spatial-canvas": widget<{
      facetId: string;
      canvasWidth: number;
      canvasHeight: number;
      gridSize: number;
      showGrid: string;
    }>(K, {
      type: "spatial-canvas",
      fields: {
        facetId: facetPickerField(K, { label: "Facet" }) as Fields[string],
        canvasWidth: f.num(),
        canvasHeight: f.num(),
        gridSize: f.num(),
        showGrid: f.stringBool(),
      },
      defaults: {
        facetId: "",
        canvasWidth: 612,
        canvasHeight: 400,
        gridSize: 8,
        showGrid: "true",
      },
      render: ({ props, kernel: k }) => {
        const facetDef = props.facetId
          ? k.getFacetDefinition(props.facetId)
          : undefined;
        if (!facetDef) {
          return (
            <div
              style={{
                border: "1px solid #f97316",
                borderRadius: 6,
                padding: 12,
                background: "#1a1a2e",
                color: "#888",
                textAlign: "center",
                minHeight: 80,
              }}
            >
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  color: "#f97316",
                  textTransform: "uppercase",
                  marginBottom: 6,
                }}
              >
                {"\uD83D\uDCD0"} Spatial Canvas
              </div>
              <div style={{ fontSize: 12 }}>
                Set a Facet Definition ID to render the canvas.
              </div>
            </div>
          );
        }
        return (
          <SpatialCanvasRenderer
            definition={facetDef}
            editable={false}
            canvasWidth={props.canvasWidth}
            canvasHeight={props.canvasHeight}
            gridSize={props.gridSize}
            showGrid={asBool(props.showGrid)}
          />
        );
      },
    }),
    "data-portal": widget<{
      relationshipId: string;
      displayFields: string;
      visibleRows: number;
      allowCreation: string;
      sortField: string;
    }>(K, {
      type: "data-portal",
      fields: {
        relationshipId: f.text(),
        displayFields: f.text(),
        visibleRows: f.num(),
        allowCreation: f.stringBool(),
        sortField: f.text(),
      },
      defaults: {
        relationshipId: "",
        displayFields: "",
        visibleRows: 5,
        allowCreation: "false",
        sortField: "",
      },
      render: ({ props, kernel: k }) => {
        if (!props.relationshipId) {
          return (
            <div
              style={{
                border: "1px solid #8b5cf6",
                borderRadius: 6,
                padding: 12,
                background: "#1e1e1e",
                color: "#888",
                textAlign: "center",
              }}
            >
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  color: "#8b5cf6",
                  textTransform: "uppercase",
                  marginBottom: 6,
                }}
              >
                {"\uD83D\uDD17"} Data Portal
              </div>
              <div style={{ fontSize: 12 }}>
                Set a Relationship Edge Type to display related records.
              </div>
            </div>
          );
        }
        return (
          <DataPortalRenderer
            objects={k.store.allObjects().filter((o) => !o.deletedAt)}
            edges={k.store.allEdges()}
            relationshipId={props.relationshipId}
            displayFields={props.displayFields
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean)}
            visibleRows={props.visibleRows}
            sortField={props.sortField || undefined}
          />
        );
      },
    }),

    // ── Collection-driven data widgets (configurable type) ───────────────
    "kanban-widget": widget<{
      collectionType: string;
      groupField: string;
      titleField: string;
      colorField: string;
      maxCardsPerColumn: number;
    }>(K, {
      type: "kanban-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        groupField: f.text(),
        titleField: f.text(),
        colorField: f.text(),
        maxCardsPerColumn: f.num(),
      },
      defaults: {
        collectionType: "",
        groupField: "status",
        titleField: "name",
        colorField: "",
        maxCardsPerColumn: 50,
      },
      render: ({ props, objects, update }) => (
        <KanbanWidgetRenderer
          objects={objects}
          groupField={props.groupField}
          titleField={props.titleField}
          colorField={props.colorField || undefined}
          maxCardsPerColumn={props.maxCardsPerColumn}
          onMoveObject={(id, newValue) =>
            update(id, { [props.groupField]: newValue })
          }
        />
      ),
    }),
    "list-widget": widget<{
      collectionType: string;
      titleField: string;
      subtitleField: string;
      showStatus: boolean;
      showTimestamp: boolean;
    }>(K, {
      type: "list-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        titleField: f.text(),
        subtitleField: f.text(),
        showStatus: f.yesNo(),
        showTimestamp: f.yesNo(),
      },
      defaults: {
        collectionType: "",
        titleField: "name",
        subtitleField: "type",
        showStatus: true,
        showTimestamp: true,
      },
      render: ({ props, objects, selectedId, select }) => (
        <ListWidgetRenderer
          objects={objects}
          titleField={props.titleField}
          subtitleField={props.subtitleField}
          showStatus={props.showStatus !== false}
          showTimestamp={props.showTimestamp !== false}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "table-widget": widget<{
      collectionType: string;
      columns: string;
      sortField: string;
      sortDir: string;
    }>(K, {
      type: "table-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        columns: f.area(),
        sortField: f.text(),
        sortDir: f.select([
          ["asc", "Ascending"],
          ["desc", "Descending"],
        ]),
      },
      defaults: {
        collectionType: "",
        columns: "name:Name, type:Type, status:Status, updatedAt:Updated",
        sortField: "name",
        sortDir: "asc",
      },
      render: ({ props, objects, selectedId, select }) => (
        <TableWidgetRenderer
          objects={objects}
          columns={parseTableColumns(props.columns)}
          sortField={props.sortField}
          sortDir={props.sortDir as TableSortDir}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "card-grid-widget": widget<{
      collectionType: string;
      titleField: string;
      subtitleField: string;
      minColumnWidth: number;
      showStatus: boolean;
    }>(K, {
      type: "card-grid-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        titleField: f.text(),
        subtitleField: f.text(),
        minColumnWidth: f.num(),
        showStatus: f.yesNo(),
      },
      defaults: {
        collectionType: "",
        titleField: "name",
        subtitleField: "type",
        minColumnWidth: 220,
        showStatus: true,
      },
      render: ({ props, objects, selectedId, select }) => (
        <CardGridWidgetRenderer
          objects={objects}
          titleField={props.titleField}
          subtitleField={props.subtitleField}
          minColumnWidth={props.minColumnWidth}
          showStatus={props.showStatus !== false}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "report-widget": widget<{
      collectionType: string;
      groupField: string;
      titleField: string;
      valueField: string;
      aggregation: string;
    }>(K, {
      type: "report-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        groupField: f.text(),
        titleField: f.text(),
        valueField: f.text(),
        aggregation: f.select([
          ["count", "Count"],
          ["sum", "Sum"],
          ["avg", "Average"],
          ["min", "Min"],
          ["max", "Max"],
        ]),
      },
      defaults: {
        collectionType: "",
        groupField: "type",
        titleField: "name",
        valueField: "",
        aggregation: "count",
      },
      render: ({ props, objects, select }) => (
        <ReportWidgetRenderer
          objects={objects}
          groupField={props.groupField}
          titleField={props.titleField}
          valueField={props.valueField || undefined}
          aggregation={props.aggregation as ReportAggregation}
          onSelectObject={select}
        />
      ),
    }),
    "calendar-widget": widget<{
      collectionType: string;
      dateField: string;
      titleField: string;
      viewType: string;
    }>(K, {
      type: "calendar-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        dateField: f.text(),
        titleField: f.text(),
        viewType: f.select([
          ["month", "Month"],
          ["week", "Week"],
          ["day", "Day"],
        ]),
      },
      defaults: {
        collectionType: "",
        dateField: "date",
        titleField: "name",
        viewType: "month",
      },
      render: ({ props, objects }) => (
        <CalendarWidgetRenderer
          objects={objects}
          dateField={props.dateField}
          titleField={props.titleField}
          viewType={props.viewType as "month" | "week" | "day"}
        />
      ),
    }),
    "chart-widget": widget<{
      collectionType: string;
      chartType: string;
      groupField: string;
      valueField: string;
      aggregation: string;
    }>(K, {
      type: "chart-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        chartType: f.select([
          ["bar", "Bar"],
          ["line", "Line"],
          ["pie", "Pie"],
          ["area", "Area"],
        ]),
        groupField: f.text(),
        valueField: f.text(),
        aggregation: f.select([
          ["count", "Count"],
          ["sum", "Sum"],
          ["avg", "Average"],
          ["min", "Min"],
          ["max", "Max"],
        ]),
      },
      defaults: {
        collectionType: "",
        chartType: "bar",
        groupField: "",
        valueField: "",
        aggregation: "count",
      },
      render: ({ props, objects }) => (
        <ChartWidgetRenderer
          objects={objects}
          chartType={props.chartType as ChartType}
          groupField={props.groupField}
          valueField={props.valueField || undefined}
          aggregation={props.aggregation as ChartAggregation}
        />
      ),
    }),
    "map-widget": widget<{
      collectionType: string;
      latField: string;
      lngField: string;
      titleField: string;
      initialZoom: number;
    }>(K, {
      type: "map-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        latField: f.text(),
        lngField: f.text(),
        titleField: f.text(),
        initialZoom: f.num(),
      },
      defaults: {
        collectionType: "",
        latField: "lat",
        lngField: "lng",
        titleField: "name",
        initialZoom: 10,
      },
      render: ({ props, objects }) => (
        <Suspense fallback={null}>
          <MapWidgetRenderer
            objects={objects}
            latField={props.latField}
            lngField={props.lngField}
            titleField={props.titleField}
          />
        </Suspense>
      ),
    }),

    // ── Fixed record-type widgets ────────────────────────────────────────
    "tasks-widget": widget<{
      title: string;
      filter: string;
      project: string;
      maxItems: number;
      showPriority: boolean;
      showDueDate: boolean;
    }>(K, {
      type: "tasks-widget",
      query: { kind: "fixed", type: "task" },
      fields: {
        title: f.text(),
        filter: f.select([
          ["all", "All tasks"],
          ["open", "Open (todo + doing)"],
          ["today", "Due today"],
          ["overdue", "Overdue"],
          ["done", "Completed"],
        ]),
        project: f.text(),
        maxItems: f.num(),
        showPriority: f.showHide(),
        showDueDate: f.showHide(),
      },
      defaults: {
        title: "Tasks",
        filter: "open",
        project: "",
        maxItems: 10,
        showPriority: true,
        showDueDate: true,
      },
      render: ({ props, objects, selectedId, select, update }) => (
        <TasksWidgetRenderer
          objects={objects}
          title={props.title}
          filter={props.filter as TaskFilter}
          project={props.project}
          maxItems={props.maxItems}
          showPriority={props.showPriority !== false}
          showDueDate={props.showDueDate !== false}
          selectedId={selectedId}
          onSelectObject={select}
          onToggleDone={(id, newStatus) => update(id, { status: newStatus })}
        />
      ),
    }),
    "reminders-widget": widget<{
      title: string;
      filter: string;
      maxItems: number;
    }>(K, {
      type: "reminders-widget",
      query: { kind: "fixed", type: "reminder" },
      fields: {
        title: f.text(),
        filter: f.select([
          ["all", "All reminders"],
          ["upcoming", "Upcoming (open)"],
          ["overdue", "Overdue"],
          ["today", "Due today"],
          ["done", "Completed"],
        ]),
        maxItems: f.num(),
      },
      defaults: { title: "Reminders", filter: "upcoming", maxItems: 8 },
      render: ({ props, objects, selectedId, select, update }) => (
        <RemindersWidgetRenderer
          objects={objects}
          title={props.title}
          filter={props.filter as ReminderFilter}
          maxItems={props.maxItems}
          selectedId={selectedId}
          onSelectObject={select}
          onToggleDone={(id, newStatus) => update(id, { status: newStatus })}
        />
      ),
    }),
    "contacts-widget": widget<{
      title: string;
      filter: string;
      display: string;
      maxItems: number;
      showOrg: boolean;
      showActions: boolean;
    }>(K, {
      type: "contacts-widget",
      query: { kind: "fixed", type: "contact" },
      fields: {
        title: f.text(),
        filter: f.select([
          ["all", "All contacts"],
          ["favorites", "Favorites (pinned)"],
          ["recent", "Recently contacted"],
        ]),
        display: f.select([
          ["cards", "Cards"],
          ["list", "List"],
        ]),
        maxItems: f.num(),
        showOrg: f.showHide(),
        showActions: f.showHide(),
      },
      defaults: {
        title: "Contacts",
        filter: "favorites",
        display: "cards",
        maxItems: 12,
        showOrg: true,
        showActions: true,
      },
      render: ({ props, objects, selectedId, select }) => (
        <ContactsWidgetRenderer
          objects={objects}
          title={props.title}
          filter={props.filter as ContactFilter}
          display={props.display as "cards" | "list"}
          maxItems={props.maxItems}
          showOrg={props.showOrg !== false}
          showActions={props.showActions !== false}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "events-widget": widget<{
      title: string;
      range: string;
      maxItems: number;
      showLocation: boolean;
    }>(K, {
      type: "events-widget",
      query: { kind: "fixed", type: "event" },
      fields: {
        title: f.text(),
        range: f.select([
          ["today", "Today"],
          ["week", "Next 7 days"],
          ["month", "Next 30 days"],
          ["all", "All upcoming"],
        ]),
        maxItems: f.num(),
        showLocation: f.showHide(),
      },
      defaults: {
        title: "Upcoming events",
        range: "week",
        maxItems: 8,
        showLocation: true,
      },
      render: ({ props, objects, selectedId, select }) => (
        <EventsWidgetRenderer
          objects={objects}
          title={props.title}
          range={props.range as EventRange}
          maxItems={props.maxItems}
          showLocation={props.showLocation !== false}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "notes-widget": widget<{
      title: string;
      filter: string;
      tag: string;
      maxItems: number;
      previewLength: number;
    }>(K, {
      type: "notes-widget",
      query: { kind: "fixed", type: "note" },
      fields: {
        title: f.text(),
        filter: f.select([
          ["all", "All notes"],
          ["pinned", "Pinned only"],
          ["recent", "Recently edited"],
        ]),
        tag: f.text(),
        maxItems: f.num(),
        previewLength: f.num(),
      },
      defaults: {
        title: "Notes",
        filter: "pinned",
        tag: "",
        maxItems: 8,
        previewLength: 120,
      },
      render: ({ props, objects, selectedId, select }) => (
        <NotesWidgetRenderer
          objects={objects}
          title={props.title}
          filter={props.filter as NoteFilter}
          tag={props.tag}
          maxItems={props.maxItems}
          previewLength={props.previewLength}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "goals-widget": widget<{
      title: string;
      filter: string;
      maxItems: number;
    }>(K, {
      type: "goals-widget",
      query: { kind: "fixed", type: "goal" },
      fields: {
        title: f.text(),
        filter: f.select([
          ["all", "All goals"],
          ["active", "Active only"],
          ["completed", "Completed"],
        ]),
        maxItems: f.num(),
      },
      defaults: { title: "Goals", filter: "active", maxItems: 6 },
      render: ({ props, objects, selectedId, select }) => (
        <GoalsWidgetRenderer
          objects={objects}
          title={props.title}
          filter={props.filter as GoalFilter}
          maxItems={props.maxItems}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "habit-tracker-widget": widget<{
      title: string;
      maxItems: number;
      showStreak: boolean;
    }>(K, {
      type: "habit-tracker-widget",
      query: { kind: "fixed", type: "habit" },
      fields: {
        title: f.text(),
        maxItems: f.num(),
        showStreak: f.showHide(),
      },
      defaults: { title: "Habits", maxItems: 8, showStreak: true },
      render: ({ props, objects, selectedId, select }) => (
        <HabitTrackerWidgetRenderer
          objects={objects}
          title={props.title}
          maxItems={props.maxItems}
          showStreak={props.showStreak !== false}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "bookmarks-widget": widget<{
      title: string;
      folder: string;
      maxItems: number;
      display: string;
    }>(K, {
      type: "bookmarks-widget",
      query: { kind: "fixed", type: "bookmark" },
      fields: {
        title: f.text(),
        folder: f.text(),
        maxItems: f.num(),
        display: f.select([
          ["grid", "Grid"],
          ["list", "List"],
        ]),
      },
      defaults: {
        title: "Bookmarks",
        folder: "",
        maxItems: 12,
        display: "grid",
      },
      render: ({ props, objects, selectedId, select }) => (
        <BookmarksWidgetRenderer
          objects={objects}
          title={props.title}
          folder={props.folder}
          maxItems={props.maxItems}
          display={props.display as "grid" | "list"}
          selectedId={selectedId}
          onSelectObject={select}
        />
      ),
    }),
    "timer-widget": widget<{
      title: string;
      defaultMinutes: number;
      maxRecent: number;
    }>(K, {
      type: "timer-widget",
      query: { kind: "fixed", type: "timer-session" },
      fields: {
        title: f.text(),
        defaultMinutes: f.num(),
        maxRecent: f.num(),
      },
      defaults: { title: "Focus timer", defaultMinutes: 25, maxRecent: 5 },
      render: ({ props, objects, selectedId, select, create }) => (
        <TimerWidgetRenderer
          objects={objects}
          title={props.title}
          defaultMinutes={props.defaultMinutes}
          maxRecent={props.maxRecent}
          selectedId={selectedId}
          onSelectObject={select}
          onCreateSession={(durationMs) => {
            const minutes = Math.round(durationMs / 60_000);
            create({
              type: "timer-session",
              name: `${minutes}m focus`,
              parentId: null,
              position: 0,
              status: "done",
              tags: [],
              date: new Date().toISOString(),
              endDate: null,
              description: "",
              color: null,
              image: null,
              pinned: false,
              data: { durationMs, kind: "focus" },
            });
          }}
        />
      ),
    }),
    "capture-inbox-widget": widget<{
      title: string;
      maxItems: number;
      showProcessed: boolean;
    }>(K, {
      type: "capture-inbox-widget",
      query: { kind: "fixed", type: "capture" },
      fields: {
        title: f.text(),
        maxItems: f.num(),
        showProcessed: f.showHide(),
      },
      defaults: { title: "Inbox", maxItems: 10, showProcessed: false },
      render: ({
        props,
        objects,
        selectedId,
        select,
        update,
        create,
        kernel: k,
      }) => (
        <CaptureInboxWidgetRenderer
          objects={objects}
          title={props.title}
          maxItems={props.maxItems}
          showProcessed={props.showProcessed === true}
          selectedId={selectedId}
          onSelectObject={select}
          onCaptureSubmit={(text) => {
            create({
              type: "capture",
              name: text.slice(0, 80),
              parentId: null,
              position: 0,
              status: "todo",
              tags: [],
              date: null,
              endDate: null,
              description: "",
              color: null,
              image: null,
              pinned: false,
              data: { body: text, source: "quick" },
            });
          }}
          onMarkProcessed={(id) => {
            const existing = k.store.getObject(
              id as unknown as Parameters<typeof k.store.getObject>[0],
            );
            if (!existing) return;
            const prev = (existing.data ?? {}) as Record<string, unknown>;
            update(id, {
              status: "done",
              data: { ...prev, processedAt: new Date().toISOString() },
            });
          }}
        />
      ),
    }),

    // ── Layout primitives ─────────────────────────────────────────────────
    "tab-container": widget<{ tabs: string; activeTab: number }>(K, {
      type: "tab-container",
      fields: { tabs: f.text(), activeTab: f.num() },
      defaults: { tabs: "Tab 1,Tab 2", activeTab: 0 },
      render: ({ props }) => (
        <TabContainerRenderer tabs={props.tabs} activeTab={props.activeTab} />
      ),
    }),
    columns: widget<{ columnCount: number; gap: number; align: string }>(K, {
      type: "columns",
      fields: {
        columnCount: f.num(),
        gap: f.num(),
        align: f.select([
          ["start", "Start"],
          ["center", "Center"],
          ["end", "End"],
          ["stretch", "Stretch"],
        ]),
      },
      defaults: { columnCount: 2, gap: 16, align: "stretch" },
      render: ({ props }) => (
        <ColumnsRenderer
          columnCount={props.columnCount}
          gap={props.gap}
          align={props.align as "start" | "center" | "end" | "stretch"}
        />
      ),
    }),
    divider: widget<{
      dividerStyle: string;
      thickness: number;
      color: string;
      spacing: number;
      label: string;
    }>(K, {
      type: "divider",
      fields: {
        dividerStyle: f.select([
          ["solid", "Solid"],
          ["dashed", "Dashed"],
          ["dotted", "Dotted"],
        ]),
        thickness: f.num(),
        color: f.text(),
        spacing: f.num(),
        label: f.text(),
      },
      defaults: {
        dividerStyle: "solid",
        thickness: 1,
        color: "#cbd5e1",
        spacing: 12,
        label: "",
      },
      render: ({ props }) => (
        <DividerRenderer
          style={props.dividerStyle as "solid" | "dashed" | "dotted"}
          thickness={props.thickness}
          color={props.color}
          spacing={props.spacing}
          label={props.label}
        />
      ),
    }),
    spacer: widget<{ size: number; axis: string }>(K, {
      type: "spacer",
      fields: {
        size: f.num(),
        axis: f.select([
          ["vertical", "Vertical"],
          ["horizontal", "Horizontal"],
        ]),
      },
      defaults: { size: 16, axis: "vertical" },
      render: ({ props }) => (
        <SpacerRenderer
          size={props.size}
          axis={props.axis as "vertical" | "horizontal"}
        />
      ),
    }),

    // ── Form inputs (rendered bare — no wrapper div) ─────────────────────
    "text-input": widget<{
      label: string;
      placeholder: string;
      defaultValue: string;
      inputType: string;
      required: string;
      help: string;
    }>(K, {
      type: "text-input",
      bare: true,
      fields: {
        label: f.text(),
        placeholder: f.text(),
        defaultValue: f.text(),
        inputType: f.select([
          ["text", "Text"],
          ["email", "Email"],
          ["url", "URL"],
          ["tel", "Phone"],
          ["password", "Password"],
        ]),
        required: f.stringBool(),
        help: f.text(),
      },
      defaults: {
        label: "Text",
        placeholder: "",
        defaultValue: "",
        inputType: "text",
        required: "false",
        help: "",
      },
      render: ({ props }) => (
        <TextInputRenderer
          label={props.label}
          placeholder={props.placeholder}
          defaultValue={props.defaultValue}
          inputType={
            props.inputType as "text" | "email" | "url" | "tel" | "password"
          }
          required={asBool(props.required)}
          help={props.help}
        />
      ),
    }),
    "textarea-input": widget<{
      label: string;
      placeholder: string;
      defaultValue: string;
      rows: number;
      required: string;
      help: string;
    }>(K, {
      type: "textarea-input",
      bare: true,
      fields: {
        label: f.text(),
        placeholder: f.text(),
        defaultValue: f.area(),
        rows: f.num(),
        required: f.stringBool(),
        help: f.text(),
      },
      defaults: {
        label: "Description",
        placeholder: "",
        defaultValue: "",
        rows: 4,
        required: "false",
        help: "",
      },
      render: ({ props }) => (
        <TextareaInputRenderer
          label={props.label}
          placeholder={props.placeholder}
          defaultValue={props.defaultValue}
          rows={props.rows}
          required={asBool(props.required)}
          help={props.help}
        />
      ),
    }),
    "select-input": widget<{
      label: string;
      options: string;
      defaultValue: string;
      required: string;
      help: string;
    }>(K, {
      type: "select-input",
      bare: true,
      fields: {
        label: f.text(),
        options: f.area(),
        defaultValue: f.text(),
        required: f.stringBool(),
        help: f.text(),
      },
      defaults: {
        label: "Choose",
        options: "one,two,three",
        defaultValue: "",
        required: "false",
        help: "",
      },
      render: ({ props }) => (
        <SelectInputRenderer
          label={props.label}
          options={props.options}
          defaultValue={props.defaultValue}
          required={asBool(props.required)}
          help={props.help}
        />
      ),
    }),
    "checkbox-input": widget<{
      label: string;
      defaultChecked: string;
      help: string;
    }>(K, {
      type: "checkbox-input",
      bare: true,
      fields: {
        label: f.text(),
        defaultChecked: f.stringBool({
          trueLabel: "Checked",
          falseLabel: "Unchecked",
        }),
        help: f.text(),
      },
      defaults: { label: "Accept", defaultChecked: "false", help: "" },
      render: ({ props }) => (
        <CheckboxInputRenderer
          label={props.label}
          defaultChecked={asBool(props.defaultChecked)}
          help={props.help}
        />
      ),
    }),
    "number-input": widget<{
      label: string;
      defaultValue: number;
      min: number;
      max: number;
      step: number;
      required: string;
      help: string;
    }>(K, {
      type: "number-input",
      bare: true,
      fields: {
        label: f.text(),
        defaultValue: f.num(),
        min: f.num(),
        max: f.num(),
        step: f.num(),
        required: f.stringBool(),
        help: f.text(),
      },
      defaults: {
        label: "Amount",
        defaultValue: 0,
        min: 0,
        max: 100,
        step: 1,
        required: "false",
        help: "",
      },
      render: ({ props }) => (
        <NumberInputRenderer
          label={props.label}
          defaultValue={props.defaultValue}
          min={props.min}
          max={props.max}
          step={props.step}
          required={asBool(props.required)}
          help={props.help}
        />
      ),
    }),
    "date-input": widget<{
      label: string;
      defaultValue: string;
      dateKind: string;
      required: string;
      help: string;
    }>(K, {
      type: "date-input",
      bare: true,
      fields: {
        label: f.text(),
        defaultValue: f.text(),
        dateKind: f.select([
          ["date", "Date"],
          ["datetime-local", "Date + Time"],
          ["time", "Time"],
        ]),
        required: f.stringBool(),
        help: f.text(),
      },
      defaults: {
        label: "Date",
        defaultValue: "",
        dateKind: "date",
        required: "false",
        help: "",
      },
      render: ({ props }) => (
        <DateInputRenderer
          label={props.label}
          defaultValue={props.defaultValue}
          dateKind={props.dateKind as "date" | "datetime-local" | "time"}
          required={asBool(props.required)}
          help={props.help}
        />
      ),
    }),

    // ── Data-display chrome ──────────────────────────────────────────────
    "stat-widget": widget<{
      collectionType: string;
      label: string;
      aggregation: string;
      valueField: string;
      prefix: string;
      suffix: string;
      decimals: number;
      thousands: string;
    }>(K, {
      type: "stat-widget",
      query: { kind: "by-prop" },
      fields: {
        collectionType: f.text(),
        label: f.text(),
        aggregation: f.select([
          ["count", "Count"],
          ["sum", "Sum"],
          ["avg", "Average"],
          ["min", "Min"],
          ["max", "Max"],
        ]),
        valueField: f.text(),
        prefix: f.text(),
        suffix: f.text(),
        decimals: f.num(),
        thousands: f.stringBool(),
      },
      defaults: {
        collectionType: "",
        label: "Total",
        aggregation: "count",
        valueField: "",
        prefix: "",
        suffix: "",
        decimals: 0,
        thousands: "true",
      },
      render: ({ props, objects }) => (
        <StatWidgetRenderer
          objects={objects}
          label={props.label}
          aggregation={props.aggregation as StatAggregation}
          valueField={props.valueField || undefined}
          prefix={props.prefix}
          suffix={props.suffix}
          decimals={props.decimals}
          thousands={asBool(props.thousands)}
        />
      ),
    }),
    badge: widget<{
      label: string;
      tone: string;
      icon: string;
      outline: string;
    }>(K, {
      type: "badge",
      bare: true,
      fields: {
        label: f.text(),
        tone: f.select([
          ["neutral", "Neutral"],
          ["info", "Info"],
          ["success", "Success"],
          ["warning", "Warning"],
          ["danger", "Danger"],
        ]),
        icon: f.text(),
        outline: f.stringBool({ trueLabel: "Outline", falseLabel: "Solid" }),
      },
      defaults: { label: "New", tone: "info", icon: "", outline: "false" },
      render: ({ props }) => (
        <BadgeRenderer
          label={props.label}
          tone={props.tone as BadgeTone}
          icon={props.icon || undefined}
          outline={asBool(props.outline)}
        />
      ),
    }),
    alert: widget<{
      title: string;
      message: string;
      tone: string;
      icon: string;
    }>(K, {
      type: "alert",
      bare: true,
      fields: {
        title: f.text(),
        message: f.area(),
        tone: f.select([
          ["neutral", "Neutral"],
          ["info", "Info"],
          ["success", "Success"],
          ["warning", "Warning"],
          ["danger", "Danger"],
        ]),
        icon: f.text(),
      },
      defaults: { title: "", message: "Notice.", tone: "info", icon: "" },
      render: ({ props }) => (
        <AlertRenderer
          title={props.title || undefined}
          message={props.message}
          tone={props.tone as BadgeTone}
          icon={props.icon || undefined}
        />
      ),
    }),
    "progress-bar": widget<{
      label: string;
      value: number;
      max: number;
      tone: string;
      showPercent: string;
    }>(K, {
      type: "progress-bar",
      bare: true,
      fields: {
        label: f.text(),
        value: f.num(),
        max: f.num(),
        tone: f.select([
          ["neutral", "Neutral"],
          ["info", "Info"],
          ["success", "Success"],
          ["warning", "Warning"],
          ["danger", "Danger"],
        ]),
        showPercent: f.stringBool(),
      },
      defaults: {
        label: "Progress",
        value: 50,
        max: 100,
        tone: "info",
        showPercent: "true",
      },
      render: ({ props }) => (
        <ProgressBarRenderer
          label={props.label || undefined}
          value={props.value}
          max={props.max}
          tone={props.tone as BadgeTone}
          showPercent={asBool(props.showPercent)}
        />
      ),
    }),

    // ── Content widgets ──────────────────────────────────────────────────
    "markdown-widget": widget<{ source: string }>(K, {
      type: "markdown-widget",
      bare: true,
      fields: { source: f.area() },
      defaults: { source: "# Heading\n\nSome **bold** content." },
      render: ({ props }) => <MarkdownWidgetRenderer source={props.source} />,
    }),
    "iframe-widget": widget<{
      src: string;
      title: string;
      height: number;
      allowFullscreen: string;
    }>(K, {
      type: "iframe-widget",
      bare: true,
      fields: {
        src: f.text(),
        title: f.text(),
        height: f.num(),
        allowFullscreen: f.stringBool(),
      },
      defaults: {
        src: "",
        title: "Embedded content",
        height: 360,
        allowFullscreen: "true",
      },
      render: ({ props }) => (
        <IframeWidgetRenderer
          src={props.src}
          title={props.title}
          height={props.height}
          allowFullscreen={asBool(props.allowFullscreen)}
        />
      ),
    }),
    "code-block": widget<{
      source: string;
      language: string;
      caption: string;
      lineNumbers: string;
      wrap: string;
    }>(K, {
      type: "code-block",
      bare: true,
      fields: {
        source: f.area(),
        language: f.select([
          ["typescript", "TypeScript"],
          ["javascript", "JavaScript"],
          ["json", "JSON"],
          ["luau", "Luau"],
          ["rust", "Rust"],
          ["python", "Python"],
          ["bash", "Bash"],
          ["yaml", "YAML"],
          ["markdown", "Markdown"],
          ["text", "Plain Text"],
        ]),
        caption: f.text(),
        lineNumbers: f.stringBool(),
        wrap: f.stringBool(),
      },
      defaults: {
        source: 'function hello() {\n  return "world";\n}',
        language: "typescript",
        caption: "",
        lineNumbers: "true",
        wrap: "false",
      },
      render: ({ props }) => (
        <CodeBlockRenderer
          source={props.source}
          language={props.language || undefined}
          caption={props.caption || undefined}
          lineNumbers={asBool(props.lineNumbers)}
          wrap={asBool(props.wrap)}
        />
      ),
    }),

    // ── Media widgets ────────────────────────────────────────────────────
    "video-widget": widget<{
      src: string;
      poster: string;
      caption: string;
      width: number;
      height: number;
      controls: string;
      autoplay: string;
      loop: string;
      muted: string;
    }>(K, {
      type: "video-widget",
      bare: true,
      fields: {
        src: mediaUploadField(K, {
          label: "Video",
          accept: "video",
        }) as Fields[string],
        poster: mediaUploadField(K, {
          label: "Poster image",
          accept: "image",
        }) as Fields[string],
        caption: f.text(),
        width: f.num(),
        height: f.num(),
        controls: f.stringBool(),
        autoplay: f.stringBool(),
        loop: f.stringBool(),
        muted: f.stringBool(),
      },
      defaults: {
        src: "",
        poster: "",
        caption: "",
        width: 640,
        height: 360,
        controls: "true",
        autoplay: "false",
        loop: "false",
        muted: "false",
      },
      render: ({ props }) => (
        <VideoWidgetRenderer
          src={props.src || undefined}
          poster={props.poster || undefined}
          caption={props.caption || undefined}
          width={props.width || undefined}
          height={props.height || undefined}
          controls={asBool(props.controls, true)}
          autoplay={asBool(props.autoplay)}
          loop={asBool(props.loop)}
          muted={asBool(props.muted)}
        />
      ),
    }),
    "audio-widget": widget<{
      src: string;
      caption: string;
      controls: string;
      autoplay: string;
      loop: string;
      muted: string;
    }>(K, {
      type: "audio-widget",
      bare: true,
      fields: {
        src: mediaUploadField(K, {
          label: "Audio",
          accept: "audio",
        }) as Fields[string],
        caption: f.text(),
        controls: f.stringBool(),
        autoplay: f.stringBool(),
        loop: f.stringBool(),
        muted: f.stringBool(),
      },
      defaults: {
        src: "",
        caption: "",
        controls: "true",
        autoplay: "false",
        loop: "false",
        muted: "false",
      },
      render: ({ props }) => (
        <AudioWidgetRenderer
          src={props.src || undefined}
          caption={props.caption || undefined}
          controls={asBool(props.controls, true)}
          autoplay={asBool(props.autoplay)}
          loop={asBool(props.loop)}
          muted={asBool(props.muted)}
        />
      ),
    }),
  };
}

// ── Orchestrator ───────────────────────────────────────────────────────────

/**
 * Build the full `Record<string, ComponentConfig>` map for every entity in
 * the kernel that's a `component` or `section`. Called once during kernel
 * construction; the result is `registerDirect`'d onto `kernel.puckComponents`
 * so panels read it from the registry instead of rebuilding on each render.
 */
export function buildEntityPuckComponents(
  kernel: StudioKernel,
): Record<string, ComponentConfig> {
  const components: Record<string, ComponentConfig> = {};
  const table = buildWidgetTable(kernel);

  for (const def of kernel.registry.allDefs()) {
    if (def.category !== "component" && def.category !== "section") continue;

    const name = kebabToPascal(def.type);

    // 1. Declarative widget table — the common case.
    const tableEntry = table[def.type];
    if (tableEntry) {
      components[name] = tableEntry;
      continue;
    }

    // 2. Layout shells (page-shell/app-shell/site-header/…) — need slot
    //    fields appended to the generic entity mapping.
    if (isShellType(def.type)) {
      components[name] = buildShellComponent(def);
      continue;
    }

    // 3. Image block with VFS-aware rendering.
    if (def.type === "image") {
      components[name] = buildImageComponent(kernel);
      continue;
    }

    // 4. Button — generic field mapping + real `<button>` render.
    if (def.type === "button") {
      components[name] = buildButtonComponent(def);
      continue;
    }

    // 5. Card — generic field mapping + VFS media + card render.
    if (def.type === "card") {
      components[name] = buildCardComponent(kernel, def);
      continue;
    }

    // 6. Everything else (heading, text-block, section, …) flows through
    //    the generic mapping. Fields with matching media-accept overrides
    //    swap their plain URL field for the VFS upload.
    const generic = entityToPuckComponent({
      type: def.type,
      label: def.label ?? def.type,
      icon: typeof def.icon === "string" ? def.icon : "",
      fields: def.fields ?? [],
    });
    const mediaFieldOverrides: Record<
      string,
      { field: string; accept: "image" | "video" | "audio"; label: string }[]
    > = {
      hero: [
        { field: "backgroundImage", accept: "image", label: "Background image" },
      ],
    };
    const overrides = mediaFieldOverrides[def.type];
    if (overrides && generic.fields) {
      const nextFields = { ...(generic.fields as Record<string, unknown>) };
      for (const o of overrides) {
        if (o.field in nextFields) {
          nextFields[o.field] = mediaUploadField(kernel, {
            label: o.label,
            accept: o.accept,
          });
        }
      }
      generic.fields = nextFields as Fields;
    }
    components[name] = generic;
  }

  // The record-list component isn't registered as an entity — it's a Puck
  // adapter for any record type picked via the `recordType` prop.
  components["RecordList"] = buildRecordListComponent(kernel);

  // Universal style-field merge: every component picks up font/color/
  // padding/etc. unless it already declares its own `fontFamily` field.
  attachStyleFieldsInPlace(components);
  return components;
}

/**
 * Build the Puck root config (fields + defaults + render) for the document
 * root. Fields come from the `page` EntityDef mapped through the same
 * `entityToPuckComponent` pipeline the rest of the canvas uses, plus a
 * `slot` field per `PAGE_SLOTS` entry so authors get native top/left/
 * right/bottom bars without needing a PageShell widget.
 *
 * The returned `render` wraps Puck's flat `children` in `PageShellRenderer`
 * when `layout === "shell"`, and otherwise returns the children untouched.
 * Bar dimensions commit back through `kernel.updateObject` on pointerup
 * via `onCommit` — the `getPageId` thunk lets the caller thread the
 * currently-selected page in reactively.
 */
export function buildPuckRootConfig(
  kernel: StudioKernel,
  getPageId: () => ObjectId | null,
): {
  fields: Fields;
  defaultProps: Record<string, unknown>;
  render: (props: {
    children: ReactNode;
    puck?: unknown;
    [key: string]: unknown;
  }) => ReactNode;
} {
  const rootFields: Record<string, unknown> = {};
  const rootDefaults: Record<string, unknown> = {};

  const pageDef = kernel.registry.get("page");
  if (pageDef) {
    const base = entityToPuckComponent({
      type: "page",
      label: "Page",
      fields: pageDef.fields ?? [],
    });
    Object.assign(rootFields, (base.fields ?? {}) as Record<string, unknown>);
    Object.assign(
      rootDefaults,
      (base.defaultProps ?? {}) as Record<string, unknown>,
    );
  }
  for (const slot of PAGE_SLOTS) {
    rootFields[slot] = { type: "slot" };
    rootDefaults[slot] = [];
  }

  type SlotFn = (props?: Record<string, unknown>) => ReactNode;
  const render = (props: {
    children: ReactNode;
    puck?: unknown;
    [key: string]: unknown;
  }): ReactNode => {
    const layout = (props["layout"] as string) ?? "flow";
    const topBarHeight = Number(props["topBarHeight"] ?? 0);
    const leftBarWidth = Number(props["leftBarWidth"] ?? 0);
    const rightBarWidth = Number(props["rightBarWidth"] ?? 0);
    const bottomBarHeight = Number(props["bottomBarHeight"] ?? 0);
    const stickyTopBar =
      props["stickyTopBar"] !== false && props["stickyTopBar"] !== "false";
    const TopBarSlot = props["topBar"] as SlotFn | undefined;
    const LeftBarSlot = props["leftBar"] as SlotFn | undefined;
    const RightBarSlot = props["rightBar"] as SlotFn | undefined;
    const BottomBarSlot = props["bottomBar"] as SlotFn | undefined;
    const mainContent = (
      <div style={{ position: "relative", minHeight: "100%" }}>
        {props.children}
      </div>
    );
    if (layout === "flow") return mainContent;
    const topBar = typeof TopBarSlot === "function" ? <TopBarSlot /> : null;
    const leftBar = typeof LeftBarSlot === "function" ? <LeftBarSlot /> : null;
    const rightBar =
      typeof RightBarSlot === "function" ? <RightBarSlot /> : null;
    const bottomBar =
      typeof BottomBarSlot === "function" ? <BottomBarSlot /> : null;
    const handleCommit = (key: string, value: number) => {
      const pageId = getPageId();
      if (!pageId) return;
      const obj = kernel.store.getObject(pageId);
      if (!obj) return;
      const currentData = (obj.data ?? {}) as Record<string, unknown>;
      kernel.updateObject(pageId, {
        data: { ...currentData, [key]: value },
      });
    };
    return (
      <PageShellRenderer
        topBarHeight={topBarHeight}
        leftBarWidth={leftBarWidth}
        rightBarWidth={rightBarWidth}
        bottomBarHeight={bottomBarHeight}
        stickyTopBar={stickyTopBar}
        topBar={topBar}
        leftBar={leftBar}
        main={mainContent}
        rightBar={rightBar}
        bottomBar={bottomBar}
        onCommit={handleCommit}
      />
    );
  };

  return {
    fields: rootFields as Fields,
    defaultProps: rootDefaults,
    render,
  };
}
