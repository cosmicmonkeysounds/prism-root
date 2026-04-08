/**
 * Work Panel — Gigs, Time Tracking, Focus Planner.
 *
 * Lists work entities from the kernel, supports CRUD for gigs,
 * time entries, and focus blocks.
 *
 * Lens #28 (Shift+W)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

import { WORK_TYPES, GIG_STATUSES, TIME_ENTRY_STATUSES, FOCUS_BLOCK_STATUSES } from "@prism/core/layer1";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Styles ──────────────────────────────────────────────────────────────────

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

type WorkTab = "gigs" | "time" | "focus";

export function WorkPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<WorkTab>("gigs");

  const gigs = objects.filter((o: GraphObject) => o.type === WORK_TYPES.GIG);
  const timeEntries = objects.filter((o: GraphObject) => o.type === WORK_TYPES.TIME_ENTRY);
  const focusBlocks = objects.filter((o: GraphObject) => o.type === WORK_TYPES.FOCUS_BLOCK);

  const createGig = useCallback(() => {
    kernel.createObject({ type: WORK_TYPES.GIG, name: "New Gig", parentId: null, position: gigs.length, status: "lead", tags: [], date: null, endDate: null, data: {}, description: "", color: null, image: null, pinned: false });
  }, [kernel, gigs.length]);

  const createTimeEntry = useCallback(() => {
    kernel.createObject({ type: WORK_TYPES.TIME_ENTRY, name: "Time Entry", parentId: null, position: timeEntries.length, status: "running", tags: [], date: null, endDate: null, data: { startTime: new Date().toISOString(), billable: true }, description: "", color: null, image: null, pinned: false });
  }, [kernel, timeEntries.length]);

  const createFocusBlock = useCallback(() => {
    const now = new Date();
    const end = new Date(now.getTime() + 90 * 60000);
    kernel.createObject({ type: WORK_TYPES.FOCUS_BLOCK, name: "Focus Block", parentId: null, position: focusBlocks.length, status: "planned", tags: [], date: null, endDate: null, data: { scheduledStart: now.toISOString(), scheduledEnd: end.toISOString(), focusType: "deep_work" }, description: "", color: null, image: null, pinned: false });
  }, [kernel, focusBlocks.length]);

  const deleteObject = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderStatusBadge = (status: string | null, statuses: ReadonlyArray<{ value: string; label: string }>) => {
    const found = statuses.find((s) => s.value === status);
    return <span style={s.badge}>{found?.label ?? status ?? "—"}</span>;
  };

  return (
    <div style={s.root} data-testid="work-panel">
      <div style={s.header}>
        <span style={s.title}>Work</span>
      </div>

      <div style={s.tabs}>
        <button style={tab === "gigs" ? s.tabActive : s.tab} onClick={() => setTab("gigs")} data-testid="work-tab-gigs">Gigs ({gigs.length})</button>
        <button style={tab === "time" ? s.tabActive : s.tab} onClick={() => setTab("time")} data-testid="work-tab-time">Time ({timeEntries.length})</button>
        <button style={tab === "focus" ? s.tabActive : s.tab} onClick={() => setTab("focus")} data-testid="work-tab-focus">Focus ({focusBlocks.length})</button>
      </div>

      {tab === "gigs" && (
        <>
          <button style={s.btnPrimary} onClick={createGig} data-testid="work-new-gig">+ New Gig</button>
          {gigs.length === 0 && <div style={s.empty}>No gigs yet</div>}
          {gigs.map((g: GraphObject) => (
            <div key={g.id} style={s.card} data-testid={`work-gig-${g.id}`}>
              <div style={s.cardTitle}>{g.name}{renderStatusBadge(g.status, GIG_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Rate:</span> {String(g.data.rate ?? "—")} / {String(g.data.rateUnit ?? "hourly")}</div>
              <div style={s.field}><span style={s.label}>Hours:</span> {String(g.data.actualHours ?? 0)} / {String(g.data.estimatedHours ?? "—")}</div>
              <button style={s.btn} onClick={() => deleteObject(g.id)} data-testid={`work-delete-${g.id}`}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "time" && (
        <>
          <button style={s.btnPrimary} onClick={createTimeEntry} data-testid="work-new-time">+ Start Timer</button>
          {timeEntries.length === 0 && <div style={s.empty}>No time entries</div>}
          {timeEntries.map((t: GraphObject) => (
            <div key={t.id} style={s.card} data-testid={`work-time-${t.id}`}>
              <div style={s.cardTitle}>{t.name}{renderStatusBadge(t.status, TIME_ENTRY_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Duration:</span> {String(t.data.durationMinutes ?? "—")} min</div>
              <div style={s.field}><span style={s.label}>Billable:</span> {t.data.billable ? "Yes" : "No"}</div>
              <button style={s.btn} onClick={() => deleteObject(t.id)} data-testid={`work-delete-${t.id}`}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "focus" && (
        <>
          <button style={s.btnPrimary} onClick={createFocusBlock} data-testid="work-new-focus">+ New Focus Block</button>
          {focusBlocks.length === 0 && <div style={s.empty}>No focus blocks</div>}
          {focusBlocks.map((f: GraphObject) => (
            <div key={f.id} style={s.card} data-testid={`work-focus-${f.id}`}>
              <div style={s.cardTitle}>{f.name}{renderStatusBadge(f.status, FOCUS_BLOCK_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Type:</span> {String(f.data.focusType ?? "deep_work")}</div>
              <button style={s.btn} onClick={() => deleteObject(f.id)} data-testid={`work-delete-${f.id}`}>Delete</button>
            </div>
          ))}
        </>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const WORK_LENS_ID = lensId("work");

export const workLensManifest: LensManifest = {

  id: WORK_LENS_ID,
  name: "Work",
  icon: "\u{1F4BC}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-work", name: "Switch to Work", shortcut: ["shift+w"], section: "Navigation" }],
  },
};

export const workLensBundle: LensBundle = defineLensBundle(
  workLensManifest,
  WorkPanel,
);
