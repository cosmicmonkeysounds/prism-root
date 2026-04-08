/**
 * SchemaDesignerPanel — Visual schema editor.
 *
 * Shows all registered EntityDefs as xyflow nodes and all EdgeTypeDefs as
 * xyflow edges. Unlike the form-based Entity Builder / Relationship Builder,
 * this is a canvas-first view: the graph IS the schema.
 *
 * Interactions
 * ────────────
 * - Click a node → select it (inspector below shows fields)
 * - Double-click empty canvas → prompt for type name → register new EntityDef
 * - Drag-to-connect (xyflow onConnect) → prompt for relation name → registerEdge
 * - Delete key on selected node → remove from registry
 * - Delete key on selected edge → removeEdge from registry
 *
 * Lens (Shift+D). Distinct from Graph panel (which shows *objects*, not types).
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  Controls,
  addEdge,
  applyNodeChanges,
  applyEdgeChanges,
  type Node,
  type Edge,
  type Connection,
  type NodeChange,
  type EdgeChange,
} from "@xyflow/react";
import type { EntityDef, EdgeTypeDef } from "@prism/core/object-model";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import { useKernel } from "../kernel/index.js";
import "@xyflow/react/dist/style.css";

// ── Style tokens ────────────────────────────────────────────────────────────

const s = {
  root: {
    display: "flex",
    flexDirection: "column" as const,
    height: "100%",
    background: "#1a1a1a",
    color: "#ccc",
    fontFamily: "system-ui",
    fontSize: 13,
  },
  toolbar: {
    display: "flex",
    gap: 6,
    padding: 8,
    borderBottom: "1px solid #333",
    alignItems: "center",
  },
  btn: {
    background: "#333",
    border: "1px solid #555",
    borderRadius: 3,
    padding: "4px 10px",
    color: "#ccc",
    cursor: "pointer",
    fontSize: 12,
  },
  btnDanger: {
    background: "#442",
    border: "1px solid #855",
    borderRadius: 3,
    padding: "4px 10px",
    color: "#f88",
    cursor: "pointer",
    fontSize: 12,
  },
  canvas: { flex: 1, position: "relative" as const, minHeight: 0 },
  inspector: {
    borderTop: "1px solid #333",
    padding: 10,
    maxHeight: 200,
    overflow: "auto",
    background: "#222",
    fontSize: 12,
  },
  inspectorTitle: {
    fontWeight: 700,
    color: "#eee",
    marginBottom: 6,
  },
  field: {
    display: "flex",
    gap: 8,
    padding: "2px 0",
    color: "#aaa",
  },
  fieldName: { color: "#9cdcfe", minWidth: 120 },
  fieldType: { color: "#ce9178" },
};

// ── Pure helpers (exported for tests) ───────────────────────────────────────

/** Auto-layout: lay out entity types in a grid. */
export function layoutEntities(
  types: EntityDef<string>[],
  positions: Map<string, { x: number; y: number }>,
  cellW = 220,
  cellH = 140,
  cols = 4,
): Map<string, { x: number; y: number }> {
  const out = new Map(positions);
  let i = 0;
  for (const def of types) {
    if (!out.has(def.type)) {
      out.set(def.type, {
        x: (i % cols) * cellW + 40,
        y: Math.floor(i / cols) * cellH + 40,
      });
    }
    i++;
  }
  return out;
}

/** Build xyflow nodes from EntityDefs + a positions map. */
export function buildSchemaNodes(
  defs: EntityDef<string>[],
  positions: Map<string, { x: number; y: number }>,
): Node[] {
  return defs.map((def) => {
    const pos = positions.get(def.type) ?? { x: 0, y: 0 };
    const fieldCount = def.fields?.length ?? 0;
    return {
      id: def.type,
      position: pos,
      data: {
        label: `${typeof def.icon === "string" ? def.icon + " " : ""}${def.label ?? def.type}`,
        sub: `${def.category} · ${fieldCount} field${fieldCount === 1 ? "" : "s"}`,
        color: def.color ?? "#888888",
      },
      type: "schemaEntity",
      style: {
        background: "#2a2a2a",
        border: `2px solid ${def.color ?? "#555"}`,
        borderRadius: 6,
        color: "#eee",
        padding: 10,
        width: 180,
        fontSize: 12,
      },
    };
  });
}

/**
 * Build xyflow edges from EdgeTypeDefs. Emits one visual edge per allowed
 * source/target pair declared in `sourceTypes`/`targetTypes`. Edge defs with
 * no explicit restrictions are skipped (they'd match every combination).
 */
export function buildSchemaEdges(edgeDefs: EdgeTypeDef[]): Edge[] {
  const out: Edge[] = [];
  for (const def of edgeDefs) {
    const srcs = def.sourceTypes ?? [];
    const tgts = def.targetTypes ?? [];
    if (srcs.length === 0 || tgts.length === 0) continue;
    for (const src of srcs) {
      for (const tgt of tgts) {
        out.push({
          id: `${def.relation}:${src}->${tgt}`,
          source: src,
          target: tgt,
          label: def.label ?? def.relation,
          data: { relation: def.relation },
          style: { stroke: def.color ?? "#888", strokeWidth: 2 },
          labelStyle: { fill: "#ccc", fontSize: 10 },
          labelBgStyle: { fill: "#1a1a1a" },
        });
      }
    }
  }
  return out;
}

// ── Custom Node ─────────────────────────────────────────────────────────────

function SchemaEntityNode({ data }: { data: { label: string; sub: string } }) {
  return (
    <div data-testid="schema-entity-node">
      <div style={{ fontWeight: 700, color: "#eee" }}>{data.label}</div>
      <div style={{ fontSize: 10, color: "#888", marginTop: 2 }}>{data.sub}</div>
    </div>
  );
}

const nodeTypes = { schemaEntity: SchemaEntityNode };

// ── Panel ───────────────────────────────────────────────────────────────────

function SchemaDesignerInner() {
  const kernel = useKernel();

  // Force re-render when registry changes. The registry is an in-memory map
  // and has no built-in subscription, so we bump a version counter after each
  // mutation. Entity/edge creation/deletion all go through this panel's
  // callbacks so we control the invalidation path.
  const [registryVersion, setRegistryVersion] = useState(0);
  const bumpRegistry = useCallback(() => setRegistryVersion((v) => v + 1), []);

  // Positions map persists across registry changes but resets with the panel.
  const [positions, setPositions] = useState<Map<string, { x: number; y: number }>>(
    () => new Map(),
  );

  // ── Registry snapshot (sensitive to registryVersion) ─────────────────────
  const entityDefs = useMemo(() => {
    // Touch registryVersion so React re-runs this memo after mutations.
    void registryVersion;
    return kernel.registry.allDefs() as EntityDef<string>[];
  }, [kernel.registry, registryVersion]);

  const edgeDefs = useMemo(() => {
    void registryVersion;
    return kernel.registry.allEdgeDefs();
  }, [kernel.registry, registryVersion]);

  // Ensure every entity has a laid-out position.
  useEffect(() => {
    setPositions((prev) => layoutEntities(entityDefs, prev));
  }, [entityDefs]);

  // ── xyflow state ─────────────────────────────────────────────────────────
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);

  // Re-derive nodes/edges when registry or positions change.
  useEffect(() => {
    setNodes(buildSchemaNodes(entityDefs, positions));
  }, [entityDefs, positions]);

  useEffect(() => {
    setEdges(buildSchemaEdges(edgeDefs));
  }, [edgeDefs]);

  const onNodesChange = useCallback((changes: NodeChange[]) => {
    setNodes((nds) => applyNodeChanges(changes, nds));
    // Persist position changes into the positions map.
    for (const change of changes) {
      if (change.type === "position" && change.position) {
        setPositions((prev) => {
          const next = new Map(prev);
          next.set(change.id, change.position as { x: number; y: number });
          return next;
        });
      }
    }
  }, []);

  const onEdgesChange = useCallback((changes: EdgeChange[]) => {
    setEdges((eds) => applyEdgeChanges(changes, eds));
  }, []);

  const onConnect = useCallback(
    (conn: Connection) => {
      if (!conn.source || !conn.target) return;
      const relation = window.prompt(
        `New relationship from "${conn.source}" → "${conn.target}"\nRelation name:`,
        `${conn.source}_to_${conn.target}`,
      );
      if (!relation) return;
      if (kernel.registry.getEdgeType(relation)) {
        kernel.notifications.add({
          title: `Relation "${relation}" already exists`,
          kind: "warning",
        });
        return;
      }
      const def: EdgeTypeDef = {
        relation,
        label: relation,
        sourceTypes: [conn.source],
        targetTypes: [conn.target],
      };
      kernel.registry.registerEdge(def);
      bumpRegistry();
      setEdges((eds) => addEdge({ ...conn, id: `${relation}:${conn.source}->${conn.target}`, label: relation }, eds));
      kernel.notifications.add({ title: `Relation "${relation}" added`, kind: "success" });
    },
    [kernel, bumpRegistry],
  );

  const addEntity = useCallback(() => {
    const type = window.prompt("New entity type name (kebab-case):", "");
    if (!type) return;
    if (kernel.registry.get(type)) {
      kernel.notifications.add({
        title: `Type "${type}" already exists`,
        kind: "warning",
      });
      return;
    }
    const def: EntityDef<string> = {
      type,
      category: "custom",
      label: type,
      pluralLabel: `${type}s`,
      icon: "\u{1F4E6}",
      color: "#a78bfa",
      fields: [],
    };
    kernel.registry.register(def);
    bumpRegistry();
    kernel.notifications.add({ title: `Entity "${type}" added`, kind: "success" });
  }, [kernel, bumpRegistry]);

  const deleteSelected = useCallback(() => {
    if (selectedNodeId) {
      const ok = kernel.registry.remove(selectedNodeId);
      if (ok) {
        kernel.notifications.add({
          title: `Entity "${selectedNodeId}" removed`,
          kind: "success",
        });
        setSelectedNodeId(null);
        bumpRegistry();
      }
      return;
    }
    if (selectedEdgeId) {
      // id is "relation:src->tgt"
      const relation = selectedEdgeId.split(":")[0];
      if (relation && kernel.registry.removeEdge(relation)) {
        kernel.notifications.add({
          title: `Relation "${relation}" removed`,
          kind: "success",
        });
        setSelectedEdgeId(null);
        bumpRegistry();
      }
    }
  }, [kernel, selectedNodeId, selectedEdgeId, bumpRegistry]);

  // Delete key handling.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Delete" || e.key === "Backspace") {
        if (selectedNodeId || selectedEdgeId) {
          deleteSelected();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedNodeId, selectedEdgeId, deleteSelected]);

  const selectedEntity = useMemo(
    () => (selectedNodeId ? entityDefs.find((d) => d.type === selectedNodeId) : null),
    [entityDefs, selectedNodeId],
  );

  return (
    <div style={s.root} data-testid="schema-designer-panel">
      <div style={s.toolbar}>
        <strong style={{ color: "#eee", marginRight: 12 }}>Schema Designer</strong>
        <button
          style={s.btn}
          onClick={addEntity}
          data-testid="schema-add-entity-btn"
          title="Add a new entity type"
        >
          + Entity
        </button>
        <button
          style={s.btnDanger}
          onClick={deleteSelected}
          disabled={!selectedNodeId && !selectedEdgeId}
          data-testid="schema-delete-btn"
          title="Remove selected entity or relationship"
        >
          ✕ Delete
        </button>
        <div style={{ flex: 1 }} />
        <span style={{ color: "#666", fontSize: 11 }}>
          {entityDefs.length} entity{entityDefs.length === 1 ? "" : " types"} ·{" "}
          {edgeDefs.length} relation{edgeDefs.length === 1 ? "" : "s"}
        </span>
      </div>

      <div style={s.canvas}>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onNodeClick={(_, n) => {
            setSelectedNodeId(n.id);
            setSelectedEdgeId(null);
          }}
          onEdgeClick={(_, e) => {
            setSelectedEdgeId(e.id);
            setSelectedNodeId(null);
          }}
          onPaneClick={() => {
            setSelectedNodeId(null);
            setSelectedEdgeId(null);
          }}
          fitView
          attributionPosition="bottom-left"
        >
          <Background color="#333" gap={20} />
          <Controls showInteractive={false} />
        </ReactFlow>
      </div>

      {selectedEntity && (
        <div style={s.inspector} data-testid="schema-inspector">
          <div style={s.inspectorTitle}>
            {typeof selectedEntity.icon === "string" ? selectedEntity.icon + " " : ""}
            {selectedEntity.label ?? selectedEntity.type}
            <span style={{ color: "#666", fontWeight: 400, marginLeft: 8, fontSize: 11 }}>
              ({selectedEntity.type})
            </span>
          </div>
          {(selectedEntity.fields ?? []).length === 0 ? (
            <div style={{ color: "#666" }}>No fields defined</div>
          ) : (
            (selectedEntity.fields ?? []).map((f) => (
              <div key={f.id} style={s.field}>
                <span style={s.fieldName}>{f.id}</span>
                <span style={s.fieldType}>{f.type}</span>
                {f.required ? <span style={{ color: "#f88" }}>*</span> : null}
                <span style={{ color: "#666" }}>{f.label ?? ""}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

export function SchemaDesignerPanel() {
  return (
    <ReactFlowProvider>
      <SchemaDesignerInner />
    </ReactFlowProvider>
  );
}

// ── Lens registration ──────────────────────────────────────────────────────

export const SCHEMA_DESIGNER_LENS_ID = lensId("schema-designer");

export const schemaDesignerLensManifest: LensManifest = {
  id: SCHEMA_DESIGNER_LENS_ID,
  name: "Schema Designer",
  icon: "\u{1F5FE}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-schema-designer", name: "Switch to Schema Designer", shortcut: ["shift+d"], section: "Navigation" },
    ],
  },
};

export const schemaDesignerLensBundle: LensBundle = defineLensBundle(
  schemaDesignerLensManifest,
  SchemaDesignerPanel,
);
