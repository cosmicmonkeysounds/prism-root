/**
 * Platform Panel — Calendar, Messaging, Reminders, Feeds.
 *
 * Lens #33 (Shift+I)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { PLATFORM_TYPES, EVENT_STATUSES, MESSAGE_STATUSES, REMINDER_STATUSES } from "@prism/core/layer1";

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

type PlatformTab = "calendar" | "messages" | "reminders" | "feeds";

export function PlatformPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<PlatformTab>("calendar");

  const events = objects.filter((o: GraphObject) => o.type === PLATFORM_TYPES.CALENDAR_EVENT);
  const messages = objects.filter((o: GraphObject) => o.type === PLATFORM_TYPES.MESSAGE);
  const reminders = objects.filter((o: GraphObject) => o.type === PLATFORM_TYPES.REMINDER);
  const feeds = objects.filter((o: GraphObject) => o.type === PLATFORM_TYPES.FEED);

  const create = useCallback((type: string, name: string, data: Record<string, unknown>, status: string) => {
    kernel.createObject({ type, name, parentId: null, position: 0, status, tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data });
  }, [kernel]);

  const del = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderBadge = (status: string | null, statuses: ReadonlyArray<{ value: string; label: string }>) => {
    const found = statuses.find((st) => st.value === status);
    return <span style={s.badge}>{found?.label ?? status ?? "—"}</span>;
  };

  return (
    <div style={s.root} data-testid="platform-panel">
      <div style={s.header}><span style={s.title}>Platform</span></div>
      <div style={s.tabs}>
        <button style={tab === "calendar" ? s.tabActive : s.tab} onClick={() => setTab("calendar")} data-testid="platform-tab-calendar">Calendar ({events.length})</button>
        <button style={tab === "messages" ? s.tabActive : s.tab} onClick={() => setTab("messages")} data-testid="platform-tab-messages">Messages ({messages.length})</button>
        <button style={tab === "reminders" ? s.tabActive : s.tab} onClick={() => setTab("reminders")} data-testid="platform-tab-reminders">Reminders ({reminders.length})</button>
        <button style={tab === "feeds" ? s.tabActive : s.tab} onClick={() => setTab("feeds")} data-testid="platform-tab-feeds">Feeds ({feeds.length})</button>
      </div>

      {tab === "calendar" && (
        <>
          <button style={s.btnPrimary} onClick={() => {
            const start = new Date();
            const end = new Date(start.getTime() + 3600000);
            create(PLATFORM_TYPES.CALENDAR_EVENT, "New Event", { startTime: start.toISOString(), endTime: end.toISOString(), recurrence: "none" }, "confirmed");
          }} data-testid="platform-new-event">+ New Event</button>
          {events.length === 0 && <div style={s.empty}>No events</div>}
          {events.map((e: GraphObject) => (
            <div key={e.id} style={s.card} data-testid={`platform-event-${e.id}`}>
              <div style={s.cardTitle}>{e.name}{renderBadge(e.status, EVENT_STATUSES)}</div>
              {e.data.location != null && <div style={s.field}><span style={s.label}>Location:</span> {String(e.data.location)}</div>}
              <button style={s.btn} onClick={() => del(e.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "messages" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(PLATFORM_TYPES.MESSAGE, "New Message", { channel: "internal" }, "draft")} data-testid="platform-new-message">+ Compose</button>
          {messages.length === 0 && <div style={s.empty}>No messages</div>}
          {messages.map((m: GraphObject) => (
            <div key={m.id} style={s.card} data-testid={`platform-message-${m.id}`}>
              <div style={s.cardTitle}>{m.data.subject ? String(m.data.subject) : m.name}{renderBadge(m.status, MESSAGE_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Channel:</span> {String(m.data.channel ?? "internal")}</div>
              <button style={s.btn} onClick={() => del(m.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "reminders" && (
        <>
          <button style={s.btnPrimary} onClick={() => {
            const due = new Date(Date.now() + 3600000);
            create(PLATFORM_TYPES.REMINDER, "Reminder", { dueAt: due.toISOString(), priority: "normal", recurring: "none" }, "pending");
          }} data-testid="platform-new-reminder">+ New Reminder</button>
          {reminders.length === 0 && <div style={s.empty}>No reminders</div>}
          {reminders.map((r: GraphObject) => (
            <div key={r.id} style={s.card} data-testid={`platform-reminder-${r.id}`}>
              <div style={s.cardTitle}>{r.name}{renderBadge(r.status, REMINDER_STATUSES)}</div>
              <button style={s.btn} onClick={() => del(r.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "feeds" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(PLATFORM_TYPES.FEED, "New Feed", { feedType: "rss", refreshIntervalMinutes: 60 }, "active")} data-testid="platform-new-feed">+ Add Feed</button>
          {feeds.length === 0 && <div style={s.empty}>No feeds</div>}
          {feeds.map((f: GraphObject) => (
            <div key={f.id} style={s.card} data-testid={`platform-feed-${f.id}`}>
              <div style={s.cardTitle}>{f.name}<span style={s.badge}>{String(f.data.feedType ?? "rss")}</span></div>
              <div style={s.field}><span style={s.label}>Items:</span> {String(f.data.itemCount ?? 0)}</div>
              <button style={s.btn} onClick={() => del(f.id)}>Delete</button>
            </div>
          ))}
        </>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const PLATFORM_LENS_ID = lensId("platform");

export const platformLensManifest: LensManifest = {

  id: PLATFORM_LENS_ID,
  name: "Platform",
  icon: "\u{1F4E1}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-platform", name: "Switch to Platform", shortcut: ["shift+i"], section: "Navigation" }],
  },
};

export const platformLensBundle: LensBundle = defineLensBundle(
  platformLensManifest,
  PlatformPanel,
);
