/**
 * SavedViewPanel — Found Set manager with view switcher, quick filters, and save dialog.
 *
 * Lists saved views for the current object type. Users can:
 *   - Save the current filter/sort/group config as a named view
 *   - Switch between saved views (loads config into LiveView)
 *   - Pin favorite views
 *   - Search views by name
 *   - Delete views
 *
 * Found Sets are applied to list/table/card-grid/report widgets composed into pages.
 *
 * Lens #25 (Shift+V)
 */

import { useState, useMemo, useCallback, type CSSProperties } from "react";
import { useKernel } from "../kernel/kernel-context.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  type SavedView,
  type ViewConfig,
  createSavedView,
  createSavedViewRegistry,
} from "@prism/core/layer1";

// ── Styles ──────────────────────────────────────────────────────────────────

const s: Record<string, CSSProperties> = {
  root: { padding: 12, fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a", height: "100%", overflow: "auto" },
  header: { display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 },
  title: { fontWeight: 700, fontSize: 15 },
  search: { background: "#333", border: "1px solid #555", borderRadius: 4, padding: "4px 8px", color: "#ccc", fontSize: 13, width: "100%", marginBottom: 8 },
  card: { background: "#252525", border: "1px solid #444", borderRadius: 4, padding: 8, marginBottom: 6, cursor: "pointer" },
  cardActive: { borderColor: "#3b82f6", background: "#1e2a3a" },
  cardHeader: { display: "flex", justifyContent: "space-between", alignItems: "center" },
  cardName: { fontWeight: 600, fontSize: 13 },
  cardMeta: { color: "#777", fontSize: 11, marginTop: 2 },
  badge: { background: "#333", borderRadius: 3, padding: "1px 5px", fontSize: 10, color: "#888" },
  pinned: { color: "#f59e0b" },
  btn: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 10px", color: "#ccc", cursor: "pointer", fontSize: 12 },
  btnPrimary: { background: "#2563eb", border: "1px solid #3b82f6", borderRadius: 3, padding: "4px 10px", color: "#fff", cursor: "pointer", fontSize: 12 },
  btnDanger: { background: "transparent", border: "none", color: "#f88", cursor: "pointer", fontSize: 11, padding: "2px 4px" },
  section: { marginBottom: 16 },
  sectionTitle: { fontSize: 11, fontWeight: 700, color: "#888", textTransform: "uppercase" as const, marginBottom: 6, letterSpacing: "0.5px" },
  form: { background: "#252525", border: "1px solid #444", borderRadius: 4, padding: 12, marginBottom: 12 },
  formInput: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 8px", color: "#ccc", fontSize: 13, width: "100%", marginBottom: 6 },
  formRow: { display: "flex", gap: 6, marginTop: 8 },
  filterTag: { background: "#333", borderRadius: 3, padding: "1px 6px", fontSize: 11, color: "#9cdcfe" },
  empty: { color: "#666", textAlign: "center" as const, padding: 32 },
};

// ── Panel ───────────────────────────────────────────────────────────────────

export function SavedViewPanel() {
  const kernel = useKernel();

  // Registry (in-memory for now — will wire to FacetStore for persistence)
  const [registry] = useState(() => {
    const reg = createSavedViewRegistry();
    // Seed with example views
    const v1 = createSavedView("active-tasks", "task", {
      filters: [{ field: "status", op: "eq", value: "active" }],
      sorts: [{ field: "name", dir: "asc" }],
    }, "Active Tasks");
    v1.pinned = true;
    v1.mode = "list";
    reg.add(v1);

    const v2 = createSavedView("overdue-invoices", "invoice", {
      filters: [{ field: "status", op: "eq", value: "overdue" }],
      sorts: [{ field: "date", dir: "desc" }],
    }, "Overdue Invoices");
    v2.mode = "table";
    reg.add(v2);

    return reg;
  });

  const [search, setSearch] = useState("");
  const [activeViewId, setActiveViewId] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newType, setNewType] = useState("task");
  const [, setTick] = useState(0);

  // Subscribe to registry changes
  useState(() => {
    return registry.subscribe(() => setTick((t) => t + 1));
  });

  const allViews = useMemo(() => {
    if (search) return registry.search(search);
    return registry.all();
  }, [registry, search]);

  const pinnedViews = useMemo(() => registry.pinned(), [registry]);

  const handleCreate = useCallback(() => {
    if (!newName.trim()) return;
    const id = `view_${Date.now()}`;
    const view = createSavedView(id, newType, {}, newName.trim());
    registry.add(view);
    setShowCreate(false);
    setNewName("");
    kernel.notifications.add({ title: `View "${newName}" saved`, kind: "success" });
  }, [newName, newType, registry, kernel]);

  const handleDelete = useCallback((id: string) => {
    registry.remove(id);
    if (activeViewId === id) setActiveViewId(null);
  }, [registry, activeViewId]);

  const handlePin = useCallback((id: string) => {
    registry.pin(id);
  }, [registry]);

  const handleActivate = useCallback((view: SavedView) => {
    setActiveViewId(view.id);
    kernel.notifications.add({
      title: `Switched to "${view.name}"`,
      kind: "info",
    });
  }, [kernel]);

  const formatFilters = (config: ViewConfig): string => {
    const parts: string[] = [];
    if (config.filters?.length) parts.push(`${config.filters.length} filter${config.filters.length > 1 ? "s" : ""}`);
    if (config.sorts?.length) parts.push(`${config.sorts.length} sort${config.sorts.length > 1 ? "s" : ""}`);
    if (config.groups?.length) parts.push(`grouped by ${config.groups[0]?.field}`);
    return parts.join(", ") || "No filters";
  };

  return (
    <div style={s.root} data-testid="saved-view-panel">
      <div style={s.header}>
        <div style={s.title}>Saved Views</div>
        <button style={s.btnPrimary} onClick={() => setShowCreate(!showCreate)}>
          + New View
        </button>
      </div>

      {/* Create form */}
      {showCreate && (
        <div style={s.form}>
          <input
            style={s.formInput}
            placeholder="View name..."
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
            autoFocus
          />
          <select
            style={{ ...s.formInput, width: "auto" }}
            value={newType}
            onChange={(e) => setNewType(e.target.value)}
          >
            <option value="task">Task</option>
            <option value="contact">Contact</option>
            <option value="invoice">Invoice</option>
            <option value="project">Project</option>
            <option value="item">Item</option>
          </select>
          <div style={s.formRow}>
            <button style={s.btnPrimary} onClick={handleCreate}>Save View</button>
            <button style={s.btn} onClick={() => setShowCreate(false)}>Cancel</button>
          </div>
        </div>
      )}

      {/* Search */}
      <input
        style={s.search}
        placeholder="Search views..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />

      {/* Pinned section */}
      {pinnedViews.length > 0 && !search && (
        <div style={s.section}>
          <div style={s.sectionTitle}>{"\u2B50"} Pinned</div>
          {pinnedViews.map((view) => (
            <ViewCard
              key={view.id}
              view={view}
              isActive={view.id === activeViewId}
              onActivate={() => handleActivate(view)}
              onPin={() => handlePin(view.id)}
              onDelete={() => handleDelete(view.id)}
              formatFilters={formatFilters}
            />
          ))}
        </div>
      )}

      {/* All views */}
      <div style={s.section}>
        <div style={s.sectionTitle}>All Views ({allViews.length})</div>
        {allViews.length === 0 && (
          <div style={s.empty}>No saved views yet</div>
        )}
        {allViews.map((view) => (
          <ViewCard
            key={view.id}
            view={view}
            isActive={view.id === activeViewId}
            onActivate={() => handleActivate(view)}
            onPin={() => handlePin(view.id)}
            onDelete={() => handleDelete(view.id)}
            formatFilters={formatFilters}
          />
        ))}
      </div>
    </div>
  );
}

// ── View Card ───────────────────────────────────────────────────────────────

function ViewCard({
  view,
  isActive,
  onActivate,
  onPin,
  onDelete,
  formatFilters,
}: {
  view: SavedView;
  isActive: boolean;
  onActivate: () => void;
  onPin: () => void;
  onDelete: () => void;
  formatFilters: (config: ViewConfig) => string;
}) {
  return (
    <div
      style={{ ...s.card, ...(isActive ? s.cardActive : {}) }}
      onClick={onActivate}
    >
      <div style={s.cardHeader}>
        <div style={s.cardName}>
          <span
            style={{ cursor: "pointer", marginRight: 4, ...(view.pinned ? s.pinned : { color: "#555" }) }}
            onClick={(e) => { e.stopPropagation(); onPin(); }}
          >
            {view.pinned ? "\u2605" : "\u2606"}
          </span>
          {view.name}
        </div>
        <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
          <span style={s.badge}>{view.mode}</span>
          <span style={s.badge}>{view.objectType}</span>
          <button
            style={s.btnDanger}
            onClick={(e) => { e.stopPropagation(); onDelete(); }}
            title="Delete view"
          >
            {"\u2715"}
          </button>
        </div>
      </div>
      <div style={s.cardMeta}>
        {formatFilters(view.config)}
        {view.config.filters?.map((f, i) => (
          <span key={i} style={{ ...s.filterTag, marginLeft: 4 }}>
            {f.field} {f.op} {String(f.value ?? "")}
          </span>
        ))}
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const SAVED_VIEW_LENS_ID = lensId("saved-view");

export const savedViewLensManifest: LensManifest = {

  id: SAVED_VIEW_LENS_ID,
  name: "Saved Views",
  icon: "\u{1F516}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-saved-view", name: "Switch to Saved Views", shortcut: ["shift+v"], section: "Navigation" }],
  },
};

export const savedViewLensBundle: LensBundle = defineLensBundle(
  savedViewLensManifest,
  SavedViewPanel,
);
