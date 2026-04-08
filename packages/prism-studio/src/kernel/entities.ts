/**
 * Page-builder entity and edge type definitions.
 *
 * Registers the object types that Studio's page builder works with.
 * Each type has a category for containment rules and fields for
 * the inspector panel.
 */

import { ObjectRegistry } from "@prism/core/object-model";
import type { EntityDef, EdgeTypeDef, CategoryRule } from "@prism/core/object-model";

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
    { id: "background", type: "color", label: "Background Color" },
    { id: "padding", type: "enum", label: "Padding", default: "md", enumOptions: [
      { value: "none", label: "None" },
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
    ]},
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
  ],
};

const luaBlockDef: EntityDef<string> = {
  type: "lua-block",
  category: "component",
  label: "Lua Block",
  pluralLabel: "Lua Blocks",
  icon: "\uD83C\uDF19",
  color: "#06b6d4",
  childOnly: true,
  fields: [
    { id: "source", type: "text", label: "Lua Source", required: true, ui: { multiline: true } },
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
  registry.register(luaBlockDef);
  registry.register(facetViewDef);
  registry.register(spatialCanvasDef);
  registry.register(dataPortalDef);
  registry.register(kanbanWidgetDef);
  registry.register(calendarWidgetDef);
  registry.register(chartWidgetDef);
  registry.register(mapWidgetDef);
  registry.register(tabContainerDef);
  registry.register(popoverWidgetDef);
  registry.register(slidePanelDef);

  registry.registerEdge(referencesEdge);
  registry.registerEdge(linksToEdge);

  return registry;
}
