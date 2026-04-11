/**
 * Spatial Layout — pure functions for free-form absolute-positioned layouts.
 *
 * These operate on SpatialRect and LayoutPart data, producing new values
 * without side effects. Used by the LayoutCanvas renderer and editor.
 */

import type { LayoutPart, LayoutPartKind, SpatialRect } from './facet-schema.js';

// ── Part band computation ───────────────────────────────────────────────

export interface ComputedBand {
  kind: LayoutPartKind;
  y: number;
  height: number;
  visible: boolean;
  backgroundColor?: string;
}

/**
 * Stack layout parts vertically, computing the Y offset of each band.
 * Parts with `visible: false` get zero height but still appear in output.
 * If a part has an explicit `height`, use it; otherwise default to 80.
 */
export function computePartBands(
  parts: readonly LayoutPart[],
  defaultHeight = 80,
): ComputedBand[] {
  const bands: ComputedBand[] = [];
  let y = 0;
  for (const part of parts) {
    const visible = part.visible !== false;
    const height = visible ? (part.height ?? defaultHeight) : 0;
    const band: ComputedBand = {
      kind: part.kind,
      y,
      height,
      visible,
    };
    if (part.backgroundColor !== undefined) {
      band.backgroundColor = part.backgroundColor;
    }
    bands.push(band);
    y += height;
  }
  return bands;
}

// ── Snap to grid ────────────────────────────────────────────────────────

/**
 * Quantize (x, y) to the nearest grid intersection.
 */
export function snapToGrid(
  x: number,
  y: number,
  gridSize: number,
): { x: number; y: number } {
  if (gridSize <= 0) return { x, y };
  return {
    x: Math.round(x / gridSize) * gridSize,
    y: Math.round(y / gridSize) * gridSize,
  };
}

// ── Alignment ───────────────────────────────────────────────────────────

export type Alignment =
  | 'left'
  | 'right'
  | 'top'
  | 'bottom'
  | 'center-h'
  | 'center-v';

/**
 * Align a set of rectangles to a shared edge or center.
 * Returns new rects; originals are not mutated.
 */
export function alignSlots(
  slots: readonly SpatialRect[],
  alignment: Alignment,
): SpatialRect[] {
  if (slots.length === 0) return [];

  switch (alignment) {
    case 'left': {
      const minX = Math.min(...slots.map((s) => s.x));
      return slots.map((s) => ({ ...s, x: minX }));
    }
    case 'right': {
      const maxRight = Math.max(...slots.map((s) => s.x + s.w));
      return slots.map((s) => ({ ...s, x: maxRight - s.w }));
    }
    case 'top': {
      const minY = Math.min(...slots.map((s) => s.y));
      return slots.map((s) => ({ ...s, y: minY }));
    }
    case 'bottom': {
      const maxBottom = Math.max(...slots.map((s) => s.y + s.h));
      return slots.map((s) => ({ ...s, y: maxBottom - s.h }));
    }
    case 'center-h': {
      const minX = Math.min(...slots.map((s) => s.x));
      const maxRight = Math.max(...slots.map((s) => s.x + s.w));
      const centerX = (minX + maxRight) / 2;
      return slots.map((s) => ({ ...s, x: centerX - s.w / 2 }));
    }
    case 'center-v': {
      const minY = Math.min(...slots.map((s) => s.y));
      const maxBottom = Math.max(...slots.map((s) => s.y + s.h));
      const centerY = (minY + maxBottom) / 2;
      return slots.map((s) => ({ ...s, y: centerY - s.h / 2 }));
    }
  }
}

// ── Distribution ────────────────────────────────────────────────────────

/**
 * Distribute slots evenly along an axis.
 * Preserves the outermost slots' positions and spaces the rest equally.
 * Requires at least 3 slots to distribute; fewer are returned unchanged.
 */
export function distributeSlots(
  slots: readonly SpatialRect[],
  axis: 'horizontal' | 'vertical',
): SpatialRect[] {
  if (slots.length < 3) return slots.map((s) => ({ ...s }));

  const indexed = slots.map((s, i) => ({ s, i }));
  const key = axis === 'horizontal' ? 'x' : 'y';
  const size = axis === 'horizontal' ? 'w' : 'h';

  indexed.sort((a, b) => a.s[key] - b.s[key]);

  const first = indexed[0];
  const last = indexed[indexed.length - 1];
  if (!first || !last) return slots.map((s) => ({ ...s }));
  const totalSpan = last.s[key] + last.s[size] - first.s[key];
  const totalSlotSize = indexed.reduce((sum, { s }) => sum + s[size], 0);
  const gap = (totalSpan - totalSlotSize) / (indexed.length - 1);

  const result = slots.map((s) => ({ ...s }));
  let pos = first.s[key];
  for (const { s, i } of indexed) {
    result[i] = { ...s, [key]: pos };
    pos += s[size] + gap;
  }
  return result;
}

// ── Overlap detection ───────────────────────────────────────────────────

/**
 * Detect overlapping slot pairs. Returns index pairs [i, j] where i < j.
 */
export function detectOverlaps(
  slots: readonly SpatialRect[],
): Array<[number, number]> {
  const pairs: Array<[number, number]> = [];
  for (let i = 0; i < slots.length; i++) {
    for (let j = i + 1; j < slots.length; j++) {
      const a = slots[i];
      const b = slots[j];
      if (!a || !b) continue;
      if (
        a.x < b.x + b.w &&
        a.x + a.w > b.x &&
        a.y < b.y + b.h &&
        a.y + a.h > b.y
      ) {
        pairs.push([i, j]);
      }
    }
  }
  return pairs;
}

// ── Hit testing ─────────────────────────────────────────────────────────

/**
 * Find the topmost slot under the given point.
 * Returns the slot index, or null if no hit.
 * When multiple slots overlap at the point, returns the one with the
 * highest zIndex (or highest array index as tiebreaker).
 */
export function slotHitTest(
  point: { x: number; y: number },
  slots: readonly SpatialRect[],
  zIndices?: readonly (number | undefined)[],
): number | null {
  let bestIndex: number | null = null;
  let bestZ = -Infinity;

  for (let i = 0; i < slots.length; i++) {
    const s = slots[i];
    if (!s) continue;
    if (
      point.x >= s.x &&
      point.x <= s.x + s.w &&
      point.y >= s.y &&
      point.y <= s.y + s.h
    ) {
      const z = zIndices?.[i] ?? 0;
      if (z > bestZ || (z === bestZ && i > (bestIndex ?? -1))) {
        bestZ = z;
        bestIndex = i;
      }
    }
  }
  return bestIndex;
}

// ── Part lookup ─────────────────────────────────────────────────────────

/**
 * Determine which layout part band a Y coordinate falls within.
 */
export function partForY(
  y: number,
  bands: readonly ComputedBand[],
): LayoutPartKind | null {
  for (const band of bands) {
    if (!band.visible) continue;
    if (y >= band.y && y < band.y + band.height) {
      return band.kind;
    }
  }
  return null;
}

/**
 * Clamp a rect to stay within a band's bounds.
 */
export function clampToBand(
  rect: SpatialRect,
  band: { y: number; height: number },
  canvasWidth: number,
): SpatialRect {
  const x = Math.max(0, Math.min(rect.x, canvasWidth - rect.w));
  const y = Math.max(band.y, Math.min(rect.y, band.y + band.height - rect.h));
  return { x, y, w: rect.w, h: rect.h };
}

// ── Z-index sorting ─────────────────────────────────────────────────────

/**
 * Return slot indices sorted by ascending zIndex, then by order.
 */
export function sortByZIndex(
  slots: readonly { zIndex?: number; order: number }[],
): number[] {
  return slots
    .map((s, i) => i)
    .sort((a, b) => {
      const sA = slots[a];
      const sB = slots[b];
      if (!sA || !sB) return 0;
      const zA = sA.zIndex ?? 0;
      const zB = sB.zIndex ?? 0;
      if (zA !== zB) return zA - zB;
      return sA.order - sB.order;
    });
}
