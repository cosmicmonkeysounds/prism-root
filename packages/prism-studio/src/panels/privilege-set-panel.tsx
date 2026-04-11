/**
 * PrivilegeSetPanel — Access control management for workspaces.
 *
 * UI for creating/editing privilege sets, assigning DIDs to roles,
 * and configuring collection/field/layout permissions.
 *
 * Lens #27 (Shift+P)
 */

import { useState, useMemo, useCallback, type CSSProperties } from "react";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  type PrivilegeSet,
  type CollectionPermission,
  type FieldPermission,
  type RoleAssignment,
  createPrivilegeSet,
} from "@prism/core/manifest";

// ── Styles ──────────────────────────────────────────────────────────────────

const s: Record<string, CSSProperties> = {
  root: { padding: 12, fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a", height: "100%", display: "flex", gap: 12 },
  sidebar: { width: 220, overflow: "auto", borderRight: "1px solid #333", paddingRight: 12 },
  main: { flex: 1, overflow: "auto" },
  item: { padding: "6px 8px", borderRadius: 4, cursor: "pointer", marginBottom: 2 },
  itemActive: { background: "#2a2a3a" },
  itemName: { fontWeight: 600, fontSize: 13 },
  itemMeta: { color: "#777", fontSize: 11 },
  btn: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 10px", color: "#ccc", cursor: "pointer", fontSize: 12 },
  btnPrimary: { background: "#2563eb", border: "1px solid #3b82f6", borderRadius: 3, padding: "4px 10px", color: "#fff", cursor: "pointer", fontSize: 12 },
  btnDanger: { background: "transparent", border: "none", color: "#f88", cursor: "pointer", fontSize: 11 },
  input: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 8px", color: "#ccc", fontSize: 13, width: "100%" },
  select: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "4px 8px", color: "#ccc", fontSize: 12 },
  label: { color: "#888", fontSize: 11, fontWeight: 600, marginBottom: 2, display: "block", marginTop: 12 },
  section: { marginBottom: 16 },
  sectionTitle: { fontSize: 12, fontWeight: 700, color: "#aaa", marginBottom: 6, borderBottom: "1px solid #333", paddingBottom: 4 },
  permRow: { display: "flex", alignItems: "center", gap: 8, padding: "4px 0", borderBottom: "1px solid #2a2a2a" },
  permLabel: { flex: 1, fontSize: 12 },
  badge: { background: "#333", borderRadius: 3, padding: "1px 5px", fontSize: 10, color: "#888" },
  empty: { color: "#666", textAlign: "center" as const, padding: 32 },
  roleRow: { display: "flex", gap: 6, alignItems: "center", padding: "6px 0", borderBottom: "1px solid #333" },
};

const COLLECTION_PERMISSIONS: CollectionPermission[] = ["full", "read", "create", "none"];
const FIELD_PERMISSIONS: FieldPermission[] = ["readwrite", "readonly", "hidden"];
// ── Panel ───────────────────────────────────────────────────────────────────

export function PrivilegeSetPanel() {
  const [sets, setSets] = useState<PrivilegeSet[]>(() => [
    createPrivilegeSet("admin", "Administrator", {
      collections: { "*": "full" },
      layouts: { "*": "visible" },
      scripts: { "*": "execute" },
      canManageAccess: true,
    }),
    createPrivilegeSet("editor", "Editor", {
      collections: { "*": "full" },
      layouts: { "*": "visible" },
      scripts: { "*": "execute" },
    }),
    createPrivilegeSet("viewer", "Viewer", {
      collections: { "*": "read" },
      layouts: { "*": "visible" },
      isDefault: true,
    }),
  ]);

  const [roles, setRoles] = useState<RoleAssignment[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>("admin");
  const [newCollId, setNewCollId] = useState("");
  const [newFieldKey, setNewFieldKey] = useState("");
  const [newRoleDid, setNewRoleDid] = useState("");

  const selected = useMemo(() => sets.find((s) => s.id === selectedId), [sets, selectedId]);

  const updateSet = useCallback((id: string, patch: Partial<PrivilegeSet>) => {
    setSets((prev) => prev.map((s) => (s.id === id ? { ...s, ...patch } : s)));
  }, []);

  const handleCreate = useCallback(() => {
    const id = `ps_${Date.now()}`;
    const ps = createPrivilegeSet(id, "New Privilege Set", { collections: { "*": "read" } });
    setSets((prev) => [...prev, ps]);
    setSelectedId(id);
  }, []);

  const handleDelete = useCallback(() => {
    if (!selectedId) return;
    setSets((prev) => prev.filter((s) => s.id !== selectedId));
    setSelectedId(null);
  }, [selectedId]);

  const setCollectionPerm = useCallback((collId: string, perm: CollectionPermission) => {
    if (!selected) return;
    updateSet(selected.id, {
      collections: { ...selected.collections, [collId]: perm },
    });
  }, [selected, updateSet]);

  const removeCollectionPerm = useCallback((collId: string) => {
    if (!selected) return;
    const colls = Object.fromEntries(
      Object.entries(selected.collections).filter(([k]) => k !== collId),
    );
    updateSet(selected.id, { collections: colls });
  }, [selected, updateSet]);

  const addCollectionPerm = useCallback(() => {
    if (!newCollId.trim() || !selected) return;
    setCollectionPerm(newCollId.trim(), "read");
    setNewCollId("");
  }, [newCollId, selected, setCollectionPerm]);

  const setFieldPerm = useCallback((key: string, perm: FieldPermission) => {
    if (!selected) return;
    updateSet(selected.id, {
      fields: { ...(selected.fields ?? {}), [key]: perm },
    });
  }, [selected, updateSet]);

  const addFieldPerm = useCallback(() => {
    if (!newFieldKey.trim() || !selected) return;
    setFieldPerm(newFieldKey.trim(), "readonly");
    setNewFieldKey("");
  }, [newFieldKey, selected, setFieldPerm]);

  const addRole = useCallback(() => {
    if (!newRoleDid.trim() || !selectedId) return;
    setRoles((prev) => [...prev, { did: newRoleDid.trim(), privilegeSetId: selectedId }]);
    setNewRoleDid("");
  }, [newRoleDid, selectedId]);

  const removeRole = useCallback((index: number) => {
    setRoles((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const rolesForSet = useMemo(
    () => roles.filter((r) => r.privilegeSetId === selectedId),
    [roles, selectedId],
  );

  return (
    <div style={s.root} data-testid="privilege-set-panel">
      {/* Sidebar */}
      <div style={s.sidebar}>
        <div style={{ fontWeight: 700, marginBottom: 8, color: "#aaa" }}>Privilege Sets</div>
        <button style={{ ...s.btnPrimary, width: "100%", marginBottom: 8 }} onClick={handleCreate}>
          + New Set
        </button>
        {sets.map((ps) => (
          <div
            key={ps.id}
            style={{ ...s.item, ...(ps.id === selectedId ? s.itemActive : {}) }}
            onClick={() => setSelectedId(ps.id)}
          >
            <div style={s.itemName}>{ps.name}</div>
            <div style={s.itemMeta}>
              {ps.isDefault && <span style={s.badge}>default</span>}
              {ps.canManageAccess && <span style={{ ...s.badge, marginLeft: 4 }}>admin</span>}
            </div>
          </div>
        ))}
      </div>

      {/* Main editor */}
      <div style={s.main}>
        {!selected && <div style={s.empty}>Select a privilege set</div>}
        {selected && (
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <input
                style={{ ...s.input, fontWeight: 700, fontSize: 15, flex: 1 }}
                value={selected.name}
                onChange={(e) => updateSet(selected.id, { name: e.target.value })}
              />
              <button style={{ ...s.btnDanger, marginLeft: 8 }} onClick={handleDelete}>Delete</button>
            </div>

            {/* Collection Permissions */}
            <div style={s.section}>
              <div style={s.sectionTitle}>Collection Permissions</div>
              {Object.entries(selected.collections).map(([collId, perm]) => (
                <div key={collId} style={s.permRow}>
                  <span style={s.permLabel}>{collId === "*" ? "Default (*)" : collId}</span>
                  <select
                    style={s.select}
                    value={perm}
                    onChange={(e) => setCollectionPerm(collId, e.target.value as CollectionPermission)}
                  >
                    {COLLECTION_PERMISSIONS.map((p) => (
                      <option key={p} value={p}>{p}</option>
                    ))}
                  </select>
                  {collId !== "*" && (
                    <button style={s.btnDanger} onClick={() => removeCollectionPerm(collId)}>{"\u2715"}</button>
                  )}
                </div>
              ))}
              <div style={{ ...s.permRow, borderBottom: "none" }}>
                <input
                  style={{ ...s.input, flex: 1 }}
                  value={newCollId}
                  onChange={(e) => setNewCollId(e.target.value)}
                  placeholder="Collection ID"
                  onKeyDown={(e) => { if (e.key === "Enter") addCollectionPerm(); }}
                />
                <button style={s.btn} onClick={addCollectionPerm}>Add</button>
              </div>
            </div>

            {/* Field Permissions */}
            <div style={s.section}>
              <div style={s.sectionTitle}>Field Overrides</div>
              {selected.fields && Object.entries(selected.fields).map(([key, perm]) => (
                <div key={key} style={s.permRow}>
                  <span style={s.permLabel}>{key}</span>
                  <select
                    style={s.select}
                    value={perm}
                    onChange={(e) => setFieldPerm(key, e.target.value as FieldPermission)}
                  >
                    {FIELD_PERMISSIONS.map((p) => (
                      <option key={p} value={p}>{p}</option>
                    ))}
                  </select>
                </div>
              ))}
              <div style={{ ...s.permRow, borderBottom: "none" }}>
                <input
                  style={{ ...s.input, flex: 1 }}
                  value={newFieldKey}
                  onChange={(e) => setNewFieldKey(e.target.value)}
                  placeholder="collection.field"
                  onKeyDown={(e) => { if (e.key === "Enter") addFieldPerm(); }}
                />
                <button style={s.btn} onClick={addFieldPerm}>Add</button>
              </div>
            </div>

            {/* Row-level security */}
            <div style={s.section}>
              <div style={s.sectionTitle}>Row-Level Security</div>
              <label style={s.label}>Record Filter Expression</label>
              <input
                style={{ ...s.input, fontFamily: "monospace" }}
                value={selected.recordFilter ?? ""}
                onChange={(e) => {
                  const val = e.target.value;
                  if (val) {
                    updateSet(selected.id, { recordFilter: val });
                  } else {
                    // Remove recordFilter — rebuild without it to avoid undefined
                    const cleaned = Object.fromEntries(
                      Object.entries(selected).filter(([k]) => k !== "recordFilter"),
                    ) as PrivilegeSet;
                    setSets((prev) => prev.map((s) => (s.id === selected.id ? cleaned : s)));
                  }
                }}
                placeholder='e.g., record.owner_did == current_did'
              />
            </div>

            {/* Role Assignments */}
            <div style={s.section}>
              <div style={s.sectionTitle}>Assigned Users</div>
              {rolesForSet.map((role, i) => (
                <div key={i} style={s.roleRow}>
                  <span style={{ flex: 1, fontFamily: "monospace", fontSize: 11 }}>{role.did}</span>
                  <button style={s.btnDanger} onClick={() => removeRole(roles.indexOf(role))}>{"\u2715"}</button>
                </div>
              ))}
              <div style={{ ...s.roleRow, borderBottom: "none" }}>
                <input
                  style={{ ...s.input, flex: 1, fontFamily: "monospace", fontSize: 11 }}
                  value={newRoleDid}
                  onChange={(e) => setNewRoleDid(e.target.value)}
                  placeholder="did:key:..."
                  onKeyDown={(e) => { if (e.key === "Enter") addRole(); }}
                />
                <button style={s.btn} onClick={addRole}>Assign</button>
              </div>
            </div>

            {/* Options */}
            <div style={s.section}>
              <div style={s.sectionTitle}>Options</div>
              <label style={{ display: "flex", gap: 8, alignItems: "center", padding: "4px 0" }}>
                <input
                  type="checkbox"
                  checked={selected.isDefault ?? false}
                  onChange={(e) => updateSet(selected.id, { isDefault: e.target.checked })}
                />
                <span>Default for new users</span>
              </label>
              <label style={{ display: "flex", gap: 8, alignItems: "center", padding: "4px 0" }}>
                <input
                  type="checkbox"
                  checked={selected.canManageAccess ?? false}
                  onChange={(e) => updateSet(selected.id, { canManageAccess: e.target.checked })}
                />
                <span>Can manage access control</span>
              </label>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const PRIVILEGE_SET_LENS_ID = lensId("privilege-set");

export const privilegeSetLensManifest: LensManifest = {

  id: PRIVILEGE_SET_LENS_ID,
  name: "Privilege Sets",
  icon: "\u{1F46E}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-privilege-set", name: "Switch to Privilege Set Manager", shortcut: ["shift+p"], section: "Navigation" }],
  },
};

export const privilegeSetLensBundle: LensBundle = defineLensBundle(
  privilegeSetLensManifest,
  PrivilegeSetPanel,
);
