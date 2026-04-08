/**
 * Spatial Canvas Panel — dedicated editing lens for free-form layouts.
 *
 * Activated when a spatial-canvas component is selected. Provides:
 * - Full-screen canvas with rulers and grid
 * - Left sidebar: field palette (draggable field chips from entity's EntityFieldDef)
 * - Right sidebar: slot property inspector (x, y, w, h, styling)
 * - Toolbar: alignment, distribute, zoom, snap toggle, add text/drawing
 *
 * All mutations update the FacetDefinition in the kernel registry.
 */

import { useState, useCallback, useMemo } from "react";
import { useKernel, useSelection, useFacetDefinitions } from "../kernel/index.js";
import { SpatialCanvasRenderer } from "../components/spatial-canvas-renderer.js";
import type { FacetDefinition, FacetSlot } from "@prism/core/facet";
import { facetDefinitionBuilder } from "@prism/core/facet";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    display: "flex",
    height: "100%",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
    fontSize: "0.8125rem",
  },
  sidebar: {
    width: 200,
    minWidth: 200,
    borderRight: "1px solid #333",
    display: "flex",
    flexDirection: "column" as const,
    overflow: "auto",
    padding: "0.5rem",
    gap: 4,
  },
  sidebarRight: {
    width: 220,
    minWidth: 220,
    borderLeft: "1px solid #333",
    display: "flex",
    flexDirection: "column" as const,
    overflow: "auto",
    padding: "0.5rem",
    gap: 4,
  },
  main: {
    flex: 1,
    display: "flex",
    flexDirection: "column" as const,
    overflow: "auto",
  },
  toolbar: {
    display: "flex",
    gap: 6,
    alignItems: "center",
    padding: "6px 12px",
    borderBottom: "1px solid #333",
    background: "#252526",
    flexWrap: "wrap" as const,
  },
  canvasArea: {
    flex: 1,
    overflow: "auto",
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "center",
    padding: 20,
    background: "#141414",
  },
  sectionTitle: {
    fontSize: 10,
    fontWeight: 600 as const,
    color: "#888",
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    marginBottom: 4,
    marginTop: 8,
  },
  fieldChip: {
    padding: "4px 8px",
    fontSize: 11,
    background: "#2a2d35",
    border: "1px solid #3a3d45",
    borderRadius: 3,
    color: "#ccc",
    cursor: "grab",
  },
  btn: {
    padding: "3px 8px",
    fontSize: 10,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnActive: {
    padding: "3px 8px",
    fontSize: 10,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    padding: "3px 6px",
    fontSize: 11,
    width: "100%",
  },
  label: {
    fontSize: 10,
    color: "#888",
    marginBottom: 2,
  },
  empty: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "100%",
    color: "#666",
    textAlign: "center" as const,
  },
} as const;

// ── Main Panel ──────────────────────────────────────────────────────────────

export function SpatialCanvasPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const { definitions, register, get: getFacet } = useFacetDefinitions();

  // Resolve the selected spatial-canvas object
  const selectedObj = useMemo(() => {
    if (!selectedId) return undefined;
    const obj = kernel.store.getObject(selectedId);
    if (!obj || obj.deletedAt || obj.type !== "spatial-canvas") return undefined;
    return obj;
  }, [kernel, selectedId]);

  // Get/create the FacetDefinition for this canvas
  const facetId = (selectedObj?.data as Record<string, unknown> | undefined)?.facetId as string | undefined;
  const definition = facetId ? getFacet(facetId) : undefined;

  // Grid state
  const [showGrid, setShowGrid] = useState(true);
  const [gridSize, setGridSize] = useState(8);
  const [selectedSlotIndex, setSelectedSlotIndex] = useState<number | null>(null);

  // Canvas dimensions from the object or defaults
  const canvasWidth = ((selectedObj?.data as Record<string, unknown> | undefined)?.canvasWidth as number) ?? 612;
  const canvasHeight = ((selectedObj?.data as Record<string, unknown> | undefined)?.canvasHeight as number) ?? 400;

  // Get entity fields for the field palette
  const entityFields = useMemo(() => {
    if (!definition) return [];
    const entityDef = kernel.registry.allDefs().find((d) => d.type === definition.objectType);
    if (!entityDef) return [];
    return entityDef.fields ?? [];
  }, [kernel, definition]);

  // Handle definition changes from the canvas renderer
  const handleDefinitionChange = useCallback(
    (updated: FacetDefinition) => {
      register(updated);
    },
    [register],
  );

  // Add a new field slot
  const addFieldSlot = useCallback(
    (fieldPath: string) => {
      if (!definition) return;
      const newSlot: FacetSlot = {
        kind: "field",
        slot: {
          fieldPath,
          part: "body",
          order: definition.slots.length,
          x: 10,
          y: 10 + definition.slots.length * 40,
          w: 200,
          h: 28,
        },
      };
      register({ ...definition, slots: [...definition.slots, newSlot] });
    },
    [definition, register],
  );

  // Add a text slot
  const addTextSlot = useCallback(() => {
    if (!definition) return;
    const newSlot: FacetSlot = {
      kind: "text",
      slot: {
        text: "Label",
        part: "body",
        order: definition.slots.length,
        x: 10,
        y: 10 + definition.slots.length * 40,
        w: 150,
        h: 24,
        fontSize: 12,
      },
    };
    register({ ...definition, slots: [...definition.slots, newSlot] });
  }, [definition, register]);

  // Add a drawing slot
  const addDrawingSlot = useCallback(
    (shape: "rectangle" | "line" | "ellipse" | "rounded-rectangle") => {
      if (!definition) return;
      const newSlot: FacetSlot = {
        kind: "drawing",
        slot: {
          shape,
          part: "body",
          order: definition.slots.length,
          x: 10,
          y: 10 + definition.slots.length * 40,
          w: 150,
          h: 80,
          strokeColor: "#666",
          strokeWidth: 1,
        },
      };
      register({ ...definition, slots: [...definition.slots, newSlot] });
    },
    [definition, register],
  );

  // Delete selected slot
  const deleteSelectedSlot = useCallback(() => {
    if (!definition || selectedSlotIndex === null) return;
    const newSlots = definition.slots.filter((_, i) => i !== selectedSlotIndex);
    register({ ...definition, slots: newSlots });
    setSelectedSlotIndex(null);
  }, [definition, selectedSlotIndex, register]);

  // Create a new facet definition for this canvas
  const createDefinition = useCallback(() => {
    if (!selectedObj) return;
    const id = `spatial-${Date.now()}`;
    const def = facetDefinitionBuilder(id, "page", "form")
      .name("Spatial Layout")
      .layoutMode("spatial")
      .canvasSize(canvasWidth, canvasHeight)
      .addPart({ kind: "body", height: canvasHeight })
      .build();
    register(def);
    // Update the spatial-canvas object with the new facetId
    kernel.updateObject(selectedObj.id, {
      data: { ...(selectedObj.data as Record<string, unknown>), facetId: id },
    });
  }, [selectedObj, canvasWidth, canvasHeight, register, kernel]);

  // ── No spatial-canvas selected ──────────────────────────────────────────

  if (!selectedObj) {
    return (
      <div style={styles.empty} data-testid="spatial-canvas-panel">
        <div>
          <div style={{ fontSize: "2em", marginBottom: 8, opacity: 0.5 }}>{"\uD83D\uDCD0"}</div>
          <div>Select a Spatial Canvas component to edit its layout</div>
        </div>
      </div>
    );
  }

  // ── No facet definition bound ───────────────────────────────────────────

  if (!definition) {
    return (
      <div style={styles.empty} data-testid="spatial-canvas-panel">
        <div>
          <div style={{ fontSize: "2em", marginBottom: 8, opacity: 0.5 }}>{"\uD83D\uDCD0"}</div>
          <div style={{ marginBottom: 12 }}>This canvas has no Facet Definition.</div>
          <button style={styles.btnActive} onClick={createDefinition}>
            Create Spatial Layout
          </button>
        </div>
      </div>
    );
  }

  // ── Selected slot properties ────────────────────────────────────────────

  const selectedSlot = selectedSlotIndex !== null ? definition.slots[selectedSlotIndex] : null;

  return (
    <div style={styles.container} data-testid="spatial-canvas-panel">
      {/* Left sidebar — Field Palette */}
      <div style={styles.sidebar}>
        <div style={styles.sectionTitle}>Fields</div>
        {entityFields.map((field) => (
          <div
            key={field.id}
            style={styles.fieldChip}
            onClick={() => addFieldSlot(field.id)}
            data-testid={`palette-field-${field.id}`}
          >
            {field.label ?? field.id}
          </div>
        ))}

        <div style={styles.sectionTitle}>Text &amp; Drawing</div>
        <div
          style={styles.fieldChip}
          onClick={addTextSlot}
          data-testid="palette-text"
        >
          + Text Label
        </div>
        <div
          style={styles.fieldChip}
          onClick={() => addDrawingSlot("rectangle")}
          data-testid="palette-rectangle"
        >
          + Rectangle
        </div>
        <div
          style={styles.fieldChip}
          onClick={() => addDrawingSlot("line")}
          data-testid="palette-line"
        >
          + Line
        </div>
        <div
          style={styles.fieldChip}
          onClick={() => addDrawingSlot("ellipse")}
          data-testid="palette-ellipse"
        >
          + Ellipse
        </div>

        <div style={styles.sectionTitle}>Facet Definitions</div>
        {definitions.map((d) => (
          <div
            key={d.id}
            style={{
              ...styles.fieldChip,
              borderColor: d.id === facetId ? "#0e639c" : "#3a3d45",
            }}
            title={d.id}
          >
            {d.name} ({d.layout})
          </div>
        ))}
      </div>

      {/* Main canvas area */}
      <div style={styles.main}>
        {/* Toolbar */}
        <div style={styles.toolbar}>
          <button
            style={showGrid ? styles.btnActive : styles.btn}
            onClick={() => setShowGrid((v) => !v)}
          >
            Grid
          </button>
          <label style={{ fontSize: 10, color: "#888" }}>
            Snap:
            <select
              value={gridSize}
              onChange={(e) => setGridSize(Number(e.target.value))}
              style={{ ...styles.input, width: 50, marginLeft: 4 }}
            >
              <option value={4}>4</option>
              <option value={8}>8</option>
              <option value={16}>16</option>
              <option value={32}>32</option>
            </select>
          </label>
          <span style={{ flex: 1 }} />
          {selectedSlot && (
            <button style={styles.btn} onClick={deleteSelectedSlot}>
              Delete Slot
            </button>
          )}
          <span style={{ color: "#555", fontSize: 10 }}>
            {definition.slots.length} slot{definition.slots.length !== 1 ? "s" : ""}
            {" | "}
            {canvasWidth} x {canvasHeight} pt
          </span>
        </div>

        {/* Canvas */}
        <div style={styles.canvasArea}>
          <SpatialCanvasRenderer
            definition={definition}
            editable
            gridSize={gridSize}
            showGrid={showGrid}
            canvasWidth={canvasWidth}
            canvasHeight={canvasHeight}
            onDefinitionChange={handleDefinitionChange}
          />
        </div>
      </div>

      {/* Right sidebar — Slot Inspector */}
      <div style={styles.sidebarRight}>
        <div style={styles.sectionTitle}>Slot Properties</div>
        {selectedSlot ? (
          <div>
            <div style={styles.label}>Kind: {selectedSlot.kind}</div>
            {selectedSlot.kind === "field" && (
              <div style={styles.label}>Field: {selectedSlot.slot.fieldPath}</div>
            )}
            {selectedSlot.kind === "text" && (
              <div style={styles.label}>Text: {selectedSlot.slot.text}</div>
            )}
            {selectedSlot.kind === "drawing" && (
              <div style={styles.label}>Shape: {selectedSlot.slot.shape}</div>
            )}
            <div style={{ marginTop: 8 }}>
              <div style={styles.label}>Position</div>
              <div style={{ display: "flex", gap: 4 }}>
                <div>
                  <div style={styles.label}>X</div>
                  <input style={styles.input} type="number" value={selectedSlot.slot.x ?? 0} readOnly />
                </div>
                <div>
                  <div style={styles.label}>Y</div>
                  <input style={styles.input} type="number" value={selectedSlot.slot.y ?? 0} readOnly />
                </div>
              </div>
              <div style={{ display: "flex", gap: 4, marginTop: 4 }}>
                <div>
                  <div style={styles.label}>W</div>
                  <input style={styles.input} type="number" value={selectedSlot.slot.w ?? 0} readOnly />
                </div>
                <div>
                  <div style={styles.label}>H</div>
                  <input style={styles.input} type="number" value={selectedSlot.slot.h ?? 0} readOnly />
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div style={{ color: "#555", fontSize: 11 }}>Select a slot to inspect</div>
        )}
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const SPATIAL_CANVAS_LENS_ID = lensId("spatial-canvas");

export const spatialCanvasLensManifest: LensManifest = {

  id: SPATIAL_CANVAS_LENS_ID,
  name: "Spatial Canvas",
  icon: "\uD83D\uDCD0",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-spatial-canvas", name: "Switch to Spatial Canvas Editor", shortcut: ["shift+x"], section: "Navigation" }],
  },
};

export const spatialCanvasLensBundle: LensBundle = defineLensBundle(
  spatialCanvasLensManifest,
  SpatialCanvasPanel,
);
