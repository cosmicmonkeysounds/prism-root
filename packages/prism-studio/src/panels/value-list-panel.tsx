/**
 * ValueListPanel — Value list editor for managing constrained field inputs.
 *
 * CRUD for static and dynamic value lists. Static lists are edited inline.
 * Dynamic lists configure collection source, value/display fields, and filters.
 *
 * Lens #26 (Shift+L)
 */

import { useState, useMemo, useCallback, type CSSProperties } from "react";
import {
  type ValueListItem,
  createStaticValueList,
  createDynamicValueList,
  createValueListRegistry,
} from "@prism/core/layer1";

// ── Styles ─��────────────────────────────────────────────────────────────────

const s: Record<string, CSSProperties> = {
  root: { padding: 12, fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a", height: "100%", display: "flex", gap: 12 },
  list: { width: 240, overflow: "auto", borderRight: "1px solid #333", paddingRight: 12 },
  detail: { flex: 1, overflow: "auto" },
  item: { padding: "6px 8px", borderRadius: 4, cursor: "pointer", marginBottom: 2 },
  itemActive: { background: "#2a2a3a", borderColor: "#3b82f6" },
  itemName: { fontWeight: 600, fontSize: 13 },
  itemMeta: { color: "#777", fontSize: 11 },
  btn: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 10px", color: "#ccc", cursor: "pointer", fontSize: 12 },
  btnPrimary: { background: "#2563eb", border: "1px solid #3b82f6", borderRadius: 3, padding: "4px 10px", color: "#fff", cursor: "pointer", fontSize: 12 },
  btnDanger: { background: "transparent", border: "none", color: "#f88", cursor: "pointer", fontSize: 11, padding: "2px 4px" },
  input: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 8px", color: "#ccc", fontSize: 13, width: "100%" },
  label: { color: "#888", fontSize: 11, fontWeight: 600, marginBottom: 2, display: "block", marginTop: 8 },
  section: { marginBottom: 16 },
  row: { display: "flex", gap: 6, alignItems: "center", marginBottom: 4 },
  valueRow: { display: "flex", gap: 6, alignItems: "center", padding: "4px 0", borderBottom: "1px solid #333" },
  empty: { color: "#666", textAlign: "center" as const, padding: 32 },
  badge: { background: "#333", borderRadius: 3, padding: "1px 5px", fontSize: 10, color: "#888" },
};

// ── Panel ─────────────────────────────────���─────────────────────────────────

export function ValueListPanel() {
  const [registry] = useState(() => {
    const reg = createValueListRegistry();
    // Seed examples
    reg.register(createStaticValueList("status-list", "Task Status", [
      { value: "active", label: "Active" },
      { value: "completed", label: "Completed" },
      { value: "archived", label: "Archived" },
    ]));
    reg.register(createStaticValueList("priority-list", "Priority", [
      { value: "low", label: "Low", color: "#4ade80" },
      { value: "medium", label: "Medium", color: "#fbbf24" },
      { value: "high", label: "High", color: "#f87171" },
    ]));
    reg.register(createDynamicValueList("client-list", "Clients", {
      collectionId: "contacts",
      valueField: "id",
      displayField: "name",
      sortField: "name",
      filter: { field: "type", op: "eq", value: "client" },
    }));
    return reg;
  });

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [, setTick] = useState(0);

  useState(() => registry.subscribe(() => setTick((t) => t + 1)));

  const lists = useMemo(() => registry.all(), [registry]);
  const selected = selectedId ? registry.get(selectedId) : undefined;

  const handleCreateStatic = useCallback(() => {
    const id = `vl_${Date.now()}`;
    registry.register(createStaticValueList(id, "New Value List", []));
    setSelectedId(id);
  }, [registry]);

  const handleCreateDynamic = useCallback(() => {
    const id = `vl_${Date.now()}`;
    registry.register(createDynamicValueList(id, "New Dynamic List", {
      collectionId: "",
      valueField: "id",
      displayField: "name",
    }));
    setSelectedId(id);
  }, [registry]);

  const handleDelete = useCallback(() => {
    if (selectedId) {
      registry.remove(selectedId);
      setSelectedId(null);
    }
  }, [selectedId, registry]);

  const handleUpdateName = useCallback((name: string) => {
    if (!selected) return;
    registry.register({ ...selected, name });
  }, [selected, registry]);

  const handleAddItem = useCallback(() => {
    if (!selected || selected.source.kind !== "static") return;
    const items = [...selected.source.items, { value: "", label: "" }];
    registry.register({ ...selected, source: { kind: "static", items } });
  }, [selected, registry]);

  const handleUpdateItem = useCallback((index: number, patch: Partial<ValueListItem>) => {
    if (!selected || selected.source.kind !== "static") return;
    const items = selected.source.items.map((item, i) =>
      i === index ? { ...item, ...patch } : item,
    );
    registry.register({ ...selected, source: { kind: "static", items } });
  }, [selected, registry]);

  const handleRemoveItem = useCallback((index: number) => {
    if (!selected || selected.source.kind !== "static") return;
    const items = selected.source.items.filter((_, i) => i !== index);
    registry.register({ ...selected, source: { kind: "static", items } });
  }, [selected, registry]);

  const handleUpdateDynamic = useCallback((patch: Record<string, string>) => {
    if (!selected || selected.source.kind !== "dynamic") return;
    registry.register({
      ...selected,
      source: { ...selected.source, ...patch },
    });
  }, [selected, registry]);

  return (
    <div style={s.root}>
      {/* List sidebar */}
      <div style={s.list}>
        <div style={{ fontWeight: 700, marginBottom: 8, color: "#aaa" }}>Value Lists</div>
        <div style={{ display: "flex", gap: 4, marginBottom: 8 }}>
          <button style={s.btn} onClick={handleCreateStatic}>+ Static</button>
          <button style={s.btn} onClick={handleCreateDynamic}>+ Dynamic</button>
        </div>
        {lists.map((list) => (
          <div
            key={list.id}
            style={{ ...s.item, ...(list.id === selectedId ? s.itemActive : {}) }}
            onClick={() => setSelectedId(list.id)}
          >
            <div style={s.itemName}>{list.name}</div>
            <div style={s.itemMeta}>
              <span style={s.badge}>{list.source.kind}</span>
              {list.source.kind === "static" && ` ${list.source.items.length} items`}
            </div>
          </div>
        ))}
        {lists.length === 0 && <div style={s.empty}>No value lists</div>}
      </div>

      {/* Detail editor */}
      <div style={s.detail}>
        {!selected && (
          <div style={s.empty}>Select a value list to edit</div>
        )}
        {selected && (
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <input
                style={{ ...s.input, fontWeight: 700, fontSize: 15, width: "auto", flex: 1 }}
                value={selected.name}
                onChange={(e) => handleUpdateName(e.target.value)}
              />
              <button style={{ ...s.btnDanger, marginLeft: 8 }} onClick={handleDelete}>Delete</button>
            </div>

            {/* Static list editor */}
            {selected.source.kind === "static" && (
              <div style={s.section}>
                <label style={s.label}>Values</label>
                {selected.source.items.map((item, i) => (
                  <div key={i} style={s.valueRow}>
                    <input
                      style={{ ...s.input, flex: 1 }}
                      value={String(item.value)}
                      onChange={(e) => handleUpdateItem(i, { value: e.target.value })}
                      placeholder="Value"
                    />
                    <input
                      style={{ ...s.input, flex: 1 }}
                      value={item.label ?? ""}
                      onChange={(e) => handleUpdateItem(i, { label: e.target.value })}
                      placeholder="Label"
                    />
                    {item.color && (
                      <input
                        type="color"
                        value={item.color}
                        onChange={(e) => handleUpdateItem(i, { color: e.target.value })}
                        style={{ width: 24, height: 24, border: "none", background: "none", cursor: "pointer" }}
                      />
                    )}
                    <button style={s.btnDanger} onClick={() => handleRemoveItem(i)}>{"\u2715"}</button>
                  </div>
                ))}
                <button style={{ ...s.btn, marginTop: 8 }} onClick={handleAddItem}>+ Add Value</button>
              </div>
            )}

            {/* Dynamic list editor */}
            {selected.source.kind === "dynamic" && (
              <div style={s.section}>
                <label style={s.label}>Collection ID</label>
                <input
                  style={s.input}
                  value={selected.source.collectionId}
                  onChange={(e) => handleUpdateDynamic({ collectionId: e.target.value })}
                  placeholder="e.g., contacts"
                />
                <label style={s.label}>Value Field</label>
                <input
                  style={s.input}
                  value={selected.source.valueField}
                  onChange={(e) => handleUpdateDynamic({ valueField: e.target.value })}
                  placeholder="e.g., id"
                />
                <label style={s.label}>Display Field</label>
                <input
                  style={s.input}
                  value={selected.source.displayField}
                  onChange={(e) => handleUpdateDynamic({ displayField: e.target.value })}
                  placeholder="e.g., name"
                />
                <label style={s.label}>Sort Field</label>
                <input
                  style={s.input}
                  value={selected.source.sortField ?? ""}
                  onChange={(e) => handleUpdateDynamic({ sortField: e.target.value })}
                  placeholder="e.g., name"
                />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
