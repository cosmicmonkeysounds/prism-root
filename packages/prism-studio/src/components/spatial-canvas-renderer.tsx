/**
 * SpatialCanvasRenderer — free-form absolute-positioned layout canvas.
 *
 * Renders a FacetDefinition with layoutMode='spatial' as positioned slots
 * on a measured canvas. In edit mode, wraps slots with react-moveable for
 * drag/resize/snap. In preview mode, renders statically.
 */

import { useState, useRef, useCallback, useMemo } from "react";
import Moveable from "react-moveable";
import Selecto from "react-selecto";
import type {
  FacetDefinition,
  FacetSlot,
} from "@prism/core/facet";
import {
  computePartBands,
  snapToGrid,
  type ComputedBand,
} from "@prism/core/facet";

// ── Types ───────────────────────────────────────────────────────────────────

export interface SpatialCanvasProps {
  definition: FacetDefinition;
  editable?: boolean;
  gridSize?: number;
  showGrid?: boolean;
  canvasWidth?: number;
  canvasHeight?: number;
  onDefinitionChange?: (updated: FacetDefinition) => void;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const canvasStyles = {
  wrapper: {
    position: "relative" as const,
    overflow: "hidden",
    background: "#1a1a2e",
    border: "1px solid #333",
    borderRadius: 4,
  },
  band: {
    position: "absolute" as const,
    left: 0,
    width: "100%",
    borderBottom: "1px dashed #333",
  },
  bandLabel: {
    position: "absolute" as const,
    right: 4,
    top: 2,
    fontSize: 9,
    color: "#555",
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    pointerEvents: "none" as const,
  },
  gridLine: {
    position: "absolute" as const,
    background: "rgba(255,255,255,0.03)",
  },
  slot: {
    position: "absolute" as const,
    border: "1px solid #4a5568",
    borderRadius: 3,
    background: "rgba(255,255,255,0.05)",
    cursor: "move",
    overflow: "hidden",
    fontSize: 11,
    color: "#ccc",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    userSelect: "none" as const,
  },
  slotSelected: {
    borderColor: "#3b82f6",
    boxShadow: "0 0 0 1px #3b82f6",
  },
  slotField: {
    background: "rgba(59,130,246,0.08)",
  },
  slotPortal: {
    background: "rgba(139,92,246,0.08)",
    borderColor: "#8b5cf6",
  },
  slotText: {
    background: "rgba(245,158,11,0.08)",
    borderColor: "#f59e0b",
  },
  slotDrawing: {
    background: "rgba(16,185,129,0.08)",
    borderColor: "#10b981",
  },
  slotContainer: {
    background: "rgba(236,72,153,0.08)",
    borderColor: "#ec4899",
  },
} as const;

// ── Slot label helper ───────────────────────────────────────────────────────

function slotLabel(slot: FacetSlot): string {
  switch (slot.kind) {
    case "field":
      return slot.slot.label ?? slot.slot.fieldPath;
    case "portal":
      return `Portal: ${slot.slot.relationshipId}`;
    case "text":
      return slot.slot.text.length > 30
        ? slot.slot.text.slice(0, 30) + "\u2026"
        : slot.slot.text;
    case "drawing":
      return slot.slot.shape;
    case "container":
      return slot.slot.label ?? `File: ${slot.slot.fieldPath}`;
  }
}

function slotStyle(slot: FacetSlot): Record<string, string> {
  switch (slot.kind) {
    case "field":
      return canvasStyles.slotField;
    case "portal":
      return canvasStyles.slotPortal;
    case "text":
      return canvasStyles.slotText;
    case "drawing":
      return canvasStyles.slotDrawing;
    case "container":
      return canvasStyles.slotContainer;
  }
}

// ── Drawing renderer ────────────────────────────────────────────────────────

function DrawingShape({ slot }: { slot: FacetSlot & { kind: "drawing" } }) {
  const d = slot.slot;
  const w = d.w ?? 100;
  const h = d.h ?? 50;

  switch (d.shape) {
    case "line":
      return (
        <svg width="100%" height="100%" viewBox={`0 0 ${w} ${h}`}>
          <line
            x1="0" y1={h / 2} x2={w} y2={h / 2}
            stroke={d.strokeColor ?? "#888"}
            strokeWidth={d.strokeWidth ?? 1}
          />
        </svg>
      );
    case "rectangle":
      return (
        <svg width="100%" height="100%" viewBox={`0 0 ${w} ${h}`}>
          <rect
            x="0.5" y="0.5" width={w - 1} height={h - 1}
            fill={d.fillColor ?? "none"}
            stroke={d.strokeColor ?? "#888"}
            strokeWidth={d.strokeWidth ?? 1}
          />
        </svg>
      );
    case "ellipse":
      return (
        <svg width="100%" height="100%" viewBox={`0 0 ${w} ${h}`}>
          <ellipse
            cx={w / 2} cy={h / 2} rx={w / 2 - 1} ry={h / 2 - 1}
            fill={d.fillColor ?? "none"}
            stroke={d.strokeColor ?? "#888"}
            strokeWidth={d.strokeWidth ?? 1}
          />
        </svg>
      );
    case "rounded-rectangle":
      return (
        <svg width="100%" height="100%" viewBox={`0 0 ${w} ${h}`}>
          <rect
            x="0.5" y="0.5" width={w - 1} height={h - 1}
            rx={d.cornerRadius ?? 8} ry={d.cornerRadius ?? 8}
            fill={d.fillColor ?? "none"}
            stroke={d.strokeColor ?? "#888"}
            strokeWidth={d.strokeWidth ?? 1}
          />
        </svg>
      );
  }
}

// ── Main Component ──────────────────────────────────────────────────────────

export function SpatialCanvasRenderer({
  definition,
  editable = false,
  gridSize: gridSizeProp,
  showGrid: showGridProp,
  canvasWidth: widthProp,
  canvasHeight: heightProp,
  onDefinitionChange,
}: SpatialCanvasProps) {
  const canvasWidth = widthProp ?? definition.canvasWidth ?? 612;
  const canvasHeight = heightProp ?? definition.canvasHeight ?? 400;
  const gridSize = gridSizeProp ?? 8;
  const showGrid = showGridProp ?? true;

  const canvasRef = useRef<HTMLDivElement>(null);
  const moveableRef = useRef<Moveable>(null);
  const [selectedIndices, setSelectedIndices] = useState<number[]>([]);

  // Compute part bands
  const bands = useMemo(
    () => computePartBands(definition.parts),
    [definition.parts],
  );

  // Build slot elements
  const slots = definition.slots;

  // Default position for slots without spatial data
  const getSlotRect = useCallback(
    (slot: FacetSlot, index: number) => ({
      x: slot.slot.x ?? 10,
      y: slot.slot.y ?? 10 + index * 40,
      w: slot.slot.w ?? 200,
      h: slot.slot.h ?? 30,
    }),
    [],
  );

  // Handle moveable drag end
  const handleDragEnd = useCallback(
    (index: number, x: number, y: number) => {
      if (!onDefinitionChange) return;
      const snapped = snapToGrid(x, y, gridSize);
      const newSlots = [...definition.slots];
      const slot = newSlots[index];
      if (!slot) return;
      newSlots[index] = {
        ...slot,
        slot: { ...slot.slot, x: snapped.x, y: snapped.y },
      } as FacetSlot;
      onDefinitionChange({ ...definition, slots: newSlots });
    },
    [definition, gridSize, onDefinitionChange],
  );

  // Handle moveable resize end
  const handleResizeEnd = useCallback(
    (index: number, w: number, h: number, x: number, y: number) => {
      if (!onDefinitionChange) return;
      const snapped = snapToGrid(x, y, gridSize);
      const newSlots = [...definition.slots];
      const slot = newSlots[index];
      if (!slot) return;
      newSlots[index] = {
        ...slot,
        slot: { ...slot.slot, x: snapped.x, y: snapped.y, w, h },
      } as FacetSlot;
      onDefinitionChange({ ...definition, slots: newSlots });
    },
    [definition, gridSize, onDefinitionChange],
  );

  // Resolve selected targets for Moveable
  const selectedTargets = useMemo(() => {
    if (!editable || !canvasRef.current) return [];
    return selectedIndices
      .map((i) => canvasRef.current?.querySelector(`[data-slot-index="${i}"]`) as HTMLElement | null)
      .filter(Boolean) as HTMLElement[];
  }, [selectedIndices, editable]);

  // Render grid lines
  const gridLines = useMemo(() => {
    if (!showGrid) return null;
    const lines: React.JSX.Element[] = [];
    for (let x = gridSize; x < canvasWidth; x += gridSize) {
      lines.push(
        <div
          key={`gv-${x}`}
          style={{
            ...canvasStyles.gridLine,
            left: x,
            top: 0,
            width: 1,
            height: canvasHeight,
          }}
        />,
      );
    }
    for (let y = gridSize; y < canvasHeight; y += gridSize) {
      lines.push(
        <div
          key={`gh-${y}`}
          style={{
            ...canvasStyles.gridLine,
            left: 0,
            top: y,
            width: canvasWidth,
            height: 1,
          }}
        />,
      );
    }
    return lines;
  }, [showGrid, gridSize, canvasWidth, canvasHeight]);

  return (
    <div
      data-testid="spatial-canvas"
      style={{ position: "relative" }}
    >
      {/* Canvas surface */}
      <div
        ref={canvasRef}
        className="spatial-canvas-surface"
        style={{
          ...canvasStyles.wrapper,
          width: canvasWidth,
          height: canvasHeight,
        }}
      >
        {/* Grid */}
        {gridLines}

        {/* Part bands */}
        {bands.map((band: ComputedBand) =>
          band.visible ? (
            <div
              key={band.kind}
              data-testid={`band-${band.kind}`}
              style={{
                ...canvasStyles.band,
                top: band.y,
                height: band.height,
                backgroundColor: band.backgroundColor ?? "transparent",
              }}
            >
              <span style={canvasStyles.bandLabel}>{band.kind}</span>
            </div>
          ) : null,
        )}

        {/* Slots */}
        {slots.map((slot, i) => {
          const rect = getSlotRect(slot, i);
          const isSelected = selectedIndices.includes(i);
          return (
            <div
              key={i}
              data-slot-index={i}
              data-testid={`slot-${i}`}
              onClick={(e) => {
                if (!editable) return;
                e.stopPropagation();
                if (e.shiftKey) {
                  setSelectedIndices((prev) =>
                    prev.includes(i) ? prev.filter((idx) => idx !== i) : [...prev, i],
                  );
                } else {
                  setSelectedIndices([i]);
                }
              }}
              style={{
                ...canvasStyles.slot,
                ...slotStyle(slot),
                ...(isSelected ? canvasStyles.slotSelected : {}),
                left: rect.x,
                top: rect.y,
                width: rect.w,
                height: rect.h,
                zIndex: slot.slot.zIndex ?? 0,
              }}
            >
              {slot.kind === "drawing" ? (
                <DrawingShape slot={slot as FacetSlot & { kind: "drawing" }} />
              ) : (
                <span style={{ padding: "0 6px", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {slotLabel(slot)}
                </span>
              )}
            </div>
          );
        })}
      </div>

      {/* Moveable (edit mode only) */}
      {editable && selectedTargets.length > 0 && (
        <Moveable
          ref={moveableRef}
          target={selectedTargets}
          draggable
          resizable
          snappable
          snapGridWidth={gridSize}
          snapGridHeight={gridSize}
          onDrag={({ target, left, top }) => {
            target.style.left = `${left}px`;
            target.style.top = `${top}px`;
          }}
          onDragEnd={({ target }) => {
            const idx = Number(target.getAttribute("data-slot-index"));
            handleDragEnd(idx, parseFloat(target.style.left), parseFloat(target.style.top));
          }}
          onResize={({ target, width, height, drag }) => {
            target.style.width = `${width}px`;
            target.style.height = `${height}px`;
            target.style.left = `${drag.left}px`;
            target.style.top = `${drag.top}px`;
          }}
          onResizeEnd={({ target }) => {
            const idx = Number(target.getAttribute("data-slot-index"));
            handleResizeEnd(
              idx,
              parseFloat(target.style.width),
              parseFloat(target.style.height),
              parseFloat(target.style.left),
              parseFloat(target.style.top),
            );
          }}
        />
      )}

      {/* Selecto (edit mode only) */}
      {editable && (
        <Selecto
          container={canvasRef.current}
          selectableTargets={["[data-slot-index]"]}
          hitRate={0}
          selectByClick
          selectFromInside={false}
          onSelect={({ selected }) => {
            setSelectedIndices(
              selected
                .map((el) => Number(el.getAttribute("data-slot-index")))
                .filter((n) => !Number.isNaN(n)),
            );
          }}
        />
      )}
    </div>
  );
}
