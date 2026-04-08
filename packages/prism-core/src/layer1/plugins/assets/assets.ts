/**
 * @prism/plugin-assets — Asset Management Domain Registry (Layer 1)
 *
 * Registers media assets, content items, scanned documents, and collections.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "../../object-model/types.js";
import type { FluxAutomationPreset } from "../../flux/flux-types.js";
import type { PrismPlugin } from "../../plugin/plugin-types.js";
import { pluginId } from "../../plugin/plugin-types.js";
import type { AssetsRegistry, AssetsEntityType, AssetsEdgeType } from "./assets-types.js";
import { ASSETS_CATEGORIES, ASSETS_TYPES, ASSETS_EDGES, MEDIA_KINDS } from "./assets-types.js";

// ── Field Definitions ────────────────────────────────────────────────────

function enumOptions(values: ReadonlyArray<{ value: string; label: string }>): Array<{ value: string; label: string }> {
  return values.map(v => ({ value: v.value, label: v.label }));
}

const MEDIA_ASSET_FIELDS: EntityFieldDef[] = [
  { id: "mediaKind", type: "enum", label: "Kind", enumOptions: enumOptions(MEDIA_KINDS), required: true },
  { id: "mimeType", type: "string", label: "MIME Type", ui: { readonly: true } },
  { id: "fileSize", type: "int", label: "File Size (bytes)", ui: { readonly: true } },
  { id: "width", type: "int", label: "Width (px)" },
  { id: "height", type: "int", label: "Height (px)" },
  { id: "duration", type: "float", label: "Duration (sec)" },
  { id: "blobRef", type: "string", label: "Blob Reference", ui: { hidden: true } },
  { id: "thumbnailRef", type: "string", label: "Thumbnail Ref", ui: { hidden: true } },
  { id: "altText", type: "string", label: "Alt Text" },
  { id: "caption", type: "text", label: "Caption", ui: { multiline: true } },
  { id: "tags", type: "string", label: "Tags", ui: { placeholder: "comma-separated" } },
  { id: "source", type: "url", label: "Source URL" },
  { id: "license", type: "string", label: "License" },
];

const CONTENT_ITEM_FIELDS: EntityFieldDef[] = [
  { id: "contentType", type: "enum", label: "Content Type", enumOptions: [
    { value: "article", label: "Article" },
    { value: "note", label: "Note" },
    { value: "snippet", label: "Snippet" },
    { value: "template", label: "Template" },
    { value: "reference", label: "Reference" },
  ] },
  { id: "body", type: "text", label: "Body", ui: { multiline: true } },
  { id: "summary", type: "text", label: "Summary", ui: { multiline: true } },
  { id: "tags", type: "string", label: "Tags", ui: { placeholder: "comma-separated" } },
  { id: "author", type: "string", label: "Author" },
  { id: "publishedAt", type: "datetime", label: "Published At" },
  { id: "sourceUrl", type: "url", label: "Source URL" },
  { id: "wordCount", type: "int", label: "Word Count", ui: { readonly: true } },
];

const SCANNED_DOC_FIELDS: EntityFieldDef[] = [
  { id: "blobRef", type: "string", label: "Scan Blob Ref", ui: { hidden: true } },
  { id: "mimeType", type: "string", label: "MIME Type", ui: { readonly: true } },
  { id: "pageCount", type: "int", label: "Pages" },
  { id: "ocrText", type: "text", label: "OCR Text", ui: { multiline: true, readonly: true } },
  { id: "ocrConfidence", type: "float", label: "OCR Confidence", ui: { readonly: true } },
  { id: "language", type: "string", label: "Language" },
  { id: "category", type: "string", label: "Category" },
  { id: "scannedAt", type: "datetime", label: "Scanned At", ui: { readonly: true } },
  { id: "tags", type: "string", label: "Tags", ui: { placeholder: "comma-separated" } },
];

const COLLECTION_FIELDS: EntityFieldDef[] = [
  { id: "description", type: "text", label: "Description", ui: { multiline: true } },
  { id: "color", type: "color", label: "Color" },
  { id: "sortField", type: "string", label: "Sort Field" },
  { id: "sortDirection", type: "enum", label: "Sort Direction", enumOptions: [
    { value: "asc", label: "Ascending" },
    { value: "desc", label: "Descending" },
  ], default: "asc" },
  { id: "itemCount", type: "int", label: "Items", default: 0, ui: { readonly: true } },
  { id: "isSmartCollection", type: "bool", label: "Smart Collection", default: false },
  { id: "filterExpression", type: "string", label: "Filter Expression", ui: { placeholder: "e.g. tags contains 'photo'" } },
];

// ── Entity Definitions ───────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    {
      type: ASSETS_TYPES.MEDIA_ASSET,
      nsid: "io.prismapp.assets.media-asset",
      category: ASSETS_CATEGORIES.MEDIA,
      label: "Media Asset",
      pluralLabel: "Media Assets",
      defaultChildView: "grid",
      fields: MEDIA_ASSET_FIELDS,
    },
    {
      type: ASSETS_TYPES.CONTENT_ITEM,
      nsid: "io.prismapp.assets.content-item",
      category: ASSETS_CATEGORIES.CONTENT,
      label: "Content Item",
      pluralLabel: "Content Items",
      defaultChildView: "list",
      fields: CONTENT_ITEM_FIELDS,
    },
    {
      type: ASSETS_TYPES.SCANNED_DOC,
      nsid: "io.prismapp.assets.scanned-doc",
      category: ASSETS_CATEGORIES.CONTENT,
      label: "Scanned Document",
      pluralLabel: "Scanned Documents",
      defaultChildView: "list",
      fields: SCANNED_DOC_FIELDS,
    },
    {
      type: ASSETS_TYPES.COLLECTION,
      nsid: "io.prismapp.assets.collection",
      category: ASSETS_CATEGORIES.COLLECTIONS,
      label: "Collection",
      pluralLabel: "Collections",
      defaultChildView: "grid",
      fields: COLLECTION_FIELDS,
      extraChildTypes: [ASSETS_TYPES.MEDIA_ASSET, ASSETS_TYPES.CONTENT_ITEM, ASSETS_TYPES.SCANNED_DOC],
    },
  ];
}

// ── Edge Definitions ─────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: ASSETS_EDGES.IN_COLLECTION,
      nsid: "io.prismapp.assets.in-collection",
      label: "In Collection",
      behavior: "membership",
      sourceTypes: [ASSETS_TYPES.MEDIA_ASSET, ASSETS_TYPES.CONTENT_ITEM, ASSETS_TYPES.SCANNED_DOC],
      targetTypes: [ASSETS_TYPES.COLLECTION],
    },
    {
      relation: ASSETS_EDGES.DERIVED_FROM,
      nsid: "io.prismapp.assets.derived-from",
      label: "Derived From",
      behavior: "weak",
      sourceTypes: [ASSETS_TYPES.MEDIA_ASSET],
      targetTypes: [ASSETS_TYPES.MEDIA_ASSET],
      description: "Links derivative works to their source (e.g. thumbnail, crop, transcode)",
    },
    {
      relation: ASSETS_EDGES.ATTACHED_TO,
      nsid: "io.prismapp.assets.attached-to",
      label: "Attached To",
      behavior: "weak",
      sourceTypes: [ASSETS_TYPES.MEDIA_ASSET, ASSETS_TYPES.SCANNED_DOC],
      description: "Attaches a media asset or scan to any object",
      suggestInline: true,
    },
  ];
}

// ── Automation Presets ────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "assets:auto:collection-count",
      name: "Update collection item count",
      entityType: ASSETS_TYPES.COLLECTION as string,
      trigger: "on_update",
      condition: "true",
      actions: [
        { kind: "set_field", target: "itemCount", value: "{{count(children)}}" },
      ],
    },
    {
      id: "assets:auto:ocr-complete",
      name: "Notify on OCR completion",
      entityType: ASSETS_TYPES.SCANNED_DOC as string,
      trigger: "on_status_change",
      condition: "status == 'completed'",
      actions: [
        { kind: "send_notification", target: "owner", value: "Document '{{name}}' OCR complete ({{ocrConfidence}}% confidence)" },
      ],
    },
  ];
}

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.assets"),
    name: "Assets",
    contributes: {
      views: [
        { id: "assets:media", label: "Media Library", zone: "content", componentId: "MediaLibraryView", description: "Media asset browser" },
        { id: "assets:content", label: "Content Library", zone: "content", componentId: "ContentLibraryView", description: "Content management" },
        { id: "assets:scanner", label: "Document Scanner", zone: "content", componentId: "DocumentScannerView", description: "OCR document scanner" },
        { id: "assets:collections", label: "Collections", zone: "content", componentId: "CollectionBrowserView", description: "Asset collections" },
      ],
      commands: [
        { id: "assets:import-media", label: "Import Media", category: "Assets", action: "assets.importMedia" },
        { id: "assets:scan-document", label: "Scan Document", category: "Assets", action: "assets.scanDocument" },
        { id: "assets:new-collection", label: "New Collection", category: "Assets", action: "assets.newCollection" },
      ],
      activityBar: [
        { id: "assets:activity", label: "Assets", position: "top", priority: 35 },
      ],
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createAssetsRegistry(): AssetsRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();
  const plugin = buildPlugin();

  return {
    getEntityDefs: () => entityDefs,
    getEdgeDefs: () => edgeDefs,
    getEntityDef: (type: AssetsEntityType) => entityDefs.find(d => d.type === type),
    getEdgeDef: (relation: AssetsEdgeType) => edgeDefs.find(d => d.relation === relation),
    getAutomationPresets: () => presets,
    getPlugin: () => plugin,
  };
}

// ── Self-Registering Bundle ──────────────────────────────────────────────

import type { PluginBundle, PluginInstallContext } from "../plugin-install.js";

export function createAssetsBundle(): PluginBundle {
  return {
    id: "prism.plugin.assets",
    name: "Assets",
    install(ctx: PluginInstallContext) {
      const reg = createAssetsRegistry();
      ctx.objectRegistry.registerAll(reg.getEntityDefs());
      ctx.objectRegistry.registerEdges(reg.getEdgeDefs());
      return ctx.pluginRegistry.register(reg.getPlugin());
    },
  };
}
