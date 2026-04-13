/**
 * Pure-data helpers for the layout panel.
 *
 * Everything in this file is framework-agnostic: no React, no Puck runtime,
 * no kernel instance — just the shell-slot metadata and the kernel-object-to-
 * Puck-Data projection used by `layout-panel.tsx`. Split out so vitest can
 * exercise it without dragging in the heavy UI graph (leaflet, react, etc.)
 * pulled in transitively by the panel module.
 */

import type { Data } from "@measured/puck";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

/**
 * Slot field names for every layout-shell entity type.
 *
 * Shells expose their regions via Puck slot fields instead of stacking
 * children vertically. Each kernel child of a shell carries `data.__slot`
 * naming which slot it belongs to; on projection the children are grouped
 * back into per-slot Puck content arrays.
 *
 * The `page` entry is special: pages don't live inside Puck's `content`
 * array the way shells do, so their slot children get projected into
 * `root.props` via `kernelToPuckData` and rendered by a custom Puck root
 * component. This is what makes sidebar/header/footer regions a first-class
 * page concept — authors don't need to drop a `PageShell` widget first.
 */
export const PAGE_SLOTS: readonly string[] = ["header", "sidebar", "footer"];

export const SHELL_SLOTS: Readonly<Record<string, readonly string[]>> = {
  page: PAGE_SLOTS,
  "page-shell": ["header", "sidebar", "main", "footer"],
  "site-header": ["nav"],
  "site-footer": ["col1", "col2", "col3"],
  "side-bar": ["content"],
  "nav-bar": ["links"],
  "hero": ["content"],
};

export function getShellSlots(kernelType: string): readonly string[] {
  return SHELL_SLOTS[kernelType] ?? [];
}

export function isShellType(kernelType: string): boolean {
  return kernelType in SHELL_SLOTS;
}

export function kebabToPascal(s: string): string {
  return s
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join("");
}

export function pascalToKebab(s: string): string {
  return s.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
}

// ── Component category mapping ─────────────────────────────────────────────

/**
 * Groups every registered component entity into one of Puck's sidebar
 * categories. Ordering here also drives sidebar ordering — Puck preserves
 * insertion order of the `categories` record. New entity types default to
 * the `other` bucket (Puck renders that catch-all automatically) so this
 * map never has to be exhaustive to avoid breakage.
 */
export const COMPONENT_CATEGORY_MAP: Record<string, string> = {
  // Layout primitives & shells
  "page-shell": "layout",
  "site-header": "layout",
  "site-footer": "layout",
  "side-bar": "layout",
  "nav-bar": "layout",
  hero: "layout",
  section: "layout",
  columns: "layout",
  "tab-container": "layout",
  spacer: "layout",
  divider: "layout",

  // Static content
  heading: "content",
  "text-block": "content",
  "markdown-widget": "content",
  "iframe-widget": "content",
  "code-block": "content",

  // Media
  image: "media",
  "video-widget": "media",
  "audio-widget": "media",

  // Data-aware views (backed by FacetDefinitions or collections)
  "facet-view": "dataViews",
  "spatial-canvas": "dataViews",
  "data-portal": "dataViews",
  "kanban-widget": "dataViews",
  "list-widget": "dataViews",
  "table-widget": "dataViews",
  "card-grid-widget": "dataViews",
  "report-widget": "dataViews",
  "calendar-widget": "dataViews",
  "chart-widget": "dataViews",
  "map-widget": "dataViews",

  // Form inputs
  "text-input": "forms",
  "textarea-input": "forms",
  "select-input": "forms",
  "checkbox-input": "forms",
  "number-input": "forms",
  "date-input": "forms",

  // Navigation elements
  "site-nav": "navigation",
  breadcrumbs: "navigation",
  button: "navigation",

  // Data display / KPI chrome
  "stat-widget": "display",
  badge: "display",
  alert: "display",
  "progress-bar": "display",
  card: "display",

  // Dynamic / scripted
  "luau-block": "dynamic",
  "popover-widget": "dynamic",
  "slide-panel": "dynamic",
};

/**
 * Human titles for each category key. `other` is Puck's built-in
 * catch-all bucket for components that aren't listed anywhere else.
 */
export const CATEGORY_TITLES: Record<string, string> = {
  layout: "Layout",
  content: "Content",
  media: "Media",
  dataViews: "Data Views",
  forms: "Forms",
  navigation: "Navigation",
  display: "Display",
  dynamic: "Dynamic",
  other: "Other",
};

export interface PuckCategoryBucket {
  title: string;
  components: string[];
  defaultExpanded?: boolean;
}

/**
 * Pure helper: group a set of PascalCase component names into the Puck
 * `categories` shape based on `COMPONENT_CATEGORY_MAP`. Exported for
 * tests — verifies that every registered entity lands in *some* bucket.
 */
export function buildPuckCategories(
  componentNames: ReadonlyArray<string>,
): Record<string, PuckCategoryBucket> {
  const out: Record<string, PuckCategoryBucket> = {};
  const ensure = (key: string): PuckCategoryBucket => {
    let bucket = out[key];
    if (!bucket) {
      bucket = {
        title: CATEGORY_TITLES[key] ?? key,
        components: [],
        defaultExpanded: key === "layout" || key === "content",
      };
      out[key] = bucket;
    }
    return bucket;
  };

  // Seed the canonical order so empty buckets still show up stable.
  for (const key of Object.keys(CATEGORY_TITLES)) {
    if (key !== "other") ensure(key);
  }

  for (const name of componentNames) {
    const kebab = pascalToKebab(name);
    const key = COMPONENT_CATEGORY_MAP[kebab] ?? "other";
    ensure(key).components.push(name);
  }

  // Drop empty non-other buckets so the sidebar isn't cluttered with
  // headings that never contain anything. Building a new record
  // preserves insertion order without triggering the
  // no-dynamic-delete lint rule.
  const pruned: Record<string, PuckCategoryBucket> = {};
  for (const [key, bucket] of Object.entries(out)) {
    if (key === "other" || bucket.components.length > 0) {
      pruned[key] = bucket;
    }
  }
  return pruned;
}

/**
 * Project the subtree under `pageId` into Puck `Data`.
 *
 * - The page's non-slotted children become the top-level `content` array
 *   (Puck's main flow), with legacy top-level sections flattened so their
 *   grandchildren are promoted up.
 * - Shells (page-shell, site-header, etc.) nested inside `content` still
 *   emit per-slot child arrays so Puck's slot drop zones populate
 *   recursively.
 * - The page entity itself exposes header/sidebar/footer slots via
 *   `root.props` — those are any kernel children of the page that carry
 *   a `data.__slot` tag matching `PAGE_SLOTS`. The layout + sidebarWidth +
 *   stickyHeader props live on `page.data` and are also surfaced on
 *   `root.props` so the Puck root render can decide whether to wrap the
 *   flat content in a sidebar grid.
 */
export function kernelToPuckData(
  pageId: ObjectId,
  allObjects: GraphObject[],
): Data {
  const content = buildPuckContent(pageId, null, allObjects, /*topLevel*/ true);
  const page = allObjects.find((o) => o.id === pageId && !o.deletedAt);
  const rootProps: Record<string, unknown> = {};
  if (page) {
    const pd = (page.data ?? {}) as Record<string, unknown>;
    for (const [k, v] of Object.entries(pd)) {
      if (k === "__slot") continue;
      rootProps[k] = v;
    }
  }
  for (const slot of PAGE_SLOTS) {
    rootProps[slot] = buildPuckContent(pageId, slot, allObjects, false);
  }
  return { content, root: { props: rootProps } };
}

/**
 * Reverse of `kernelToPuckData`'s `root.props` projection: split a Puck
 * `root.props` bag into the non-slot scalar props (→ `page.data`) and the
 * per-slot content arrays (→ kernel children with `data.__slot`).
 *
 * Kept as a pure split so `syncPuckToKernel` stays thin and vitest can
 * check the partitioning without needing a real Puck editor.
 */
export function splitRootProps(
  rootProps: Record<string, unknown> | undefined,
): {
  pageData: Record<string, unknown>;
  slots: Record<string, Data["content"]>;
} {
  const pageData: Record<string, unknown> = {};
  const slots: Record<string, Data["content"]> = {};
  const slotSet = new Set<string>(PAGE_SLOTS);
  for (const [k, v] of Object.entries(rootProps ?? {})) {
    if (slotSet.has(k)) {
      slots[k] = Array.isArray(v) ? (v as Data["content"]) : [];
    } else {
      pageData[k] = v;
    }
  }
  for (const slot of PAGE_SLOTS) {
    if (!(slot in slots)) slots[slot] = [];
  }
  return { pageData, slots };
}

function buildPuckContent(
  parentId: ObjectId,
  slotName: string | null,
  allObjs: GraphObject[],
  topLevel: boolean,
): Data["content"] {
  const children = allObjs
    .filter((o) => o.parentId === parentId && !o.deletedAt)
    .filter((o) => {
      const tag = (o.data as Record<string, unknown> | undefined)?.["__slot"];
      return slotName === null ? typeof tag !== "string" : tag === slotName;
    })
    .sort((a, b) => a.position - b.position);

  const out: Data["content"] = [];
  for (const child of children) {
    if (topLevel && child.type === "section") {
      // Legacy: flatten section grandchildren into the page-level content
      // so old pages without shells keep round-tripping cleanly.
      const grand = allObjs
        .filter((o) => o.parentId === child.id && !o.deletedAt)
        .sort((a, b) => a.position - b.position);
      for (const sc of grand) out.push(toPuckItem(sc, allObjs));
      continue;
    }
    out.push(toPuckItem(child, allObjs));
  }
  return out;
}

function toPuckItem(
  obj: GraphObject,
  allObjs: GraphObject[],
): Data["content"][number] {
  const raw = (obj.data ?? {}) as Record<string, unknown>;
  const props: Record<string, unknown> = { id: obj.id };
  for (const [k, v] of Object.entries(raw)) {
    if (k === "__slot") continue;
    props[k] = v;
  }
  for (const slot of getShellSlots(obj.type)) {
    props[slot] = buildPuckContent(obj.id, slot, allObjs, false);
  }
  return {
    type: kebabToPascal(obj.type),
    props,
  } as Data["content"][number];
}
