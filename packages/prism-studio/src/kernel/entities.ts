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

  registry.registerEdge(referencesEdge);
  registry.registerEdge(linksToEdge);

  return registry;
}
