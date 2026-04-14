/**
 * Page-builder entity and edge type definitions.
 *
 * Registers the object types that Studio's page builder works with.
 * Each type has a category for containment rules and fields for
 * the inspector panel.
 */

import { ObjectRegistry } from "@prism/core/object-model";
import type { EntityDef, EdgeTypeDef, CategoryRule } from "@prism/core/object-model";
import type { LensPuckConfig } from "@prism/core/puck";
import { STYLE_FIELD_DEFS } from "@prism/core/page-builder";

// ── Category Rules ──────────────────────────────────────────────────────────

const categoryRules: CategoryRule[] = [
  { category: "workspace", canParent: ["page"], canBeRoot: true },
  { category: "page", canParent: ["section", "component"], canBeRoot: true },
  { category: "section", canParent: ["component"], canBeRoot: false },
  { category: "component", canParent: [], canBeRoot: false },
  // "record" = free-standing data records (tasks, reminders, contacts, events…)
  // that dynamic widgets query via kernel.store.allObjects(). Records have no
  // children; they live at the workspace root alongside folders and pages.
  { category: "record", canParent: [], canBeRoot: true },
  // "app" = a buildable Prism App (Flux, Cadence, …). Contains an app-shell
  // (outer chrome), routes (URL → page mapping), pages (the actual content),
  // and behaviors (Luau scripts attached to components or routes).
  { category: "app", canParent: ["component", "route", "page", "behavior"], canBeRoot: true },
  // "route" / "behavior" live under an `app` and never contain children.
  { category: "route", canParent: [], canBeRoot: false },
  { category: "behavior", canParent: [], canBeRoot: false },
  // "facet" = a reusable FacetDefinition (form/list/table/report/card
  // projection of an entity type). Lives at the workspace root alongside
  // pages; referenced by facet-view and spatial-canvas components.
  { category: "facet", canParent: [], canBeRoot: true },
];

// ── Entity Definitions ──────────────────────────────────────────────────────

const folderDef: EntityDef<string, LensPuckConfig> = {
  type: "folder",
  category: "workspace",
  label: "Folder",
  pluralLabel: "Folders",
  icon: "\uD83D\uDCC1",
  color: "#e8a838",
  fields: [],
};

const pageDef: EntityDef<string, LensPuckConfig> = {
  type: "page",
  category: "page",
  label: "Page",
  pluralLabel: "Pages",
  icon: "\uD83D\uDCC4",
  color: "#4a9eff",
  defaultChildView: "list",
  fields: [
    { id: "title", type: "string", label: "Title", required: true },
    { id: "slug", type: "string", label: "URL Slug", ui: { placeholder: "/about" } },
    {
      id: "layout",
      type: "enum",
      label: "Layout",
      default: "flow",
      enumOptions: [
        { value: "flow", label: "Flow (stacked content)" },
        { value: "shell", label: "Shell (resizable bars)" },
      ],
      ui: { group: "Layout" },
    },
    { id: "topBarHeight", type: "int", label: "Top Bar Height (px)", default: 0, ui: { group: "Layout" } },
    { id: "leftBarWidth", type: "int", label: "Left Bar Width (px)", default: 0, ui: { group: "Layout" } },
    { id: "rightBarWidth", type: "int", label: "Right Bar Width (px)", default: 0, ui: { group: "Layout" } },
    { id: "bottomBarHeight", type: "int", label: "Bottom Bar Height (px)", default: 0, ui: { group: "Layout" } },
    { id: "stickyTopBar", type: "bool", label: "Sticky Top Bar", default: true, ui: { group: "Layout" } },
    { id: "published", type: "bool", label: "Published", default: false },
    { id: "publishedAt", type: "datetime", label: "Published At" },
    { id: "metaDescription", type: "text", label: "Meta Description", ui: { multiline: true, group: "SEO" } },
  ],
};

const sectionDef: EntityDef<string, LensPuckConfig> = {
  type: "section",
  category: "section",
  label: "Section",
  pluralLabel: "Sections",
  icon: "\u25A8",
  color: "#8b5cf6",
  childOnly: true,
  fields: [
    {
      id: "variant",
      type: "enum",
      label: "Variant",
      default: "default",
      enumOptions: [
        { value: "default", label: "Default" },
        { value: "hero", label: "Hero" },
        { value: "cta", label: "Call to Action" },
        { value: "grid", label: "Grid" },
        { value: "columns", label: "Columns" },
      ],
    },
    ...STYLE_FIELD_DEFS,
  ],
};

const headingDef: EntityDef<string, LensPuckConfig> = {
  type: "heading",
  category: "component",
  label: "Heading",
  pluralLabel: "Headings",
  icon: "H",
  color: "#f59e0b",
  childOnly: true,
  fields: [
    { id: "text", type: "string", label: "Text", required: true },
    {
      id: "level",
      type: "enum",
      label: "Level",
      default: "h2",
      enumOptions: [
        { value: "h1", label: "H1" },
        { value: "h2", label: "H2" },
        { value: "h3", label: "H3" },
        { value: "h4", label: "H4" },
      ],
    },
    { id: "align", type: "enum", label: "Alignment", default: "left", enumOptions: [
      { value: "left", label: "Left" },
      { value: "center", label: "Center" },
      { value: "right", label: "Right" },
    ]},
    ...STYLE_FIELD_DEFS,
  ],
};

const textBlockDef: EntityDef<string, LensPuckConfig> = {
  type: "text-block",
  category: "component",
  label: "Text Block",
  pluralLabel: "Text Blocks",
  icon: "\u00B6",
  color: "#6b7280",
  childOnly: true,
  fields: [
    { id: "content", type: "text", label: "Content", required: true, ui: { multiline: true } },
    { id: "format", type: "enum", label: "Format", default: "markdown", enumOptions: [
      { value: "plain", label: "Plain Text" },
      { value: "markdown", label: "Markdown" },
    ]},
    ...STYLE_FIELD_DEFS,
  ],
};

const imageDef: EntityDef<string, LensPuckConfig> = {
  type: "image",
  category: "component",
  label: "Image",
  pluralLabel: "Images",
  icon: "\uD83D\uDDBC",
  color: "#10b981",
  childOnly: true,
  fields: [
    { id: "src", type: "url", label: "Source URL", required: true },
    { id: "alt", type: "string", label: "Alt Text" },
    { id: "caption", type: "string", label: "Caption" },
    { id: "width", type: "int", label: "Width (px)" },
    { id: "height", type: "int", label: "Height (px)" },
  ],
};

const buttonDef: EntityDef<string, LensPuckConfig> = {
  type: "button",
  category: "component",
  label: "Button",
  pluralLabel: "Buttons",
  icon: "\u25A3",
  color: "#3b82f6",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label", required: true, default: "Click me" },
    { id: "href", type: "url", label: "Link URL" },
    { id: "variant", type: "enum", label: "Variant", default: "primary", enumOptions: [
      { value: "primary", label: "Primary" },
      { value: "secondary", label: "Secondary" },
      { value: "outline", label: "Outline" },
      { value: "ghost", label: "Ghost" },
      { value: "danger", label: "Danger" },
      { value: "success", label: "Success" },
      { value: "gradient", label: "Gradient" },
    ]},
    { id: "size", type: "enum", label: "Size", default: "md", enumOptions: [
      { value: "xs", label: "Extra Small" },
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
      { value: "xl", label: "Extra Large" },
    ]},
    { id: "icon", type: "string", label: "Icon", ui: { placeholder: "→ or ✓ or emoji", group: "Content" } },
    { id: "iconPosition", type: "enum", label: "Icon Position", default: "left", enumOptions: [
      { value: "left", label: "Left" },
      { value: "right", label: "Right" },
    ], ui: { group: "Content" }},
    { id: "fullWidth", type: "bool", label: "Full Width", default: false, ui: { group: "Layout" } },
    { id: "disabled", type: "bool", label: "Disabled", default: false, ui: { group: "State" } },
    { id: "loading", type: "bool", label: "Loading", default: false, ui: { group: "State" } },
    { id: "rounded", type: "enum", label: "Rounded", default: "md", enumOptions: [
      { value: "none", label: "Square" },
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
      { value: "full", label: "Pill" },
    ], ui: { group: "Appearance" }},
    { id: "shadow", type: "enum", label: "Shadow", default: "none", enumOptions: [
      { value: "none", label: "None" },
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
    ], ui: { group: "Appearance" }},
    { id: "hoverEffect", type: "enum", label: "Hover Effect", default: "none", enumOptions: [
      { value: "none", label: "None" },
      { value: "lift", label: "Lift" },
      { value: "glow", label: "Glow" },
      { value: "scale", label: "Scale" },
    ], ui: { group: "Appearance" }},
    { id: "target", type: "enum", label: "Link Target", default: "_self", enumOptions: [
      { value: "_self", label: "Same tab" },
      { value: "_blank", label: "New tab" },
      { value: "_parent", label: "Parent frame" },
      { value: "_top", label: "Top frame" },
    ], ui: { group: "Link" }},
    { id: "rel", type: "string", label: "Link rel", ui: { placeholder: "noopener noreferrer", group: "Link" } },
    { id: "buttonType", type: "enum", label: "Button Type", default: "button", enumOptions: [
      { value: "button", label: "Button" },
      { value: "submit", label: "Submit" },
      { value: "reset", label: "Reset" },
    ], ui: { group: "Advanced" }},
    { id: "ariaLabel", type: "string", label: "Aria Label", ui: { group: "Advanced" } },
    ...STYLE_FIELD_DEFS,
  ],
};

const cardDef: EntityDef<string, LensPuckConfig> = {
  type: "card",
  category: "component",
  label: "Card",
  pluralLabel: "Cards",
  icon: "\u25A1",
  color: "#8b5cf6",
  childOnly: true,
  fields: [
    { id: "eyebrow", type: "string", label: "Eyebrow", ui: { placeholder: "NEW · FEATURE · ...", group: "Content" } },
    { id: "title", type: "string", label: "Title", ui: { group: "Content" } },
    { id: "body", type: "text", label: "Body", ui: { multiline: true, group: "Content" } },
    { id: "imageUrl", type: "url", label: "Image URL", ui: { group: "Media" } },
    { id: "mediaFit", type: "enum", label: "Media Fit", default: "cover", enumOptions: [
      { value: "cover", label: "Cover" },
      { value: "contain", label: "Contain" },
    ], ui: { group: "Media" }},
    { id: "mediaAspectRatio", type: "string", label: "Media Aspect Ratio", ui: { placeholder: "16 / 9", group: "Media" } },
    { id: "variant", type: "enum", label: "Variant", default: "elevated", enumOptions: [
      { value: "elevated", label: "Elevated" },
      { value: "outlined", label: "Outlined" },
      { value: "filled", label: "Filled" },
      { value: "ghost", label: "Ghost" },
    ], ui: { group: "Appearance" }},
    { id: "layout", type: "enum", label: "Layout", default: "vertical", enumOptions: [
      { value: "vertical", label: "Vertical" },
      { value: "horizontal", label: "Horizontal" },
      { value: "overlay", label: "Overlay" },
    ], ui: { group: "Appearance" }},
    { id: "hoverEffect", type: "enum", label: "Hover Effect", default: "lift", enumOptions: [
      { value: "none", label: "None" },
      { value: "lift", label: "Lift" },
      { value: "glow", label: "Glow" },
    ], ui: { group: "Appearance" }},
    { id: "overlayOpacity", type: "float", label: "Overlay Opacity", default: 0.55, ui: { group: "Appearance" } },
    { id: "linkUrl", type: "url", label: "Link URL", ui: { group: "CTA" } },
    { id: "ctaLabel", type: "string", label: "CTA Label", ui: { group: "CTA" } },
    { id: "ctaVariant", type: "enum", label: "CTA Variant", default: "primary", enumOptions: [
      { value: "primary", label: "Primary" },
      { value: "secondary", label: "Secondary" },
      { value: "outline", label: "Outline" },
      { value: "ghost", label: "Ghost" },
      { value: "gradient", label: "Gradient" },
    ], ui: { group: "CTA" }},
    ...STYLE_FIELD_DEFS,
  ],
};

const luauBlockDef: EntityDef<string, LensPuckConfig> = {
  type: "luau-block",
  category: "component",
  label: "Luau Block",
  pluralLabel: "Luau Blocks",
  icon: "\uD83C\uDF19",
  color: "#06b6d4",
  childOnly: true,
  fields: [
    { id: "source", type: "text", label: "Luau Source", required: true, ui: { multiline: true } },
    { id: "title", type: "string", label: "Title" },
  ],
};

// ── Data-Aware Components (FileMaker Pro parity) ───────────────────────────

const facetViewDef: EntityDef<string, LensPuckConfig> = {
  type: "facet-view",
  category: "component",
  label: "Facet View",
  pluralLabel: "Facet Views",
  icon: "\uD83D\uDCCB",
  color: "#10b981",
  childOnly: true,
  fields: [
    { id: "facetId", type: "string", label: "Facet Definition ID", required: true },
    {
      id: "viewMode",
      type: "enum",
      label: "View Mode",
      default: "form",
      enumOptions: [
        { value: "form", label: "Form" },
        { value: "list", label: "List" },
        { value: "table", label: "Table" },
        { value: "report", label: "Report" },
        { value: "card", label: "Card" },
      ],
    },
    { id: "maxRows", type: "int", label: "Max Rows", default: 25 },
  ],
};

const spatialCanvasDef: EntityDef<string, LensPuckConfig> = {
  type: "spatial-canvas",
  category: "component",
  label: "Spatial Canvas",
  pluralLabel: "Spatial Canvases",
  icon: "\uD83D\uDCD0",
  color: "#f97316",
  childOnly: true,
  fields: [
    { id: "facetId", type: "string", label: "Facet Definition ID", required: true },
    { id: "canvasWidth", type: "int", label: "Canvas Width (pt)", default: 612 },
    { id: "canvasHeight", type: "int", label: "Canvas Height (pt)", default: 400 },
    { id: "gridSize", type: "int", label: "Grid Size (pt)", default: 8 },
    {
      id: "showGrid",
      type: "bool",
      label: "Show Grid",
      default: true,
    },
  ],
};

const dataPortalDef: EntityDef<string, LensPuckConfig> = {
  type: "data-portal",
  category: "component",
  label: "Data Portal",
  pluralLabel: "Data Portals",
  icon: "\uD83D\uDD17",
  color: "#8b5cf6",
  childOnly: true,
  fields: [
    { id: "relationshipId", type: "string", label: "Relationship Edge Type", required: true },
    { id: "displayFields", type: "string", label: "Display Fields (comma-separated)" },
    { id: "visibleRows", type: "int", label: "Visible Rows", default: 5 },
    { id: "allowCreation", type: "bool", label: "Allow Inline Creation", default: false },
    { id: "sortField", type: "string", label: "Sort Field" },
    {
      id: "sortDirection",
      type: "enum",
      label: "Sort Direction",
      default: "asc",
      enumOptions: [
        { value: "asc", label: "Ascending" },
        { value: "desc", label: "Descending" },
      ],
    },
  ],
};

// ── Data-Aware Widgets (Puck-draggable visualisations) ─────────────────────

const recordListDef: EntityDef<string, LensPuckConfig> = {
  type: "record-list",
  category: "component",
  label: "Record List",
  pluralLabel: "Record Lists",
  icon: "\uD83D\uDCDC",
  color: "#14b8a6",
  childOnly: true,
  description:
    "Parametric list over kernel records. One widget replaces tasks/events/notes/etc via a filter/sort spec and a row template.",
  fields: [
    {
      id: "recordType",
      type: "string",
      label: "Record Type",
      required: true,
      ui: { placeholder: "task | event | note | *" },
    },
    {
      id: "titleField",
      type: "string",
      label: "Title Field",
      default: "name",
    },
    {
      id: "subtitleField",
      type: "string",
      label: "Subtitle Field",
      default: "description",
    },
    {
      id: "metaFields",
      type: "text",
      label: "Meta Fields",
      default: "status:badge, date:date",
      ui: {
        multiline: true,
        placeholder:
          "field:kind, field:kind  (kind: text | date | badge | status | tags)",
      },
    },
    {
      id: "filterExpression",
      type: "text",
      label: "Filter Expression",
      ui: {
        multiline: true,
        placeholder: "status eq open; priority in high,urgent",
      },
    },
    {
      id: "sortField",
      type: "string",
      label: "Sort Field",
      default: "updatedAt",
    },
    {
      id: "sortDir",
      type: "enum",
      label: "Sort Direction",
      default: "desc",
      enumOptions: [
        { value: "asc", label: "Ascending" },
        { value: "desc", label: "Descending" },
      ],
    },
    { id: "limit", type: "int", label: "Limit", default: 50 },
    {
      id: "emptyMessage",
      type: "string",
      label: "Empty Message",
      default: "No records to display.",
    },
  ],
};

const kanbanWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "kanban-widget",
  category: "component",
  label: "Kanban Widget",
  pluralLabel: "Kanban Widgets",
  icon: "\uD83D\uDCCB",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    { id: "groupField", type: "string", label: "Group Field", default: "status" },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    { id: "colorField", type: "string", label: "Color Field" },
    { id: "maxCardsPerColumn", type: "int", label: "Max Cards / Column", default: 50 },
  ],
};

const listWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "list-widget",
  category: "component",
  label: "List Widget",
  pluralLabel: "List Widgets",
  icon: "\u2630",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    { id: "subtitleField", type: "string", label: "Subtitle Field", default: "type" },
    { id: "showStatus", type: "bool", label: "Show Status", default: true },
    { id: "showTimestamp", type: "bool", label: "Show Timestamp", default: true },
  ],
};

const tableWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "table-widget",
  category: "component",
  label: "Table Widget",
  pluralLabel: "Table Widgets",
  icon: "\u2637",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    {
      id: "columns",
      type: "text",
      label: "Columns",
      default: "name:Name, type:Type, status:Status, updatedAt:Updated",
      ui: { multiline: true, placeholder: "name:Name, status:Status, data.priority:Priority" },
    },
    { id: "sortField", type: "string", label: "Default Sort Field", default: "name" },
    {
      id: "sortDir",
      type: "enum",
      label: "Default Sort Direction",
      default: "asc",
      enumOptions: [
        { value: "asc", label: "Ascending" },
        { value: "desc", label: "Descending" },
      ],
    },
  ],
};

const cardGridWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "card-grid-widget",
  category: "component",
  label: "Card Grid Widget",
  pluralLabel: "Card Grid Widgets",
  icon: "\u25A6",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    { id: "subtitleField", type: "string", label: "Subtitle Field", default: "type" },
    { id: "minColumnWidth", type: "int", label: "Min Column Width (px)", default: 220 },
    { id: "showStatus", type: "bool", label: "Show Status", default: true },
  ],
};

const reportWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "report-widget",
  category: "component",
  label: "Report Widget",
  pluralLabel: "Report Widgets",
  icon: "\uD83D\uDCCB",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    { id: "groupField", type: "string", label: "Group By Field", default: "type" },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    { id: "valueField", type: "string", label: "Value Field" },
    {
      id: "aggregation",
      type: "enum",
      label: "Aggregation",
      default: "count",
      enumOptions: [
        { value: "count", label: "Count" },
        { value: "sum", label: "Sum" },
        { value: "avg", label: "Average" },
        { value: "min", label: "Min" },
        { value: "max", label: "Max" },
      ],
    },
  ],
};

const calendarWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "calendar-widget",
  category: "component",
  label: "Calendar Widget",
  pluralLabel: "Calendar Widgets",
  icon: "\uD83D\uDCC5",
  color: "#22c55e",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "event" } },
    { id: "dateField", type: "string", label: "Date Field", default: "date" },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    {
      id: "viewType",
      type: "enum",
      label: "View",
      default: "month",
      enumOptions: [
        { value: "month", label: "Month" },
        { value: "week", label: "Week" },
        { value: "day", label: "Day" },
      ],
    },
  ],
};

const chartWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "chart-widget",
  category: "component",
  label: "Chart Widget",
  pluralLabel: "Chart Widgets",
  icon: "\uD83D\uDCCA",
  color: "#a855f7",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "sale" } },
    {
      id: "chartType",
      type: "enum",
      label: "Chart Type",
      default: "bar",
      enumOptions: [
        { value: "bar", label: "Bar" },
        { value: "line", label: "Line" },
        { value: "pie", label: "Pie" },
        { value: "area", label: "Area" },
      ],
    },
    { id: "groupField", type: "string", label: "Group By Field", required: true },
    { id: "valueField", type: "string", label: "Value Field" },
    {
      id: "aggregation",
      type: "enum",
      label: "Aggregation",
      default: "count",
      enumOptions: [
        { value: "count", label: "Count" },
        { value: "sum", label: "Sum" },
        { value: "avg", label: "Average" },
        { value: "min", label: "Min" },
        { value: "max", label: "Max" },
      ],
    },
  ],
};

const mapWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "map-widget",
  category: "component",
  label: "Map Widget",
  pluralLabel: "Map Widgets",
  icon: "\uD83D\uDDFA",
  color: "#059669",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "place" } },
    { id: "latField", type: "string", label: "Latitude Field", default: "lat" },
    { id: "lngField", type: "string", label: "Longitude Field", default: "lng" },
    { id: "titleField", type: "string", label: "Title Field", default: "name" },
    { id: "initialZoom", type: "int", label: "Initial Zoom", default: 10 },
  ],
};

const tabContainerDef: EntityDef<string, LensPuckConfig> = {
  type: "tab-container",
  category: "component",
  label: "Tab Container",
  pluralLabel: "Tab Containers",
  icon: "\uD83D\uDDC2",
  color: "#f97316",
  childOnly: true,
  fields: [
    { id: "tabs", type: "string", label: "Tab Labels (comma-separated)", required: true, default: "Tab 1,Tab 2" },
    { id: "activeTab", type: "int", label: "Default Active Tab", default: 0 },
  ],
};

const popoverWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "popover-widget",
  category: "component",
  label: "Popover",
  pluralLabel: "Popovers",
  icon: "\u2699",
  color: "#ec4899",
  childOnly: true,
  fields: [
    { id: "triggerLabel", type: "string", label: "Trigger Label", required: true, default: "Open" },
    { id: "content", type: "text", label: "Content", ui: { multiline: true } },
  ],
};

const slidePanelDef: EntityDef<string, LensPuckConfig> = {
  type: "slide-panel",
  category: "component",
  label: "Slide Panel",
  pluralLabel: "Slide Panels",
  icon: "\u2630",
  color: "#6366f1",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label", required: true, default: "Details" },
    { id: "content", type: "text", label: "Content", ui: { multiline: true } },
    { id: "collapsed", type: "bool", label: "Start Collapsed", default: false },
  ],
};

// ── Form Input Widgets ─────────────────────────────────────────────────────

const textInputDef: EntityDef<string, LensPuckConfig> = {
  type: "text-input",
  category: "component",
  label: "Text Input",
  pluralLabel: "Text Inputs",
  icon: "\u270E",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    { id: "placeholder", type: "string", label: "Placeholder" },
    { id: "defaultValue", type: "string", label: "Default Value" },
    {
      id: "inputType",
      type: "enum",
      label: "Input Type",
      default: "text",
      enumOptions: [
        { value: "text", label: "Text" },
        { value: "email", label: "Email" },
        { value: "url", label: "URL" },
        { value: "tel", label: "Phone" },
        { value: "password", label: "Password" },
      ],
    },
    { id: "required", type: "bool", label: "Required", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

const textareaInputDef: EntityDef<string, LensPuckConfig> = {
  type: "textarea-input",
  category: "component",
  label: "Textarea",
  pluralLabel: "Textareas",
  icon: "\u00B6",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    { id: "placeholder", type: "string", label: "Placeholder" },
    { id: "defaultValue", type: "text", label: "Default Value", ui: { multiline: true } },
    { id: "rows", type: "int", label: "Rows", default: 4 },
    { id: "required", type: "bool", label: "Required", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

const selectInputDef: EntityDef<string, LensPuckConfig> = {
  type: "select-input",
  category: "component",
  label: "Select",
  pluralLabel: "Selects",
  icon: "\u25BE",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    {
      id: "options",
      type: "text",
      label: "Options (CSV or JSON)",
      default: "one,two,three",
      ui: { multiline: true, placeholder: "value:Label, another:Another" },
    },
    { id: "defaultValue", type: "string", label: "Default Value" },
    { id: "required", type: "bool", label: "Required", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

const checkboxInputDef: EntityDef<string, LensPuckConfig> = {
  type: "checkbox-input",
  category: "component",
  label: "Checkbox",
  pluralLabel: "Checkboxes",
  icon: "\u2611",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label", default: "Accept" },
    { id: "defaultChecked", type: "bool", label: "Checked", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

const numberInputDef: EntityDef<string, LensPuckConfig> = {
  type: "number-input",
  category: "component",
  label: "Number Input",
  pluralLabel: "Number Inputs",
  icon: "#",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    { id: "defaultValue", type: "float", label: "Default Value" },
    { id: "min", type: "float", label: "Min" },
    { id: "max", type: "float", label: "Max" },
    { id: "step", type: "float", label: "Step" },
    { id: "required", type: "bool", label: "Required", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

const dateInputDef: EntityDef<string, LensPuckConfig> = {
  type: "date-input",
  category: "component",
  label: "Date Input",
  pluralLabel: "Date Inputs",
  icon: "\uD83D\uDCC6",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    { id: "defaultValue", type: "string", label: "Default Value (ISO)" },
    {
      id: "dateKind",
      type: "enum",
      label: "Kind",
      default: "date",
      enumOptions: [
        { value: "date", label: "Date" },
        { value: "datetime-local", label: "Date + Time" },
        { value: "time", label: "Time" },
      ],
    },
    { id: "required", type: "bool", label: "Required", default: false },
    { id: "help", type: "string", label: "Help Text" },
  ],
};

// ── Layout Primitives ──────────────────────────────────────────────────────

const columnsDef: EntityDef<string, LensPuckConfig> = {
  type: "columns",
  category: "component",
  label: "Columns",
  pluralLabel: "Columns",
  icon: "\u2551",
  color: "#6366f1",
  childOnly: true,
  fields: [
    { id: "columnCount", type: "int", label: "Column Count", default: 2 },
    { id: "gap", type: "int", label: "Gap (px)", default: 16 },
    {
      id: "align",
      type: "enum",
      label: "Align Items",
      default: "stretch",
      enumOptions: [
        { value: "start", label: "Start" },
        { value: "center", label: "Center" },
        { value: "end", label: "End" },
        { value: "stretch", label: "Stretch" },
      ],
    },
  ],
};

const dividerDef: EntityDef<string, LensPuckConfig> = {
  type: "divider",
  category: "component",
  label: "Divider",
  pluralLabel: "Dividers",
  icon: "\u2014",
  color: "#94a3b8",
  childOnly: true,
  fields: [
    {
      id: "dividerStyle",
      type: "enum",
      label: "Style",
      default: "solid",
      enumOptions: [
        { value: "solid", label: "Solid" },
        { value: "dashed", label: "Dashed" },
        { value: "dotted", label: "Dotted" },
      ],
    },
    { id: "thickness", type: "int", label: "Thickness (px)", default: 1 },
    { id: "color", type: "color", label: "Color", default: "#cbd5e1" },
    { id: "spacing", type: "int", label: "Spacing (px)", default: 12 },
    { id: "label", type: "string", label: "Label" },
  ],
};

const spacerDef: EntityDef<string, LensPuckConfig> = {
  type: "spacer",
  category: "component",
  label: "Spacer",
  pluralLabel: "Spacers",
  icon: "\u2B1A",
  color: "#94a3b8",
  childOnly: true,
  fields: [
    { id: "size", type: "int", label: "Size (px)", default: 16 },
    {
      id: "axis",
      type: "enum",
      label: "Axis",
      default: "vertical",
      enumOptions: [
        { value: "vertical", label: "Vertical" },
        { value: "horizontal", label: "Horizontal" },
      ],
    },
  ],
};

// ── Wix-style layout primitives ────────────────────────────────────────────

const pageShellDef: EntityDef<string, LensPuckConfig> = {
  type: "page-shell",
  category: "component",
  label: "Page Shell",
  pluralLabel: "Page Shells",
  icon: "\u25A6",
  color: "#0ea5e9",
  fields: [
    { id: "topBarHeight", type: "int", label: "Top Bar Height (px)", default: 0 },
    { id: "leftBarWidth", type: "int", label: "Left Bar Width (px)", default: 0 },
    { id: "rightBarWidth", type: "int", label: "Right Bar Width (px)", default: 0 },
    { id: "bottomBarHeight", type: "int", label: "Bottom Bar Height (px)", default: 0 },
    { id: "stickyTopBar", type: "bool", label: "Sticky Top Bar", default: true },
    ...STYLE_FIELD_DEFS,
  ],
};

const siteHeaderDef: EntityDef<string, LensPuckConfig> = {
  type: "site-header",
  category: "component",
  label: "Site Header",
  pluralLabel: "Site Headers",
  icon: "\u2630",
  color: "#0ea5e9",
  fields: [
    { id: "brand", type: "string", label: "Brand", default: "Your Brand" },
    { id: "tagline", type: "string", label: "Tagline" },
    { id: "sticky", type: "bool", label: "Sticky", default: false },
    ...STYLE_FIELD_DEFS,
  ],
};

const siteFooterDef: EntityDef<string, LensPuckConfig> = {
  type: "site-footer",
  category: "component",
  label: "Site Footer",
  pluralLabel: "Site Footers",
  icon: "\u2500",
  color: "#0ea5e9",
  fields: [
    { id: "copyright", type: "string", label: "Copyright", default: "© Your Brand" },
    ...STYLE_FIELD_DEFS,
  ],
};

const sideBarDef: EntityDef<string, LensPuckConfig> = {
  type: "side-bar",
  category: "component",
  label: "Bar",
  pluralLabel: "Bars",
  icon: "\u2590",
  color: "#0ea5e9",
  fields: [
    { id: "width", type: "int", label: "Size (px)", default: 260 },
    {
      id: "position",
      type: "enum",
      label: "Position",
      default: "left",
      enumOptions: [
        { value: "left", label: "Left" },
        { value: "right", label: "Right" },
        { value: "top", label: "Top" },
        { value: "bottom", label: "Bottom" },
      ],
    },
    ...STYLE_FIELD_DEFS,
  ],
};

const navBarDef: EntityDef<string, LensPuckConfig> = {
  type: "nav-bar",
  category: "component",
  label: "Nav Bar",
  pluralLabel: "Nav Bars",
  icon: "\u2630",
  color: "#0ea5e9",
  fields: [
    {
      id: "align",
      type: "enum",
      label: "Alignment",
      default: "start",
      enumOptions: [
        { value: "start", label: "Start" },
        { value: "center", label: "Center" },
        { value: "end", label: "End" },
      ],
    },
    ...STYLE_FIELD_DEFS,
  ],
};

const heroDef: EntityDef<string, LensPuckConfig> = {
  type: "hero",
  category: "component",
  label: "Hero",
  pluralLabel: "Heroes",
  icon: "\u2605",
  color: "#0ea5e9",
  fields: [
    {
      id: "align",
      type: "enum",
      label: "Alignment",
      default: "center",
      enumOptions: [
        { value: "left", label: "Left" },
        { value: "center", label: "Center" },
        { value: "right", label: "Right" },
      ],
    },
    { id: "minHeight", type: "int", label: "Min Height (px)", default: 360 },
    { id: "backgroundImage", type: "url", label: "Background Image" },
    ...STYLE_FIELD_DEFS,
  ],
};

// ── Data Display Widgets ───────────────────────────────────────────────────

const statWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "stat-widget",
  category: "component",
  label: "Stat (KPI)",
  pluralLabel: "Stats",
  icon: "\u2116",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "collectionType", type: "string", label: "Object Type", required: true, ui: { placeholder: "task" } },
    { id: "label", type: "string", label: "Label", default: "Total" },
    {
      id: "aggregation",
      type: "enum",
      label: "Aggregation",
      default: "count",
      enumOptions: [
        { value: "count", label: "Count" },
        { value: "sum", label: "Sum" },
        { value: "avg", label: "Average" },
        { value: "min", label: "Min" },
        { value: "max", label: "Max" },
      ],
    },
    { id: "valueField", type: "string", label: "Value Field" },
    { id: "prefix", type: "string", label: "Prefix" },
    { id: "suffix", type: "string", label: "Suffix" },
    { id: "decimals", type: "int", label: "Decimals", default: 0 },
    { id: "thousands", type: "bool", label: "Thousands Separator", default: true },
  ],
};

const badgeDef: EntityDef<string, LensPuckConfig> = {
  type: "badge",
  category: "component",
  label: "Badge",
  pluralLabel: "Badges",
  icon: "\u25CF",
  color: "#22c55e",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label", required: true, default: "New" },
    {
      id: "tone",
      type: "enum",
      label: "Tone",
      default: "neutral",
      enumOptions: [
        { value: "neutral", label: "Neutral" },
        { value: "info", label: "Info" },
        { value: "success", label: "Success" },
        { value: "warning", label: "Warning" },
        { value: "danger", label: "Danger" },
      ],
    },
    { id: "icon", type: "string", label: "Icon (emoji)" },
    { id: "outline", type: "bool", label: "Outline", default: false },
  ],
};

const alertDef: EntityDef<string, LensPuckConfig> = {
  type: "alert",
  category: "component",
  label: "Alert",
  pluralLabel: "Alerts",
  icon: "\u26A0",
  color: "#f59e0b",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Title" },
    { id: "message", type: "text", label: "Message", required: true, default: "Notice.", ui: { multiline: true } },
    {
      id: "tone",
      type: "enum",
      label: "Tone",
      default: "info",
      enumOptions: [
        { value: "neutral", label: "Neutral" },
        { value: "info", label: "Info" },
        { value: "success", label: "Success" },
        { value: "warning", label: "Warning" },
        { value: "danger", label: "Danger" },
      ],
    },
    { id: "icon", type: "string", label: "Icon (emoji)" },
  ],
};

const progressBarDef: EntityDef<string, LensPuckConfig> = {
  type: "progress-bar",
  category: "component",
  label: "Progress Bar",
  pluralLabel: "Progress Bars",
  icon: "\u25B0",
  color: "#22c55e",
  childOnly: true,
  fields: [
    { id: "label", type: "string", label: "Label" },
    { id: "value", type: "float", label: "Value", default: 50 },
    { id: "max", type: "float", label: "Max", default: 100 },
    {
      id: "tone",
      type: "enum",
      label: "Tone",
      default: "info",
      enumOptions: [
        { value: "neutral", label: "Neutral" },
        { value: "info", label: "Info" },
        { value: "success", label: "Success" },
        { value: "warning", label: "Warning" },
        { value: "danger", label: "Danger" },
      ],
    },
    { id: "showPercent", type: "bool", label: "Show Percent", default: true },
  ],
};

// ── Content Widgets ────────────────────────────────────────────────────────

const markdownWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "markdown-widget",
  category: "component",
  label: "Markdown",
  pluralLabel: "Markdown Blocks",
  icon: "M",
  color: "#0f172a",
  childOnly: true,
  fields: [
    {
      id: "source",
      type: "text",
      label: "Markdown",
      required: true,
      default: "# Heading\n\nSome **bold** content.",
      ui: { multiline: true },
    },
  ],
};

const iframeWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "iframe-widget",
  category: "component",
  label: "Embed (iframe)",
  pluralLabel: "Embeds",
  icon: "\u25A2",
  color: "#64748b",
  childOnly: true,
  fields: [
    { id: "src", type: "url", label: "URL", required: true },
    { id: "title", type: "string", label: "Title", default: "Embedded content" },
    { id: "height", type: "int", label: "Height (px)", default: 360 },
    { id: "allowFullscreen", type: "bool", label: "Allow Fullscreen", default: true },
  ],
};

const codeBlockDef: EntityDef<string, LensPuckConfig> = {
  type: "code-block",
  category: "component",
  label: "Code Block",
  pluralLabel: "Code Blocks",
  icon: "\u003C\u003E",
  color: "#a78bfa",
  childOnly: true,
  fields: [
    {
      id: "source",
      type: "text",
      label: "Source",
      required: true,
      default: "function hello() {\n  return \"world\";\n}",
      ui: { multiline: true },
    },
    {
      id: "language",
      type: "enum",
      label: "Language",
      default: "typescript",
      enumOptions: [
        { value: "typescript", label: "TypeScript" },
        { value: "javascript", label: "JavaScript" },
        { value: "json", label: "JSON" },
        { value: "luau", label: "Luau" },
        { value: "rust", label: "Rust" },
        { value: "python", label: "Python" },
        { value: "bash", label: "Bash" },
        { value: "yaml", label: "YAML" },
        { value: "markdown", label: "Markdown" },
        { value: "text", label: "Plain Text" },
      ],
    },
    { id: "caption", type: "string", label: "Caption" },
    { id: "lineNumbers", type: "bool", label: "Line Numbers", default: true },
    { id: "wrap", type: "bool", label: "Wrap Long Lines", default: false },
  ],
};

const videoWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "video-widget",
  category: "component",
  label: "Video",
  pluralLabel: "Videos",
  icon: "\uD83C\uDFAC",
  color: "#ef4444",
  childOnly: true,
  fields: [
    { id: "src", type: "url", label: "Video URL", required: true },
    { id: "poster", type: "url", label: "Poster URL" },
    { id: "caption", type: "string", label: "Caption" },
    { id: "width", type: "int", label: "Width (px)", default: 640 },
    { id: "height", type: "int", label: "Height (px)", default: 360 },
    { id: "controls", type: "bool", label: "Show Controls", default: true },
    { id: "autoplay", type: "bool", label: "Autoplay", default: false },
    { id: "loop", type: "bool", label: "Loop", default: false },
    { id: "muted", type: "bool", label: "Muted", default: false },
  ],
};

const audioWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "audio-widget",
  category: "component",
  label: "Audio",
  pluralLabel: "Audio Clips",
  icon: "\uD83D\uDD0A",
  color: "#f97316",
  childOnly: true,
  fields: [
    { id: "src", type: "url", label: "Audio URL", required: true },
    { id: "caption", type: "string", label: "Caption" },
    { id: "controls", type: "bool", label: "Show Controls", default: true },
    { id: "autoplay", type: "bool", label: "Autoplay", default: false },
    { id: "loop", type: "bool", label: "Loop", default: false },
    { id: "muted", type: "bool", label: "Muted", default: false },
  ],
};

// ── Navigation Widgets ─────────────────────────────────────────────────────

const siteNavDef: EntityDef<string, LensPuckConfig> = {
  type: "site-nav",
  category: "component",
  label: "Site Nav",
  pluralLabel: "Site Navs",
  icon: "\u{1F5FA}",
  color: "#6366f1",
  childOnly: true,
  fields: [
    {
      id: "layout",
      type: "enum",
      label: "Layout",
      default: "horizontal",
      enumOptions: [
        { value: "horizontal", label: "Horizontal" },
        { value: "vertical", label: "Vertical" },
      ],
    },
    {
      id: "source",
      type: "enum",
      label: "Source",
      default: "pages",
      enumOptions: [
        { value: "pages", label: "All pages in site" },
        { value: "manual", label: "Manual links" },
      ],
    },
    {
      id: "links",
      type: "text",
      label: "Manual Links (label|url per line)",
      ui: { multiline: true, placeholder: "Home|/\nAbout|/about" },
    },
    { id: "showIcons", type: "bool", label: "Show Icons", default: false },
  ],
};

const breadcrumbsDef: EntityDef<string, LensPuckConfig> = {
  type: "breadcrumbs",
  category: "component",
  label: "Breadcrumbs",
  pluralLabel: "Breadcrumbs",
  icon: "\u{1F4CD}",
  color: "#6366f1",
  childOnly: true,
  fields: [
    { id: "separator", type: "string", label: "Separator", default: "/" },
    { id: "showHome", type: "bool", label: "Show Home", default: true },
  ],
};

// ── Dynamic Data Records ───────────────────────────────────────────────────
//
// Free-standing records that live at the workspace root. `name`, `description`,
// `status`, `date`, `endDate`, `tags`, `pinned`, `color` come from the
// GraphObject shell (editable in the inspector's shell section). Only
// domain-specific props are declared here and stored in `data`.

const taskDef: EntityDef<string, LensPuckConfig> = {
  type: "task",
  category: "record",
  label: "Task",
  pluralLabel: "Tasks",
  icon: "\u2611",
  color: "#8b5cf6",
  fields: [
    {
      id: "priority",
      type: "enum",
      label: "Priority",
      default: "normal",
      enumOptions: [
        { value: "low", label: "Low" },
        { value: "normal", label: "Normal" },
        { value: "high", label: "High" },
        { value: "urgent", label: "Urgent" },
      ],
    },
    { id: "project", type: "string", label: "Project / List" },
    { id: "estimateMinutes", type: "int", label: "Estimate (minutes)" },
    { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
  ],
};

const reminderDef: EntityDef<string, LensPuckConfig> = {
  type: "reminder",
  category: "record",
  label: "Reminder",
  pluralLabel: "Reminders",
  icon: "\u23F0",
  color: "#f59e0b",
  fields: [
    {
      id: "repeat",
      type: "enum",
      label: "Repeat",
      default: "none",
      enumOptions: [
        { value: "none", label: "None" },
        { value: "daily", label: "Daily" },
        { value: "weekly", label: "Weekly" },
        { value: "monthly", label: "Monthly" },
        { value: "yearly", label: "Yearly" },
      ],
    },
    {
      id: "channel",
      type: "enum",
      label: "Channel",
      default: "notification",
      enumOptions: [
        { value: "notification", label: "In-app notification" },
        { value: "email", label: "Email" },
        { value: "push", label: "Push" },
      ],
    },
    { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
  ],
};

const contactDef: EntityDef<string, LensPuckConfig> = {
  type: "contact",
  category: "record",
  label: "Contact",
  pluralLabel: "Contacts",
  icon: "\u{1F464}",
  color: "#0ea5e9",
  fields: [
    { id: "email", type: "string", label: "Email", ui: { placeholder: "jane@example.com" } },
    { id: "phone", type: "string", label: "Phone" },
    { id: "org", type: "string", label: "Organisation" },
    { id: "role", type: "string", label: "Role / Title" },
    { id: "avatarUrl", type: "url", label: "Avatar URL", ui: { group: "Appearance" } },
    { id: "website", type: "url", label: "Website", ui: { group: "Links" } },
    { id: "twitter", type: "string", label: "Twitter / X handle", ui: { group: "Links" } },
    { id: "lastContactedAt", type: "datetime", label: "Last Contacted", ui: { group: "History" } },
    { id: "notes", type: "text", label: "Notes", ui: { multiline: true, group: "History" } },
  ],
};

const eventDef: EntityDef<string, LensPuckConfig> = {
  type: "event",
  category: "record",
  label: "Event",
  pluralLabel: "Events",
  icon: "\u{1F4C5}",
  color: "#22c55e",
  fields: [
    { id: "location", type: "string", label: "Location", ui: { placeholder: "123 Main St, or room ID" } },
    { id: "videoUrl", type: "url", label: "Video / Call URL" },
    { id: "allDay", type: "bool", label: "All Day", default: false },
    {
      id: "attendance",
      type: "enum",
      label: "Attendance",
      default: "confirmed",
      enumOptions: [
        { value: "confirmed", label: "Confirmed" },
        { value: "tentative", label: "Tentative" },
        { value: "declined", label: "Declined" },
      ],
    },
    { id: "agenda", type: "text", label: "Agenda", ui: { multiline: true } },
  ],
};

const noteDef: EntityDef<string, LensPuckConfig> = {
  type: "note",
  category: "record",
  label: "Note",
  pluralLabel: "Notes",
  icon: "\u{1F4DD}",
  color: "#facc15",
  fields: [
    { id: "body", type: "text", label: "Body", required: true, ui: { multiline: true } },
    {
      id: "format",
      type: "enum",
      label: "Format",
      default: "markdown",
      enumOptions: [
        { value: "plain", label: "Plain Text" },
        { value: "markdown", label: "Markdown" },
      ],
    },
  ],
};

const goalDef: EntityDef<string, LensPuckConfig> = {
  type: "goal",
  category: "record",
  label: "Goal",
  pluralLabel: "Goals",
  icon: "\u{1F3AF}",
  color: "#ec4899",
  fields: [
    { id: "targetValue", type: "float", label: "Target Value", default: 100 },
    { id: "currentValue", type: "float", label: "Current Value", default: 0 },
    { id: "unit", type: "string", label: "Unit", ui: { placeholder: "kg, hours, $, reps…" } },
    {
      id: "cadence",
      type: "enum",
      label: "Cadence",
      default: "once",
      enumOptions: [
        { value: "once", label: "One-off" },
        { value: "daily", label: "Daily" },
        { value: "weekly", label: "Weekly" },
        { value: "monthly", label: "Monthly" },
        { value: "quarterly", label: "Quarterly" },
        { value: "yearly", label: "Yearly" },
      ],
    },
  ],
};

const habitDef: EntityDef<string, LensPuckConfig> = {
  type: "habit",
  category: "record",
  label: "Habit",
  pluralLabel: "Habits",
  icon: "\u{1F501}",
  color: "#14b8a6",
  fields: [
    {
      id: "frequency",
      type: "enum",
      label: "Frequency",
      default: "daily",
      enumOptions: [
        { value: "daily", label: "Daily" },
        { value: "weekdays", label: "Weekdays" },
        { value: "weekly", label: "Weekly" },
        { value: "custom", label: "Custom" },
      ],
    },
    { id: "streak", type: "int", label: "Current Streak", default: 0 },
    { id: "longestStreak", type: "int", label: "Longest Streak", default: 0 },
    { id: "lastCompletedAt", type: "datetime", label: "Last Completed" },
    { id: "targetPerWeek", type: "int", label: "Target Per Week", default: 7 },
  ],
};

const bookmarkDef: EntityDef<string, LensPuckConfig> = {
  type: "bookmark",
  category: "record",
  label: "Bookmark",
  pluralLabel: "Bookmarks",
  icon: "\u{1F516}",
  color: "#6366f1",
  fields: [
    { id: "url", type: "url", label: "URL", required: true },
    { id: "faviconUrl", type: "url", label: "Favicon URL" },
    { id: "folder", type: "string", label: "Folder" },
    { id: "excerpt", type: "text", label: "Excerpt", ui: { multiline: true } },
  ],
};

const timerSessionDef: EntityDef<string, LensPuckConfig> = {
  type: "timer-session",
  category: "record",
  label: "Timer Session",
  pluralLabel: "Timer Sessions",
  icon: "\u23F1",
  color: "#ef4444",
  fields: [
    { id: "durationMs", type: "int", label: "Duration (ms)", default: 0 },
    { id: "taskId", type: "string", label: "Linked Task ID" },
    {
      id: "kind",
      type: "enum",
      label: "Kind",
      default: "work",
      enumOptions: [
        { value: "work", label: "Work" },
        { value: "break", label: "Break" },
        { value: "focus", label: "Focus" },
        { value: "meeting", label: "Meeting" },
      ],
    },
    { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
  ],
};

const captureDef: EntityDef<string, LensPuckConfig> = {
  type: "capture",
  category: "record",
  label: "Capture",
  pluralLabel: "Captures",
  icon: "\u{1F4E5}",
  color: "#64748b",
  fields: [
    { id: "body", type: "text", label: "Body", required: true, ui: { multiline: true } },
    {
      id: "source",
      type: "enum",
      label: "Source",
      default: "quick",
      enumOptions: [
        { value: "quick", label: "Quick entry" },
        { value: "email", label: "Email" },
        { value: "share", label: "Share" },
        { value: "voice", label: "Voice" },
        { value: "clipboard", label: "Clipboard" },
      ],
    },
    { id: "processedAt", type: "datetime", label: "Processed At" },
  ],
};

// ── Dynamic Record Widgets ─────────────────────────────────────────────────
//
// Each widget is a Puck-draggable view over one record type. They share the
// "pull objects from kernel.store.allObjects()" pattern used by list/table/
// kanban widgets, but each renders a specialised affordance for its record
// type (checkbox rows for tasks, relative-date chips for reminders, contact
// cards, event timeline, etc.).

const tasksWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "tasks-widget",
  category: "component",
  label: "Tasks Widget",
  pluralLabel: "Tasks Widgets",
  icon: "\u2611",
  color: "#8b5cf6",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Tasks" },
    {
      id: "filter",
      type: "enum",
      label: "Filter",
      default: "open",
      enumOptions: [
        { value: "all", label: "All tasks" },
        { value: "open", label: "Open only (todo + doing)" },
        { value: "today", label: "Due today" },
        { value: "overdue", label: "Overdue" },
        { value: "done", label: "Completed" },
      ],
    },
    { id: "project", type: "string", label: "Project filter", ui: { placeholder: "blank = all projects" } },
    { id: "maxItems", type: "int", label: "Max items", default: 10 },
    { id: "showPriority", type: "bool", label: "Show priority", default: true },
    { id: "showDueDate", type: "bool", label: "Show due date", default: true },
  ],
};

const remindersWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "reminders-widget",
  category: "component",
  label: "Reminders Widget",
  pluralLabel: "Reminders Widgets",
  icon: "\u23F0",
  color: "#f59e0b",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Reminders" },
    {
      id: "filter",
      type: "enum",
      label: "Filter",
      default: "upcoming",
      enumOptions: [
        { value: "all", label: "All reminders" },
        { value: "upcoming", label: "Upcoming (open)" },
        { value: "overdue", label: "Overdue" },
        { value: "today", label: "Due today" },
        { value: "done", label: "Completed" },
      ],
    },
    { id: "maxItems", type: "int", label: "Max items", default: 8 },
  ],
};

const contactsWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "contacts-widget",
  category: "component",
  label: "Contacts Widget",
  pluralLabel: "Contacts Widgets",
  icon: "\u{1F464}",
  color: "#0ea5e9",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Contacts" },
    {
      id: "filter",
      type: "enum",
      label: "Filter",
      default: "favorites",
      enumOptions: [
        { value: "all", label: "All contacts" },
        { value: "favorites", label: "Favorites only (pinned)" },
        { value: "recent", label: "Recently contacted" },
      ],
    },
    {
      id: "display",
      type: "enum",
      label: "Display",
      default: "cards",
      enumOptions: [
        { value: "cards", label: "Cards (grid)" },
        { value: "list", label: "Compact list" },
      ],
    },
    { id: "maxItems", type: "int", label: "Max items", default: 12 },
    { id: "showOrg", type: "bool", label: "Show organisation", default: true },
    { id: "showActions", type: "bool", label: "Show email / call actions", default: true },
  ],
};

const eventsWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "events-widget",
  category: "component",
  label: "Events Widget",
  pluralLabel: "Events Widgets",
  icon: "\u{1F4C5}",
  color: "#22c55e",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Upcoming events" },
    {
      id: "range",
      type: "enum",
      label: "Range",
      default: "week",
      enumOptions: [
        { value: "today", label: "Today" },
        { value: "week", label: "Next 7 days" },
        { value: "month", label: "Next 30 days" },
        { value: "all", label: "All upcoming" },
      ],
    },
    { id: "maxItems", type: "int", label: "Max items", default: 8 },
    { id: "showLocation", type: "bool", label: "Show location", default: true },
  ],
};

const notesWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "notes-widget",
  category: "component",
  label: "Notes Widget",
  pluralLabel: "Notes Widgets",
  icon: "\u{1F4DD}",
  color: "#facc15",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Notes" },
    {
      id: "filter",
      type: "enum",
      label: "Filter",
      default: "pinned",
      enumOptions: [
        { value: "all", label: "All notes" },
        { value: "pinned", label: "Pinned only" },
        { value: "recent", label: "Recently edited" },
      ],
    },
    { id: "tag", type: "string", label: "Tag filter", ui: { placeholder: "blank = all" } },
    { id: "maxItems", type: "int", label: "Max items", default: 8 },
    { id: "previewLength", type: "int", label: "Preview chars", default: 120 },
  ],
};

const goalsWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "goals-widget",
  category: "component",
  label: "Goals Widget",
  pluralLabel: "Goals Widgets",
  icon: "\u{1F3AF}",
  color: "#ec4899",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Goals" },
    {
      id: "filter",
      type: "enum",
      label: "Filter",
      default: "active",
      enumOptions: [
        { value: "all", label: "All goals" },
        { value: "active", label: "Active only" },
        { value: "completed", label: "Completed" },
      ],
    },
    { id: "maxItems", type: "int", label: "Max items", default: 6 },
  ],
};

const habitTrackerWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "habit-tracker-widget",
  category: "component",
  label: "Habit Tracker Widget",
  pluralLabel: "Habit Tracker Widgets",
  icon: "\u{1F501}",
  color: "#14b8a6",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Habits" },
    { id: "maxItems", type: "int", label: "Max items", default: 8 },
    { id: "showStreak", type: "bool", label: "Show streak", default: true },
  ],
};

const bookmarksWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "bookmarks-widget",
  category: "component",
  label: "Bookmarks Widget",
  pluralLabel: "Bookmarks Widgets",
  icon: "\u{1F516}",
  color: "#6366f1",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Bookmarks" },
    { id: "folder", type: "string", label: "Folder filter", ui: { placeholder: "blank = all" } },
    { id: "maxItems", type: "int", label: "Max items", default: 12 },
    {
      id: "display",
      type: "enum",
      label: "Display",
      default: "grid",
      enumOptions: [
        { value: "grid", label: "Favicon grid" },
        { value: "list", label: "List" },
      ],
    },
  ],
};

const timerWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "timer-widget",
  category: "component",
  label: "Timer Widget",
  pluralLabel: "Timer Widgets",
  icon: "\u23F1",
  color: "#ef4444",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Focus timer" },
    { id: "defaultMinutes", type: "int", label: "Default duration (minutes)", default: 25 },
    { id: "maxRecent", type: "int", label: "Recent sessions to show", default: 5 },
  ],
};

const captureInboxWidgetDef: EntityDef<string, LensPuckConfig> = {
  type: "capture-inbox-widget",
  category: "component",
  label: "Capture Inbox Widget",
  pluralLabel: "Capture Inboxes",
  icon: "\u{1F4E5}",
  color: "#64748b",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Heading", default: "Inbox" },
    { id: "maxItems", type: "int", label: "Max items", default: 10 },
    { id: "showProcessed", type: "bool", label: "Show processed", default: false },
  ],
};

// ── App Builder entities ───────────────────────────────────────────────────
//
// The "app" category represents one buildable Prism App (Flux, Cadence, …).
// An app contains:
//   • one `app-shell` component (the outer chrome wrapping every route),
//   • zero or more `route` entries mapping URL paths → pages,
//   • the `page` objects those routes point at,
//   • and `behavior` entries — Luau scripts attached to components / routes.
//
// App Shells are in the `component` category (same as page-shell) so they
// can live as children of an app via the category rule. Routes and
// behaviors live in their own categories so containment rules can
// enforce "routes only under apps", "behaviors only under apps".

const appDef: EntityDef<string, LensPuckConfig> = {
  type: "app",
  category: "app",
  label: "App",
  pluralLabel: "Apps",
  icon: "\u25A2",
  color: "#a855f7",
  fields: [
    { id: "name", type: "string", label: "Name", required: true },
    {
      id: "profileId",
      type: "enum",
      label: "Profile",
      default: "studio",
      enumOptions: [
        { value: "studio", label: "Studio" },
        { value: "flux", label: "Flux" },
        { value: "lattice", label: "Lattice" },
        { value: "cadence", label: "Cadence" },
        { value: "grip", label: "Grip" },
      ],
    },
    { id: "description", type: "text", label: "Description", ui: { multiline: true } },
    { id: "themePrimary", type: "color", label: "Primary Theme Color" },
    {
      id: "homeRouteId",
      type: "object_ref",
      label: "Home Route",
      refTypes: ["route"],
    },
  ],
};

const appShellDef: EntityDef<string, LensPuckConfig> = {
  type: "app-shell",
  category: "component",
  label: "App Shell",
  pluralLabel: "App Shells",
  icon: "\u25A3",
  color: "#a855f7",
  fields: [
    { id: "brand", type: "string", label: "Brand", default: "Your App" },
    { id: "brandIcon", type: "string", label: "Brand Icon" },
    { id: "topBarHeight", type: "int", label: "Top Bar Height (px)", default: 48 },
    { id: "leftBarWidth", type: "int", label: "Left Bar Width (px)", default: 0 },
    { id: "rightBarWidth", type: "int", label: "Right Bar Width (px)", default: 0 },
    { id: "bottomBarHeight", type: "int", label: "Bottom Bar Height (px)", default: 0 },
    { id: "stickyTopBar", type: "bool", label: "Sticky Top Bar", default: true },
    {
      id: "showsActiveRoute",
      type: "bool",
      label: "Highlight Active Route",
      default: true,
    },
    ...STYLE_FIELD_DEFS,
  ],
};

const routeDef: EntityDef<string, LensPuckConfig> = {
  type: "route",
  category: "route",
  label: "Route",
  pluralLabel: "Routes",
  icon: "\u21AA",
  color: "#22d3ee",
  childOnly: true,
  fields: [
    { id: "path", type: "string", label: "URL Path", required: true, ui: { placeholder: "/tasks" } },
    { id: "label", type: "string", label: "Label", required: true },
    { id: "pageId", type: "object_ref", label: "Target Page", refTypes: ["page"] },
    { id: "showInNav", type: "bool", label: "Show in Nav", default: true },
    {
      id: "parentRouteId",
      type: "object_ref",
      label: "Parent Route",
      refTypes: ["route"],
    },
  ],
};

const behaviorDef: EntityDef<string, LensPuckConfig> = {
  type: "behavior",
  category: "behavior",
  label: "Behavior",
  pluralLabel: "Behaviors",
  icon: "\u26A1",
  color: "#facc15",
  childOnly: true,
  fields: [
    {
      id: "targetObjectId",
      type: "object_ref",
      label: "Target",
    },
    {
      id: "trigger",
      type: "enum",
      label: "Trigger",
      default: "onClick",
      enumOptions: [
        { value: "onClick", label: "On Click" },
        { value: "onMount", label: "On Mount" },
        { value: "onChange", label: "On Change" },
        { value: "onRouteEnter", label: "On Route Enter" },
        { value: "onRouteLeave", label: "On Route Leave" },
      ],
    },
    {
      id: "source",
      type: "text",
      label: "Luau Source",
      ui: { multiline: true, placeholder: "ui.navigate(\"/about\")" },
    },
    { id: "enabled", type: "bool", label: "Enabled", default: true },
  ],
};

const facetDefDef: EntityDef<string, LensPuckConfig> = {
  type: "facet-def",
  category: "facet",
  label: "Facet Definition",
  pluralLabel: "Facet Definitions",
  icon: "\uD83D\uDDC2\uFE0F",
  color: "#0ea5e9",
  fields: [
    { id: "name", type: "string", label: "Name", required: true },
    {
      id: "objectType",
      type: "string",
      label: "Object Type",
      ui: { placeholder: "task" },
    },
    {
      id: "layout",
      type: "enum",
      label: "Layout Mode",
      default: "form",
      enumOptions: [
        { value: "form", label: "Form" },
        { value: "list", label: "List" },
        { value: "table", label: "Table" },
        { value: "report", label: "Report" },
        { value: "card", label: "Card" },
      ],
    },
    {
      id: "description",
      type: "text",
      label: "Description",
      ui: { multiline: true },
    },
  ],
};

// ── Edge Type Definitions ───────────────────────────────────────────────────

const referencesEdge: EdgeTypeDef = {
  relation: "references",
  label: "References",
  behavior: "weak",
  suggestInline: true,
  color: "#94a3b8",
};

const linksToEdge: EdgeTypeDef = {
  relation: "links-to",
  label: "Links To",
  behavior: "weak",
  suggestInline: true,
  color: "#60a5fa",
};

// ── Factory ─────────────────────────────────────────────────────────────────

export function createPageBuilderRegistry(): ObjectRegistry<string, LensPuckConfig> {
  const registry = new ObjectRegistry<string, LensPuckConfig>(categoryRules);

  registry.register(folderDef);
  registry.register(pageDef);
  registry.register(sectionDef);
  registry.register(headingDef);
  registry.register(textBlockDef);
  registry.register(imageDef);
  registry.register(buttonDef);
  registry.register(cardDef);
  registry.register(luauBlockDef);
  registry.register(facetViewDef);
  registry.register(spatialCanvasDef);
  registry.register(dataPortalDef);
  registry.register(recordListDef);
  registry.register(kanbanWidgetDef);
  registry.register(listWidgetDef);
  registry.register(tableWidgetDef);
  registry.register(cardGridWidgetDef);
  registry.register(reportWidgetDef);
  registry.register(calendarWidgetDef);
  registry.register(chartWidgetDef);
  registry.register(mapWidgetDef);
  registry.register(tabContainerDef);
  registry.register(popoverWidgetDef);
  registry.register(slidePanelDef);
  registry.register(textInputDef);
  registry.register(textareaInputDef);
  registry.register(selectInputDef);
  registry.register(checkboxInputDef);
  registry.register(numberInputDef);
  registry.register(dateInputDef);
  registry.register(columnsDef);
  registry.register(dividerDef);
  registry.register(spacerDef);
  registry.register(pageShellDef);
  registry.register(appShellDef);
  registry.register(siteHeaderDef);
  registry.register(siteFooterDef);
  registry.register(sideBarDef);
  registry.register(navBarDef);
  registry.register(heroDef);
  registry.register(statWidgetDef);
  registry.register(badgeDef);
  registry.register(alertDef);
  registry.register(progressBarDef);
  registry.register(markdownWidgetDef);
  registry.register(iframeWidgetDef);
  registry.register(codeBlockDef);
  registry.register(videoWidgetDef);
  registry.register(audioWidgetDef);
  registry.register(siteNavDef);
  registry.register(breadcrumbsDef);

  // ── App builder ────────────────────────────────────────────────────────
  registry.register(appDef);
  registry.register(routeDef);
  registry.register(behaviorDef);
  registry.register(facetDefDef);

  // ── Dynamic records ────────────────────────────────────────────────────
  registry.register(taskDef);
  registry.register(reminderDef);
  registry.register(contactDef);
  registry.register(eventDef);
  registry.register(noteDef);
  registry.register(goalDef);
  registry.register(habitDef);
  registry.register(bookmarkDef);
  registry.register(timerSessionDef);
  registry.register(captureDef);

  // ── Dynamic widgets (bound to records above) ──────────────────────────
  registry.register(tasksWidgetDef);
  registry.register(remindersWidgetDef);
  registry.register(contactsWidgetDef);
  registry.register(eventsWidgetDef);
  registry.register(notesWidgetDef);
  registry.register(goalsWidgetDef);
  registry.register(habitTrackerWidgetDef);
  registry.register(bookmarksWidgetDef);
  registry.register(timerWidgetDef);
  registry.register(captureInboxWidgetDef);

  registry.registerEdge(referencesEdge);
  registry.registerEdge(linksToEdge);

  return registry;
}
