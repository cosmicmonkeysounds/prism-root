/**
 * Assets Management Panel — Media, Content, Documents, Collections.
 *
 * Lens #32 (Shift+M)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { ASSETS_TYPES, MEDIA_STATUSES, CONTENT_STATUSES, SCAN_STATUSES, MEDIA_KINDS } from "@prism/core/layer1";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
const s: Record<string, CSSProperties> = {
  root: { padding: 16, height: "100%", overflow: "auto", fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a" },
  header: { display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 },
  title: { fontSize: 18, fontWeight: 600, color: "#e5e5e5" },
  tabs: { display: "flex", gap: 4, marginBottom: 12 },
  tab: { padding: "6px 12px", border: "1px solid #444", borderRadius: 4, background: "#252526", cursor: "pointer", color: "#ccc" },
  tabActive: { padding: "6px 12px", border: "1px solid #4a9eff", borderRadius: 4, background: "#1e3a5f", cursor: "pointer", color: "#fff" },
  card: { background: "#252526", border: "1px solid #333", borderRadius: 6, padding: 12, marginBottom: 8 },
  cardTitle: { fontWeight: 600, color: "#e5e5e5", marginBottom: 4 },
  badge: { display: "inline-block", padding: "2px 8px", borderRadius: 10, fontSize: 11, background: "#333", marginLeft: 6 },
  btn: { padding: "6px 12px", border: "1px solid #555", borderRadius: 4, background: "#333", color: "#ccc", cursor: "pointer" },
  btnPrimary: { padding: "6px 12px", border: "none", borderRadius: 4, background: "#4a9eff", color: "#fff", cursor: "pointer" },
  field: { display: "flex", gap: 8, alignItems: "center", marginBottom: 4 },
  label: { color: "#888", minWidth: 80 },
  empty: { color: "#666", fontStyle: "italic", textAlign: "center" as const, padding: 32 },
};

type AssetsTab = "media" | "content" | "documents" | "collections";

export function AssetsMgmtPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<AssetsTab>("media");

  const media = objects.filter((o: GraphObject) => o.type === ASSETS_TYPES.MEDIA_ASSET);
  const content = objects.filter((o: GraphObject) => o.type === ASSETS_TYPES.CONTENT_ITEM);
  const docs = objects.filter((o: GraphObject) => o.type === ASSETS_TYPES.SCANNED_DOC);
  const collections = objects.filter((o: GraphObject) => o.type === ASSETS_TYPES.COLLECTION);

  const create = useCallback((type: string, name: string, data: Record<string, unknown>, status: string) => {
    kernel.createObject({ type, name, parentId: null, position: 0, status, tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data });
  }, [kernel]);

  const del = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderBadge = (status: string | null, statuses: ReadonlyArray<{ value: string; label: string }>) => {
    const found = statuses.find((st) => st.value === status);
    return <span style={s.badge}>{found?.label ?? status ?? "—"}</span>;
  };

  return (
    <div style={s.root} data-testid="assets-mgmt-panel">
      <div style={s.header}><span style={s.title}>Assets</span></div>
      <div style={s.tabs}>
        <button style={tab === "media" ? s.tabActive : s.tab} onClick={() => setTab("media")} data-testid="assets-tab-media">Media ({media.length})</button>
        <button style={tab === "content" ? s.tabActive : s.tab} onClick={() => setTab("content")} data-testid="assets-tab-content">Content ({content.length})</button>
        <button style={tab === "documents" ? s.tabActive : s.tab} onClick={() => setTab("documents")} data-testid="assets-tab-docs">Documents ({docs.length})</button>
        <button style={tab === "collections" ? s.tabActive : s.tab} onClick={() => setTab("collections")} data-testid="assets-tab-collections">Collections ({collections.length})</button>
      </div>

      {tab === "media" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(ASSETS_TYPES.MEDIA_ASSET, "New Media", { mediaKind: "image" }, "ready")} data-testid="assets-new-media">+ Add Media</button>
          {media.length === 0 && <div style={s.empty}>No media assets</div>}
          {media.map((m: GraphObject) => (
            <div key={m.id} style={s.card} data-testid={`assets-media-${m.id}`}>
              <div style={s.cardTitle}>{m.name}{renderBadge(m.status, MEDIA_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Kind:</span> {String(MEDIA_KINDS.find((k) => k.value === m.data.mediaKind)?.label ?? m.data.mediaKind)}</div>
              <button style={s.btn} onClick={() => del(m.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "content" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(ASSETS_TYPES.CONTENT_ITEM, "New Content", { contentType: "note" }, "draft")} data-testid="assets-new-content">+ New Content</button>
          {content.length === 0 && <div style={s.empty}>No content items</div>}
          {content.map((c: GraphObject) => (
            <div key={c.id} style={s.card} data-testid={`assets-content-${c.id}`}>
              <div style={s.cardTitle}>{c.name}{renderBadge(c.status, CONTENT_STATUSES)}</div>
              <button style={s.btn} onClick={() => del(c.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "documents" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(ASSETS_TYPES.SCANNED_DOC, "New Scan", {}, "pending")} data-testid="assets-new-doc">+ Scan Document</button>
          {docs.length === 0 && <div style={s.empty}>No scanned documents</div>}
          {docs.map((d: GraphObject) => (
            <div key={d.id} style={s.card} data-testid={`assets-doc-${d.id}`}>
              <div style={s.cardTitle}>{d.name}{renderBadge(d.status, SCAN_STATUSES)}</div>
              <button style={s.btn} onClick={() => del(d.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "collections" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(ASSETS_TYPES.COLLECTION, "New Collection", {}, "active")} data-testid="assets-new-collection">+ New Collection</button>
          {collections.length === 0 && <div style={s.empty}>No collections</div>}
          {collections.map((c: GraphObject) => (
            <div key={c.id} style={s.card} data-testid={`assets-collection-${c.id}`}>
              <div style={s.cardTitle}>{c.name}</div>
              <div style={s.field}><span style={s.label}>Items:</span> {String(c.data.itemCount ?? 0)}</div>
              <button style={s.btn} onClick={() => del(c.id)}>Delete</button>
            </div>
          ))}
        </>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const ASSETS_MGMT_LENS_ID = lensId("assets-mgmt");

export const assetsMgmtLensManifest: LensManifest = {

  id: ASSETS_MGMT_LENS_ID,
  name: "Asset Manager",
  icon: "\u{1F4E6}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-assets-mgmt", name: "Switch to Asset Manager", shortcut: ["shift+m"], section: "Navigation" }],
  },
};

export const assetsMgmtLensBundle: LensBundle = defineLensBundle(
  assetsMgmtLensManifest,
  AssetsMgmtPanel,
);
