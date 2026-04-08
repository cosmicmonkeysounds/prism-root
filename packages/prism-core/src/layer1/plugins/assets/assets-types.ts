/**
 * @prism/plugin-assets — Asset Management Domain Types (Layer 1)
 *
 * Extends Flux with media assets, content items, scanned documents,
 * and user-defined collections.
 */

import type { EntityDef, EdgeTypeDef } from "../../object-model/types.js";
import type { FluxAutomationPreset } from "../../flux/flux-types.js";
import type { PrismPlugin } from "../../plugin/plugin-types.js";

// ── Categories ────────────���───────────────────────────────────���──────────

export const ASSETS_CATEGORIES = {
  MEDIA: "assets:media",
  CONTENT: "assets:content",
  COLLECTIONS: "assets:collections",
} as const;

export type AssetsCategory = typeof ASSETS_CATEGORIES[keyof typeof ASSETS_CATEGORIES];

// ── Entity Type Strings ──────────────────────────────────────────────────

export const ASSETS_TYPES = {
  MEDIA_ASSET: "assets:media-asset",
  CONTENT_ITEM: "assets:content-item",
  SCANNED_DOC: "assets:scanned-doc",
  COLLECTION: "assets:collection",
} as const;

export type AssetsEntityType = typeof ASSETS_TYPES[keyof typeof ASSETS_TYPES];

// ── Edge Relation Strings ────────────────────────────────────────────────

export const ASSETS_EDGES = {
  IN_COLLECTION: "assets:in-collection",
  DERIVED_FROM: "assets:derived-from",
  ATTACHED_TO: "assets:attached-to",
} as const;

export type AssetsEdgeType = typeof ASSETS_EDGES[keyof typeof ASSETS_EDGES];

// ── Status Values ────────────────────────────────────────────────────────

export const MEDIA_STATUSES = [
  { value: "importing", label: "Importing" },
  { value: "ready", label: "Ready" },
  { value: "processing", label: "Processing" },
  { value: "error", label: "Error" },
  { value: "archived", label: "Archived" },
] as const;

export const CONTENT_STATUSES = [
  { value: "draft", label: "Draft" },
  { value: "review", label: "In Review" },
  { value: "published", label: "Published" },
  { value: "archived", label: "Archived" },
] as const;

export const SCAN_STATUSES = [
  { value: "pending", label: "Pending OCR" },
  { value: "processing", label: "Processing" },
  { value: "completed", label: "Completed" },
  { value: "failed", label: "Failed" },
] as const;

export const MEDIA_KINDS = [
  { value: "image", label: "Image" },
  { value: "video", label: "Video" },
  { value: "audio", label: "Audio" },
  { value: "document", label: "Document" },
  { value: "archive", label: "Archive" },
  { value: "other", label: "Other" },
] as const;

// ── Registry ─────────────────��───────────────────────────────────────────

export interface AssetsRegistry {
  getEntityDefs(): EntityDef[];
  getEdgeDefs(): EdgeTypeDef[];
  getEntityDef(type: AssetsEntityType): EntityDef | undefined;
  getEdgeDef(relation: AssetsEdgeType): EdgeTypeDef | undefined;
  getAutomationPresets(): FluxAutomationPreset[];
  getPlugin(): PrismPlugin;
}
