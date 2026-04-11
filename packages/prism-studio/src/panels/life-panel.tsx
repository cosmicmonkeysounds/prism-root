/**
 * Life Panel — Habits, Fitness, Sleep, Journal, Meals, Cycle.
 *
 * Lens #31 (Shift+H)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { LIFE_TYPES, JOURNAL_MOODS, WORKOUT_TYPES, SLEEP_QUALITY } from "@prism/core/plugin-bundles";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
const s: Record<string, CSSProperties> = {
  root: { padding: 16, height: "100%", overflow: "auto", fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a" },
  header: { display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 },
  title: { fontSize: 18, fontWeight: 600, color: "#e5e5e5" },
  tabs: { display: "flex", gap: 4, marginBottom: 12, flexWrap: "wrap" as const },
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

type LifeTab = "habits" | "fitness" | "sleep" | "journal" | "meals" | "cycle";
const today = () => new Date().toISOString().slice(0, 10);

export function LifePanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<LifeTab>("habits");

  const habits = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.HABIT);
  const fitness = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.FITNESS_LOG);
  const sleep = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.SLEEP_RECORD);
  const journal = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.JOURNAL_ENTRY);
  const meals = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.MEAL_PLAN);
  const cycle = objects.filter((o: GraphObject) => o.type === LIFE_TYPES.CYCLE_ENTRY);

  const create = useCallback((type: string, name: string, data: Record<string, unknown>) => {
    kernel.createObject({ type, name, parentId: null, position: 0, status: "active", tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data });
  }, [kernel]);

  const del = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderBadge = (val: unknown, options: ReadonlyArray<{ value: string; label: string }>) => {
    const found = options.find((o) => o.value === val);
    return found ? <span style={s.badge}>{found.label}</span> : null;
  };

  return (
    <div style={s.root} data-testid="life-panel">
      <div style={s.header}><span style={s.title}>Life</span></div>
      <div style={s.tabs}>
        {(["habits", "fitness", "sleep", "journal", "meals", "cycle"] as LifeTab[]).map((t) => (
          <button key={t} style={tab === t ? s.tabActive : s.tab} onClick={() => setTab(t)} data-testid={`life-tab-${t}`}>
            {t.charAt(0).toUpperCase() + t.slice(1)}
          </button>
        ))}
      </div>

      {tab === "habits" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.HABIT, "New Habit", { frequency: "daily", targetCount: 1, streak: 0 })} data-testid="life-new-habit">+ New Habit</button>
          {habits.length === 0 && <div style={s.empty}>No habits</div>}
          {habits.map((h: GraphObject) => (
            <div key={h.id} style={s.card} data-testid={`life-habit-${h.id}`}>
              <div style={s.cardTitle}>{h.name}</div>
              <div style={s.field}><span style={s.label}>Streak:</span> {String(h.data.streak ?? 0)} days</div>
              <div style={s.field}><span style={s.label}>Frequency:</span> {String(h.data.frequency ?? "daily")}</div>
              <button style={s.btn} onClick={() => del(h.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "fitness" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.FITNESS_LOG, "Workout", { workoutType: "strength", date: today() })} data-testid="life-new-workout">+ Log Workout</button>
          {fitness.length === 0 && <div style={s.empty}>No workouts</div>}
          {fitness.map((f: GraphObject) => (
            <div key={f.id} style={s.card} data-testid={`life-fitness-${f.id}`}>
              <div style={s.cardTitle}>{f.name}{renderBadge(f.data.workoutType, WORKOUT_TYPES)}</div>
              <div style={s.field}><span style={s.label}>Duration:</span> {String(f.data.durationMinutes ?? "—")} min</div>
              <button style={s.btn} onClick={() => del(f.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "sleep" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.SLEEP_RECORD, "Sleep", { date: today() })} data-testid="life-new-sleep">+ Log Sleep</button>
          {sleep.length === 0 && <div style={s.empty}>No sleep records</div>}
          {sleep.map((sr: GraphObject) => (
            <div key={sr.id} style={s.card} data-testid={`life-sleep-${sr.id}`}>
              <div style={s.cardTitle}>{sr.name}{renderBadge(sr.data.quality, SLEEP_QUALITY)}</div>
              <div style={s.field}><span style={s.label}>Duration:</span> {String(sr.data.durationHours ?? "—")} hrs</div>
              <button style={s.btn} onClick={() => del(sr.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "journal" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.JOURNAL_ENTRY, "Journal Entry", { date: today(), isPrivate: true })} data-testid="life-new-journal">+ New Entry</button>
          {journal.length === 0 && <div style={s.empty}>No journal entries</div>}
          {journal.map((j: GraphObject) => (
            <div key={j.id} style={s.card} data-testid={`life-journal-${j.id}`}>
              <div style={s.cardTitle}>{j.name}{renderBadge(j.data.mood, JOURNAL_MOODS)}</div>
              {!!j.data.content && <div style={{ color: "#999", marginTop: 4 }}>{String(j.data.content).slice(0, 120)}</div>}
              <button style={s.btn} onClick={() => del(j.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "meals" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.MEAL_PLAN, "Meal", { date: today(), mealType: "breakfast" })} data-testid="life-new-meal">+ Log Meal</button>
          {meals.length === 0 && <div style={s.empty}>No meals</div>}
          {meals.map((m: GraphObject) => (
            <div key={m.id} style={s.card} data-testid={`life-meal-${m.id}`}>
              <div style={s.cardTitle}>{m.name}<span style={s.badge}>{String(m.data.mealType ?? "—")}</span></div>
              <div style={s.field}><span style={s.label}>Calories:</span> {String(m.data.calories ?? "—")}</div>
              <button style={s.btn} onClick={() => del(m.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "cycle" && (
        <>
          <button style={s.btnPrimary} onClick={() => create(LIFE_TYPES.CYCLE_ENTRY, "Cycle Entry", { date: today(), isPrivate: true })} data-testid="life-new-cycle">+ New Entry</button>
          {cycle.length === 0 && <div style={s.empty}>No cycle entries</div>}
          {cycle.map((c: GraphObject) => (
            <div key={c.id} style={s.card} data-testid={`life-cycle-${c.id}`}>
              <div style={s.cardTitle}>{c.name}{c.data.phase != null && <span style={s.badge}>{String(c.data.phase)}</span>}</div>
              <button style={s.btn} onClick={() => del(c.id)}>Delete</button>
            </div>
          ))}
        </>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const LIFE_LENS_ID = lensId("life");

export const lifeLensManifest: LensManifest = {

  id: LIFE_LENS_ID,
  name: "Life",
  icon: "\u{1F33F}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-life", name: "Switch to Life", shortcut: ["shift+h"], section: "Navigation" }],
  },
};

export const lifeLensBundle: LensBundle = defineLensBundle(
  lifeLensManifest,
  LifePanel,
);
