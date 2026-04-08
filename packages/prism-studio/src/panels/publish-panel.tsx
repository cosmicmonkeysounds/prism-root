/**
 * Publish Panel — export + publish workflow + preview mode for pages.
 *
 * Tier 6 of `docs/dev/studio-checklist.md`:
 *   6A. HTML Export   — download the selected page as static HTML
 *   6B. JSON Export   — download the selected page as a JSON snapshot
 *   6C. Publish Workflow — draft → review → published state machine on pages
 *   6D. Preview Mode  — read-only rendered preview of the page tree
 *
 * The panel resolves the current page from the selection (walking up
 * parentId chains for child selection) and drives the export + status
 * transitions through the kernel. Previewing renders the same
 * `renderNodeHtml()` pipeline as the exporter so authors see exactly
 * what the HTML file will contain.
 */

import { useMemo, useState, useCallback } from "react";
import type { CSSProperties } from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { useKernel, useSelection, useObject } from "../kernel/index.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  exportPageToHtml,
  exportPageToJson,
  renderNodeHtml,
  toExportedNode,
} from "../kernel/page-export.js";

// ── Shared styles ───────────────────────────────────────────────────────────

const panelStyle: CSSProperties = {
  height: "100%",
  display: "flex",
  flexDirection: "column",
  background: "#1e1e1e",
  color: "#d4d4d4",
  fontFamily: "system-ui, sans-serif",
};

const headerStyle: CSSProperties = {
  padding: "12px 16px",
  borderBottom: "1px solid #333",
  background: "#252526",
  display: "flex",
  alignItems: "center",
  gap: 12,
  flexWrap: "wrap",
};

const bodyStyle: CSSProperties = {
  flex: 1,
  overflow: "auto",
  padding: "16px",
};

const btnStyle: CSSProperties = {
  background: "#0e639c",
  color: "#fff",
  border: "1px solid transparent",
  borderRadius: 4,
  padding: "6px 12px",
  cursor: "pointer",
  fontSize: 12,
};

const btnSecondary: CSSProperties = {
  ...btnStyle,
  background: "#3c3c3c",
};

const statusPill = (color: string): CSSProperties => ({
  display: "inline-block",
  padding: "2px 10px",
  borderRadius: 999,
  background: color,
  color: "#0f172a",
  fontSize: 11,
  fontWeight: 600,
  textTransform: "uppercase",
  letterSpacing: "0.05em",
});

// ── Status helpers ──────────────────────────────────────────────────────────

type PageStatus = "draft" | "review" | "published";
const STATUS_ORDER: PageStatus[] = ["draft", "review", "published"];

export function nextStatus(current: string | null | undefined): PageStatus {
  if (!current) return "draft";
  const idx = STATUS_ORDER.indexOf(current as PageStatus);
  if (idx === -1) return "draft";
  if (idx >= STATUS_ORDER.length - 1) return "published";
  return STATUS_ORDER[idx + 1] as PageStatus;
}

export function statusColor(s: string | null | undefined): string {
  switch (s) {
    case "draft":
      return "#e2e8f0";
    case "review":
      return "#fde68a";
    case "published":
      return "#bbf7d0";
    default:
      return "#e2e8f0";
  }
}

// ── Page resolution ─────────────────────────────────────────────────────────

function resolvePageId(
  selectedId: ObjectId | null,
  getObject: (id: ObjectId) => GraphObject | undefined,
): ObjectId | null {
  if (!selectedId) return null;
  let current = getObject(selectedId);
  while (current && current.type !== "page") {
    if (!current.parentId) return null;
    current = getObject(current.parentId);
  }
  return current ? current.id : null;
}

// ── Download helper ─────────────────────────────────────────────────────────

function triggerDownload(filename: string, mime: string, content: string) {
  if (typeof document === "undefined" || typeof URL === "undefined") return;
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function safeFilename(input: string, fallback = "page"): string {
  const cleaned = input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-_.]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return cleaned || fallback;
}

// ── Component ───────────────────────────────────────────────────────────────

export function PublishPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selectedObj = useObject(selectedId);
  const [previewMode, setPreviewMode] = useState(false);

  const pageId = useMemo(
    () =>
      resolvePageId(selectedId, (id) => kernel.store.getObject(id) ?? undefined),
    [selectedId, kernel.store, selectedObj],
  );

  const page = pageId ? kernel.store.getObject(pageId) : null;
  const allObjects = useMemo(
    () => kernel.store.allObjects().filter((o) => !o.deletedAt),
    [kernel.store, page],
  );

  const downloadHtml = useCallback(() => {
    if (!page) return;
    const html = exportPageToHtml(page, allObjects);
    const filename = `${safeFilename(page.name)}.html`;
    triggerDownload(filename, "text/html;charset=utf-8", html);
    kernel.notifications.add({ title: `Exported ${filename}`, kind: "success" });
  }, [page, allObjects, kernel]);

  const downloadJson = useCallback(() => {
    if (!page) return;
    const json = exportPageToJson(page, allObjects);
    const filename = `${safeFilename(page.name)}.json`;
    triggerDownload(filename, "application/json", JSON.stringify(json, null, 2));
    kernel.notifications.add({ title: `Exported ${filename}`, kind: "success" });
  }, [page, allObjects, kernel]);

  const advance = useCallback(() => {
    if (!page) return;
    const next = nextStatus(page.status);
    kernel.updateObject(page.id, { status: next });
    kernel.notifications.add({
      title: `Page status → ${next}`,
      kind: next === "published" ? "success" : "info",
    });
  }, [page, kernel]);

  const reset = useCallback(() => {
    if (!page) return;
    kernel.updateObject(page.id, { status: "draft" });
    kernel.notifications.add({ title: "Page reset to draft", kind: "info" });
  }, [page, kernel]);

  const previewHtml = useMemo(() => {
    if (!page || !previewMode) return "";
    const byParent = new Map<ObjectId | null, GraphObject[]>();
    for (const obj of allObjects) {
      const parentId = obj.parentId ?? null;
      const bucket = byParent.get(parentId);
      if (bucket) bucket.push(obj);
      else byParent.set(parentId, [obj]);
    }
    for (const bucket of byParent.values()) {
      bucket.sort((a, b) => a.position - b.position);
    }
    return renderNodeHtml(toExportedNode(page, byParent));
  }, [page, previewMode, allObjects]);

  if (!page) {
    return (
      <div style={panelStyle} data-testid="publish-panel">
        <div style={headerStyle}>
          <strong style={{ fontSize: 14 }}>Publish</strong>
        </div>
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#888",
            fontSize: 13,
          }}
        >
          Select a page to export or publish.
        </div>
      </div>
    );
  }

  return (
    <div style={panelStyle} data-testid="publish-panel">
      <div style={headerStyle}>
        <strong style={{ fontSize: 14 }}>Publish</strong>
        <span style={{ color: "#888", fontSize: 12 }}>·</span>
        <span style={{ fontSize: 13 }}>{page.name}</span>
        <span data-testid="publish-status" style={statusPill(statusColor(page.status))}>
          {page.status}
        </span>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          data-testid="publish-toggle-preview"
          onClick={() => setPreviewMode((v) => !v)}
          style={btnSecondary}
        >
          {previewMode ? "Exit Preview" : "Preview Mode"}
        </button>
        <button
          type="button"
          data-testid="publish-export-html"
          onClick={downloadHtml}
          style={btnSecondary}
        >
          Export HTML
        </button>
        <button
          type="button"
          data-testid="publish-export-json"
          onClick={downloadJson}
          style={btnSecondary}
        >
          Export JSON
        </button>
        {page.status !== "published" ? (
          <button
            type="button"
            data-testid="publish-advance"
            onClick={advance}
            style={btnStyle}
          >
            {page.status === "draft" ? "Request Review" : "Publish"}
          </button>
        ) : (
          <button
            type="button"
            data-testid="publish-reset"
            onClick={reset}
            style={btnSecondary}
          >
            Back to Draft
          </button>
        )}
      </div>
      <div style={bodyStyle}>
        {previewMode ? (
          <div
            data-testid="publish-preview"
            style={{
              background: "#ffffff",
              color: "#0f172a",
              borderRadius: 8,
              padding: 16,
              minHeight: "100%",
            }}
            dangerouslySetInnerHTML={{ __html: previewHtml }}
          />
        ) : (
          <div style={{ color: "#a1a1aa", fontSize: 13, lineHeight: 1.6 }}>
            <p>
              Use the header to export the page as HTML or JSON, toggle a
              read-only preview, or advance the page through the publish
              workflow.
            </p>
            <ul>
              <li>
                <strong>Draft</strong> — editable, not yet reviewed.
              </li>
              <li>
                <strong>Review</strong> — ready for approval.
              </li>
              <li>
                <strong>Published</strong> — live.
              </li>
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const PUBLISH_LENS_ID = lensId("publish");

export const publishLensManifest: LensManifest = {

  id: PUBLISH_LENS_ID,
  name: "Publish",
  icon: "\u{1F680}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      {
        id: "switch-publish",
        name: "Switch to Publish",
        shortcut: ["shift+u"],
        section: "Navigation",
      },
    ],
  },
};

export const publishLensBundle: LensBundle = defineLensBundle(
  publishLensManifest,
  PublishPanel,
);
