/**
 * Portal Renderer — extracts structured data from a CollectionStore
 * for rendering Sovereign Portal pages.
 *
 * This is a framework-agnostic Layer 1 primitive. It converts CRDT
 * collection data into a PortalSnapshot that any renderer (Hono JSX,
 * React, etc.) can consume.
 */

import type { CollectionStore } from "@prism/core/persistence";
import type { GraphObject } from "@prism/core/object-model";
import type { ObjectEdge } from "@prism/core/object-model";
import type { PortalManifest } from "./relay-types.js";

// ── Snapshot Types ─────────────────────────────────────────────────────────

/** A single object prepared for portal display. */
export interface PortalObject {
  id: string;
  type: string;
  name: string;
  description: string;
  status: string | null;
  tags: string[];
  date: string | null;
  color: string | null;
  image: string | null;
  pinned: boolean;
  data: Record<string, unknown>;
  children: PortalObject[];
}

/** An edge prepared for portal display. */
export interface PortalEdge {
  id: string;
  sourceId: string;
  targetId: string;
  relation: string;
  data: Record<string, unknown>;
}

/** Complete snapshot of a portal's data, ready for rendering. */
export interface PortalSnapshot {
  portal: PortalManifest;
  /** Root-level objects (tree structure preserved). */
  objects: PortalObject[];
  /** All edges in the collection. */
  edges: PortalEdge[];
  /** Total object count (including nested). */
  objectCount: number;
  /** ISO-8601 timestamp of snapshot generation. */
  generatedAt: string;
}

// ── Snapshot Extraction ────────────────────────────────────────────────────

/**
 * Extract a PortalSnapshot from a collection store.
 *
 * Builds a tree of PortalObjects from the flat object list,
 * preserving parent-child relationships. Only non-deleted objects
 * are included.
 */
export function extractPortalSnapshot(
  portal: PortalManifest,
  store: CollectionStore,
): PortalSnapshot {
  const allObjects = store.listObjects({ excludeDeleted: true });
  const allEdges = store.listEdges();

  // Build a lookup and child map
  const byId = new Map<string, GraphObject>();
  const childrenOf = new Map<string, GraphObject[]>();

  for (const obj of allObjects) {
    byId.set(obj.id, obj);
    const parentKey = obj.parentId ?? "__root__";
    const siblings = childrenOf.get(parentKey);
    if (siblings) {
      siblings.push(obj);
    } else {
      childrenOf.set(parentKey, [obj]);
    }
  }

  // Sort children by position
  for (const children of childrenOf.values()) {
    children.sort((a, b) => a.position - b.position);
  }

  function toPortalObject(obj: GraphObject): PortalObject {
    const kids = childrenOf.get(obj.id) ?? [];
    return {
      id: obj.id,
      type: obj.type,
      name: obj.name,
      description: obj.description,
      status: obj.status,
      tags: [...obj.tags],
      date: obj.date,
      color: obj.color,
      image: obj.image,
      pinned: obj.pinned,
      data: { ...obj.data },
      children: kids.map(toPortalObject),
    };
  }

  const roots = (childrenOf.get("__root__") ?? []).map(toPortalObject);

  const edges: PortalEdge[] = allEdges.map((e: ObjectEdge) => ({
    id: e.id,
    sourceId: e.sourceId,
    targetId: e.targetId,
    relation: e.relation,
    data: { ...e.data },
  }));

  return {
    portal,
    objects: roots,
    edges,
    objectCount: allObjects.length,
    generatedAt: new Date().toISOString(),
  };
}

// ── HTML Helpers (framework-agnostic) ──────────────────────────────────────

/** Escape HTML entities for safe rendering in any template. */
export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/**
 * Render a PortalSnapshot to a minimal static HTML string.
 * This is a fallback renderer for Level 1 portals — the relay
 * server uses Hono JSX for richer rendering, but this works
 * in any environment.
 */
export function renderPortalHtml(snapshot: PortalSnapshot): string {
  const { portal, objects, objectCount, generatedAt } = snapshot;

  function renderObject(obj: PortalObject, depth: number): string {
    const indent = "  ".repeat(depth);
    const tag = depth === 0 ? "h2" : depth === 1 ? "h3" : "h4";
    const statusBadge = obj.status
      ? ` <span class="status">${escapeHtml(obj.status)}</span>`
      : "";
    const tagBadges = obj.tags.length > 0
      ? ` ${obj.tags.map((t) => `<span class="tag">${escapeHtml(t)}</span>`).join(" ")}`
      : "";

    let html = `${indent}<article class="portal-object" data-type="${escapeHtml(obj.type)}" data-id="${escapeHtml(obj.id)}">\n`;
    html += `${indent}  <${tag}>${escapeHtml(obj.name)}${statusBadge}${tagBadges}</${tag}>\n`;
    if (obj.description) {
      html += `${indent}  <p class="description">${escapeHtml(obj.description)}</p>\n`;
    }
    if (obj.date) {
      html += `${indent}  <time datetime="${escapeHtml(obj.date)}">${escapeHtml(obj.date)}</time>\n`;
    }
    if (obj.children.length > 0) {
      html += `${indent}  <div class="children">\n`;
      for (const child of obj.children) {
        html += renderObject(child, depth + 1);
      }
      html += `${indent}  </div>\n`;
    }
    html += `${indent}</article>\n`;
    return html;
  }

  const objectsHtml = objects.map((o) => renderObject(o, 0)).join("");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(portal.name)}</title>
  <meta name="generator" content="Prism Sovereign Portal">
  <style>
    :root { --bg: #fff; --fg: #1a1a1a; --muted: #666; --accent: #2563eb; --border: #e5e7eb; }
    @media (prefers-color-scheme: dark) {
      :root { --bg: #0a0a0a; --fg: #e5e5e5; --muted: #999; --accent: #60a5fa; --border: #333; }
    }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--fg); line-height: 1.6; max-width: 72rem; margin: 0 auto; padding: 2rem; }
    header { border-bottom: 1px solid var(--border); padding-bottom: 1rem; margin-bottom: 2rem; }
    h1 { font-size: 1.75rem; font-weight: 600; }
    .meta { color: var(--muted); font-size: 0.875rem; margin-top: 0.25rem; }
    article { margin-bottom: 1.5rem; padding: 1rem; border: 1px solid var(--border); border-radius: 0.5rem; }
    article article { margin-top: 0.75rem; border-color: var(--border); }
    h2 { font-size: 1.25rem; font-weight: 600; }
    h3 { font-size: 1.1rem; font-weight: 500; }
    h4 { font-size: 1rem; font-weight: 500; }
    .description { color: var(--muted); margin-top: 0.25rem; }
    .status { display: inline-block; font-size: 0.75rem; padding: 0.125rem 0.5rem; border-radius: 9999px; background: var(--accent); color: white; margin-left: 0.5rem; vertical-align: middle; }
    .tag { display: inline-block; font-size: 0.75rem; padding: 0.125rem 0.375rem; border-radius: 0.25rem; border: 1px solid var(--border); color: var(--muted); margin-left: 0.25rem; }
    time { display: block; font-size: 0.8rem; color: var(--muted); margin-top: 0.25rem; }
    .children { margin-top: 0.75rem; padding-left: 1rem; }
    footer { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--border); color: var(--muted); font-size: 0.8rem; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(portal.name)}</h1>
    <p class="meta">${objectCount} objects &middot; Level ${portal.level} portal &middot; Generated ${escapeHtml(generatedAt)}</p>
  </header>
  <main>
${objectsHtml}  </main>
  <footer>
    <p>Powered by Prism Sovereign Portal</p>
  </footer>
</body>
</html>`;
}
