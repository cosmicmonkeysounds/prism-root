/**
 * Page-builder entity and edge type definitions.
 *
 * Registers the object types that Studio's page builder works with.
 * Each type has a category for containment rules and fields for
 * the inspector panel.
 */

import { ObjectRegistry } from "@prism/core/object-model";
import type { EntityDef, EdgeTypeDef, CategoryRule } from "@prism/core/object-model";
import { STYLE_FIELD_DEFS } from "@prism/core/page-builder";

// ── Category Rules ──────────────────────────────────────────────────────────

const categoryRules: CategoryRule[] = [
  { category: "workspace", canParent: ["page"], canBeRoot: true },
  { category: "page", canParent: ["section", "component"], canBeRoot: true },
  { category: "section", canParent: ["component"], canBeRoot: false },
  { category: "component", canParent: [], canBeRoot: false },
];

// ── Entity Definitions ──────────────────────────────────────────────────────

const folderDef: EntityDef<string> = {
  type: "folder",
  category: "workspace",
  label: "Folder",
  pluralLabel: "Folders",
  icon: "\uD83D\uDCC1",
  color: "#e8a838",
  fields: [],
};

const pageDef: EntityDef<string> = {
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
      default: "single",
      enumOptions: [
        { value: "single", label: "Single Column" },
        { value: "sidebar", label: "With Sidebar" },
        { value: "full", label: "Full Width" },
      ],
    },
    { id: "published", type: "bool", label: "Published", default: false },
    { id: "publishedAt", type: "datetime", label: "Published At" },
    { id: "metaDescription", type: "text", label: "Meta Description", ui: { multiline: true, group: "SEO" } },
  ],
};

const sectionDef: EntityDef<string> = {
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

const headingDef: EntityDef<string> = {
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

const textBlockDef: EntityDef<string> = {
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

const imageDef: EntityDef<string> = {
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

const buttonDef: EntityDef<string> = {
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
    ]},
    { id: "size", type: "enum", label: "Size", default: "md", enumOptions: [
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
    ]},
    ...STYLE_FIELD_DEFS,
  ],
};

const cardDef: EntityDef<string> = {
  type: "card",
  category: "component",
  label: "Card",
  pluralLabel: "Cards",
  icon: "\u25A1",
  color: "#8b5cf6",
  childOnly: true,
  fields: [
    { id: "title", type: "string", label: "Title" },
    { id: "body", type: "text", label: "Body", ui: { multiline: true } },
    { id: "imageUrl", type: "url", label: "Image URL" },
    { id: "linkUrl", type: "url", label: "Link URL" },
    ...STYLE_FIELD_DEFS,
  ],
};

const luauBlockDef: EntityDef<string> = {
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

const facetViewDef: EntityDef<string> = {
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

const spatialCanvasDef: EntityDef<string> = {
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

const dataPortalDef: EntityDef<string> = {
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

const kanbanWidgetDef: EntityDef<string> = {
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

const listWidgetDef: EntityDef<string> = {
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

const tableWidgetDef: EntityDef<string> = {
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

const cardGridWidgetDef: EntityDef<string> = {
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

const reportWidgetDef: EntityDef<string> = {
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

const calendarWidgetDef: EntityDef<string> = {
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

const chartWidgetDef: EntityDef<string> = {
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

const mapWidgetDef: EntityDef<string> = {
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

const tabContainerDef: EntityDef<string> = {
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

const popoverWidgetDef: EntityDef<string> = {
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

const slidePanelDef: EntityDef<string> = {
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

const textInputDef: EntityDef<string> = {
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

const textareaInputDef: EntityDef<string> = {
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

const selectInputDef: EntityDef<string> = {
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

const checkboxInputDef: EntityDef<string> = {
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

const numberInputDef: EntityDef<string> = {
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

const dateInputDef: EntityDef<string> = {
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

const columnsDef: EntityDef<string> = {
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

const dividerDef: EntityDef<string> = {
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

const spacerDef: EntityDef<string> = {
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

const pageShellDef: EntityDef<string> = {
  type: "page-shell",
  category: "component",
  label: "Page Shell",
  pluralLabel: "Page Shells",
  icon: "\u25A6",
  color: "#0ea5e9",
  fields: [
    {
      id: "layout",
      type: "enum",
      label: "Layout",
      default: "sidebar-left",
      enumOptions: [
        { value: "sidebar-left", label: "Sidebar Left" },
        { value: "sidebar-right", label: "Sidebar Right" },
        { value: "stacked", label: "Stacked (no sidebar)" },
      ],
    },
    { id: "sidebarWidth", type: "int", label: "Sidebar Width (px)", default: 240 },
    { id: "stickyHeader", type: "bool", label: "Sticky Header", default: true },
    ...STYLE_FIELD_DEFS,
  ],
};

const siteHeaderDef: EntityDef<string> = {
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

const siteFooterDef: EntityDef<string> = {
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

const sideBarDef: EntityDef<string> = {
  type: "side-bar",
  category: "component",
  label: "Sidebar",
  pluralLabel: "Sidebars",
  icon: "\u2590",
  color: "#0ea5e9",
  fields: [
    { id: "width", type: "int", label: "Width (px)", default: 260 },
    {
      id: "position",
      type: "enum",
      label: "Position",
      default: "left",
      enumOptions: [
        { value: "left", label: "Left" },
        { value: "right", label: "Right" },
      ],
    },
    ...STYLE_FIELD_DEFS,
  ],
};

const navBarDef: EntityDef<string> = {
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

const heroDef: EntityDef<string> = {
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

const statWidgetDef: EntityDef<string> = {
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

const badgeDef: EntityDef<string> = {
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

const alertDef: EntityDef<string> = {
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

const progressBarDef: EntityDef<string> = {
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

const markdownWidgetDef: EntityDef<string> = {
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

const iframeWidgetDef: EntityDef<string> = {
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

const codeBlockDef: EntityDef<string> = {
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

const videoWidgetDef: EntityDef<string> = {
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

const audioWidgetDef: EntityDef<string> = {
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

const siteNavDef: EntityDef<string> = {
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

const breadcrumbsDef: EntityDef<string> = {
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

export function createPageBuilderRegistry(): ObjectRegistry<string> {
  const registry = new ObjectRegistry<string>(categoryRules);

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

  registry.registerEdge(referencesEdge);
  registry.registerEdge(linksToEdge);

  return registry;
}
