/**
 * Page export — serialise a page object tree to portable formats.
 *
 * Provides pure functions that walk a page → section → component tree and
 * emit:
 *   - `exportPageToJson()` — deterministic JSON snapshot with children inlined
 *   - `exportPageToHtml()` — dependency-free static HTML + inline CSS
 *
 * These functions are input/output only — no kernel mutations, no Loro
 * access. Callers pass the relevant objects in. This keeps the logic easy
 * to test and safe to run inside a worker if needed later.
 *
 * Part of Tier 6 of `docs/dev/studio-checklist.md`: HTML Export, JSON Export,
 * Publish Workflow, Preview Mode.
 */

import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { computeBlockStyle, extractBlockStyle, type BlockStyleData } from "./block-style.js";
import { collectFontFamilies, googleFontsHref } from "./fonts.js";

// ── JSON export ─────────────────────────────────────────────────────────────

export interface ExportedNode {
  id: ObjectId;
  type: string;
  name: string;
  position: number;
  status: string | null;
  tags: ReadonlyArray<string>;
  data: Record<string, unknown>;
  children: ExportedNode[];
}

export interface ExportedPage {
  format: "prism-page/v1";
  exportedAt: string;
  page: ExportedNode;
}

/** Build a child-lookup index keyed by parentId. */
function indexByParent(
  objects: ReadonlyArray<GraphObject>,
): Map<ObjectId | null, GraphObject[]> {
  const byParent = new Map<ObjectId | null, GraphObject[]>();
  for (const obj of objects) {
    if (obj.deletedAt) continue;
    const parentId = obj.parentId ?? null;
    const bucket = byParent.get(parentId);
    if (bucket) bucket.push(obj);
    else byParent.set(parentId, [obj]);
  }
  for (const bucket of byParent.values()) {
    bucket.sort((a, b) => a.position - b.position);
  }
  return byParent;
}

/** Convert a GraphObject subtree to an `ExportedNode` tree. */
export function toExportedNode(
  root: GraphObject,
  byParent: Map<ObjectId | null, GraphObject[]>,
): ExportedNode {
  const kids = byParent.get(root.id) ?? [];
  return {
    id: root.id,
    type: root.type,
    name: root.name,
    position: root.position,
    status: root.status,
    tags: [...(root.tags ?? [])],
    data: { ...(root.data ?? {}) } as Record<string, unknown>,
    children: kids.map((child) => toExportedNode(child, byParent)),
  };
}

/** Export a page as a self-describing JSON snapshot. */
export function exportPageToJson(
  page: GraphObject,
  allObjects: ReadonlyArray<GraphObject>,
  now: () => Date = () => new Date(),
): ExportedPage {
  const byParent = indexByParent(allObjects);
  return {
    format: "prism-page/v1",
    exportedAt: now().toISOString(),
    page: toExportedNode(page, byParent),
  };
}

// ── HTML export ─────────────────────────────────────────────────────────────

/** Escape HTML-significant characters so text renders literally. */
export function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Escape a string safe for use inside a double-quoted HTML attribute. */
export function escapeAttr(input: string): string {
  return input.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

/** Serialise a CSSProperties object into an inline style attribute. */
export function cssToInline(css: Record<string, unknown>): string {
  const parts: string[] = [];
  for (const [key, value] of Object.entries(css)) {
    if (value === undefined || value === null || value === "") continue;
    const cssKey = key.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`);
    const cssVal = typeof value === "number" && !cssKey.endsWith("line-height") ? `${value}px` : String(value);
    parts.push(`${cssKey}: ${cssVal}`);
  }
  return parts.join("; ");
}

/** Apply user-defined BlockStyleData as an inline style string. */
function blockStyleAttr(data: unknown): string {
  const bs = extractBlockStyle(data) as BlockStyleData;
  const css = computeBlockStyle(bs) as unknown as Record<string, unknown>;
  const inline = cssToInline(css);
  return inline ? ` style="${escapeAttr(inline)}"` : "";
}

/** Render one exported node as HTML. */
export function renderNodeHtml(node: ExportedNode, depth = 0): string {
  const d = node.data as Record<string, unknown>;
  const style = blockStyleAttr(d);
  const innerChildren = node.children
    .map((child) => renderNodeHtml(child, depth + 1))
    .join("\n");

  switch (node.type) {
    case "page": {
      const title = escapeHtml((d["title"] as string) ?? node.name);
      return `<main data-page-id="${escapeAttr(node.id)}"${style}>\n<h1>${title}</h1>\n${innerChildren}\n</main>`;
    }
    case "section": {
      return `<section data-section-id="${escapeAttr(node.id)}"${style}>\n${innerChildren}\n</section>`;
    }
    case "heading": {
      const level = ((d["level"] as string) ?? "h2").toLowerCase();
      const tag = ["h1", "h2", "h3", "h4", "h5", "h6"].includes(level) ? level : "h2";
      const text = escapeHtml((d["text"] as string) ?? "");
      return `<${tag}${style}>${text}</${tag}>`;
    }
    case "text-block": {
      const content = escapeHtml((d["content"] as string) ?? "");
      return `<p${style}>${content}</p>`;
    }
    case "image": {
      const src = escapeAttr((d["src"] as string) ?? "");
      const alt = escapeAttr((d["alt"] as string) ?? "");
      const caption = d["caption"] ? `<figcaption>${escapeHtml(d["caption"] as string)}</figcaption>` : "";
      return `<figure${style}><img src="${src}" alt="${alt}"/>${caption}</figure>`;
    }
    case "button": {
      const label = escapeHtml((d["label"] as string) ?? "Click");
      const href = (d["href"] as string) ?? "";
      if (href) {
        return `<a class="btn" href="${escapeAttr(href)}"${style}>${label}</a>`;
      }
      return `<button${style}>${label}</button>`;
    }
    case "card": {
      const title = d["title"] ? `<h3>${escapeHtml(d["title"] as string)}</h3>` : "";
      const body = d["body"] ? `<p>${escapeHtml(d["body"] as string)}</p>` : "";
      return `<article class="card"${style}>${title}${body}${innerChildren}</article>`;
    }
    case "markdown-widget": {
      const source = (d["source"] as string) ?? "";
      return `<div class="markdown"${style}>${escapeHtml(source)}</div>`;
    }
    case "code-block": {
      const source = (d["source"] as string) ?? "";
      const lang = (d["language"] as string) ?? "text";
      return `<pre class="code"${style}><code data-language="${escapeAttr(lang)}">${escapeHtml(source)}</code></pre>`;
    }
    case "video-widget": {
      const src = escapeAttr((d["src"] as string) ?? "");
      const poster = d["poster"] ? ` poster="${escapeAttr(d["poster"] as string)}"` : "";
      return `<video controls${poster}${style}><source src="${src}"/></video>`;
    }
    case "audio-widget": {
      const src = escapeAttr((d["src"] as string) ?? "");
      return `<audio controls${style}><source src="${src}"/></audio>`;
    }
    case "iframe-widget": {
      const src = escapeAttr((d["src"] as string) ?? "");
      const title = escapeAttr((d["title"] as string) ?? "Embedded");
      const height = (d["height"] as number) ?? 360;
      return `<iframe src="${src}" title="${title}" height="${height}" loading="lazy"${style}></iframe>`;
    }
    case "divider":
      return `<hr${style}/>`;
    case "spacer": {
      const size = (d["size"] as number) ?? 16;
      return `<div aria-hidden="true" style="height: ${size}px"></div>`;
    }
    default:
      return `<div data-block-type="${escapeAttr(node.type)}"${style}>${innerChildren}</div>`;
  }
}

const DEFAULT_HTML_STYLES = `body{font-family:system-ui,sans-serif;max-width:960px;margin:0 auto;padding:24px;line-height:1.55;color:#0f172a;background:#ffffff}
h1,h2,h3{color:#0f172a;margin-top:1.5em}
section{margin-bottom:24px}
.card{border:1px solid #e2e8f0;border-radius:8px;padding:16px;background:#f8fafc}
.btn{display:inline-block;padding:8px 16px;border-radius:6px;background:#3b82f6;color:#fff;text-decoration:none}
pre.code{background:#0f172a;color:#e2e8f0;padding:12px;border-radius:6px;overflow:auto}
figure{margin:0 0 16px 0}figure img{max-width:100%;height:auto;border-radius:6px}
iframe{border:0;width:100%;border-radius:6px}
video,audio{width:100%}`;

export interface HtmlExportOptions {
  /** Page title used for `<title>`. Defaults to the page's name or title field. */
  title?: string;
  /** Override the inline CSS. Defaults to a neutral stylesheet. */
  inlineCss?: string;
  /** Skip the document wrapper, returning just the body fragment. */
  fragmentOnly?: boolean;
}

/** Export a page to a standalone HTML document (or fragment). */
export function exportPageToHtml(
  page: GraphObject,
  allObjects: ReadonlyArray<GraphObject>,
  options: HtmlExportOptions = {},
): string {
  const byParent = indexByParent(allObjects);
  const node = toExportedNode(page, byParent);
  const body = renderNodeHtml(node);
  if (options.fragmentOnly) return body;

  const d = (page.data ?? {}) as Record<string, unknown>;
  const title = escapeHtml(options.title ?? (d["title"] as string) ?? page.name ?? "Untitled");
  const css = options.inlineCss ?? DEFAULT_HTML_STYLES;
  const fontFamilies = collectFontFamilies(node);
  const fontHref = googleFontsHref(fontFamilies);
  const fontLink = fontHref
    ? `\n<link rel="preconnect" href="https://fonts.googleapis.com"/>\n<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin/>\n<link rel="stylesheet" href="${escapeAttr(fontHref)}"/>`
    : "";
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>${title}</title>${fontLink}
<style>${css}</style>
</head>
<body>
${body}
</body>
</html>`;
}
