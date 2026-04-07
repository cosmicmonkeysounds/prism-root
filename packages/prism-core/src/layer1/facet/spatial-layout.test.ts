import { describe, it, expect } from 'vitest';
import {
  computePartBands,
  snapToGrid,
  alignSlots,
  distributeSlots,
  detectOverlaps,
  slotHitTest,
  partForY,
  clampToBand,
  sortByZIndex,
} from './spatial-layout.js';
import type { SpatialRect } from './facet-schema.js';

// ── computePartBands ────────────────────────────────────────────────────

describe('computePartBands', () => {
  it('stacks parts vertically', () => {
    const bands = computePartBands([
      { kind: 'header', height: 60 },
      { kind: 'body', height: 300 },
      { kind: 'footer', height: 40 },
    ]);
    expect(bands).toHaveLength(3);
    expect(bands[0]).toMatchObject({ kind: 'header', y: 0, height: 60 });
    expect(bands[1]).toMatchObject({ kind: 'body', y: 60, height: 300 });
    expect(bands[2]).toMatchObject({ kind: 'footer', y: 360, height: 40 });
  });

  it('uses default height when omitted', () => {
    const bands = computePartBands([{ kind: 'body' }], 100);
    expect(bands[0]).toMatchObject({ y: 0, height: 100 });
  });

  it('invisible parts get zero height but appear in output', () => {
    const bands = computePartBands([
      { kind: 'header', height: 50, visible: false },
      { kind: 'body', height: 200 },
    ]);
    expect(bands[0]).toMatchObject({ kind: 'header', y: 0, height: 0, visible: false });
    expect(bands[1]).toMatchObject({ kind: 'body', y: 0, height: 200, visible: true });
  });

  it('returns empty array for empty input', () => {
    expect(computePartBands([])).toEqual([]);
  });

  it('preserves backgroundColor', () => {
    const bands = computePartBands([{ kind: 'header', backgroundColor: '#f00' }]);
    expect(bands[0]?.backgroundColor).toBe('#f00');
  });
});

// ── snapToGrid ──────────────────────────────────────────────────────────

describe('snapToGrid', () => {
  it('snaps to nearest grid intersection', () => {
    expect(snapToGrid(13, 27, 8)).toEqual({ x: 16, y: 24 });
  });

  it('already-aligned values stay unchanged', () => {
    expect(snapToGrid(16, 24, 8)).toEqual({ x: 16, y: 24 });
  });

  it('gridSize 0 returns original values', () => {
    expect(snapToGrid(13, 27, 0)).toEqual({ x: 13, y: 27 });
  });

  it('negative gridSize returns original values', () => {
    expect(snapToGrid(13, 27, -4)).toEqual({ x: 13, y: 27 });
  });

  it('snaps negative coordinates', () => {
    const result = snapToGrid(-5, -12, 10);
    expect(result.x).toBe(-0); // Math.round(-5/10)*10 = -0
    expect(result.y).toBe(-10);
  });
});

// ── alignSlots ──────────────────────────────────────────────────────────

describe('alignSlots', () => {
  const slots: SpatialRect[] = [
    { x: 10, y: 20, w: 100, h: 30 },
    { x: 50, y: 80, w: 80, h: 40 },
    { x: 30, y: 50, w: 120, h: 25 },
  ];

  it('aligns left', () => {
    const result = alignSlots(slots, 'left');
    expect(result.every((s) => s.x === 10)).toBe(true);
  });

  it('aligns right', () => {
    const result = alignSlots(slots, 'right');
    const maxRight = Math.max(...slots.map((s) => s.x + s.w)); // 150
    expect(result.every((s) => s.x + s.w === maxRight)).toBe(true);
  });

  it('aligns top', () => {
    const result = alignSlots(slots, 'top');
    expect(result.every((s) => s.y === 20)).toBe(true);
  });

  it('aligns bottom', () => {
    const result = alignSlots(slots, 'bottom');
    const maxBottom = Math.max(...slots.map((s) => s.y + s.h)); // 120
    expect(result.every((s) => s.y + s.h === maxBottom)).toBe(true);
  });

  it('aligns center-h', () => {
    const result = alignSlots(slots, 'center-h');
    const centers = result.map((s) => s.x + s.w / 2);
    expect(centers[0]).toBeCloseTo(centers[1] ?? 0, 5);
    expect(centers[0]).toBeCloseTo(centers[2] ?? 0, 5);
  });

  it('aligns center-v', () => {
    const result = alignSlots(slots, 'center-v');
    const centers = result.map((s) => s.y + s.h / 2);
    expect(centers[0]).toBeCloseTo(centers[1] ?? 0, 5);
    expect(centers[0]).toBeCloseTo(centers[2] ?? 0, 5);
  });

  it('preserves dimensions', () => {
    const result = alignSlots(slots, 'left');
    for (let i = 0; i < slots.length; i++) {
      expect(result[i]?.w).toBe(slots[i]?.w);
      expect(result[i]?.h).toBe(slots[i]?.h);
    }
  });

  it('returns empty for empty input', () => {
    expect(alignSlots([], 'left')).toEqual([]);
  });
});

// ── distributeSlots ─────────────────────────────────────────────────────

describe('distributeSlots', () => {
  it('distributes 3+ slots evenly on horizontal axis', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 20, h: 20 },
      { x: 10, y: 0, w: 20, h: 20 },
      { x: 100, y: 0, w: 20, h: 20 },
    ];
    const result = distributeSlots(slots, 'horizontal');
    // First and last stay; middle evenly spaced
    expect(result[0]?.x).toBe(0);
    expect(result[2]?.x).toBe(100);
    // totalSpan = 120, totalSlotSize = 60, gap = (120-60)/2 = 30
    // middle slot: 0 + 20 + 30 = 50
    expect(result[1]?.x).toBeCloseTo(50, 1);
  });

  it('returns copies unchanged for fewer than 3 slots', () => {
    const slots: SpatialRect[] = [
      { x: 10, y: 0, w: 20, h: 20 },
      { x: 80, y: 0, w: 20, h: 20 },
    ];
    const result = distributeSlots(slots, 'horizontal');
    expect(result[0]).toEqual(slots[0]);
    expect(result[1]).toEqual(slots[1]);
  });

  it('distributes vertically', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 20, h: 20 },
      { x: 0, y: 5, w: 20, h: 20 },
      { x: 0, y: 100, w: 20, h: 20 },
    ];
    const result = distributeSlots(slots, 'vertical');
    expect(result[0]?.y).toBe(0);
    expect(result[2]?.y).toBe(100);
  });
});

// ── detectOverlaps ──────────────────────────────────────────────────────

describe('detectOverlaps', () => {
  it('detects overlapping pair', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 50, h: 50 },
      { x: 30, y: 30, w: 50, h: 50 },
    ];
    expect(detectOverlaps(slots)).toEqual([[0, 1]]);
  });

  it('returns empty for non-overlapping slots', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 50, h: 50 },
      { x: 60, y: 0, w: 50, h: 50 },
    ];
    expect(detectOverlaps(slots)).toEqual([]);
  });

  it('adjacent (touching) slots do not overlap', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 50, h: 50 },
      { x: 50, y: 0, w: 50, h: 50 },
    ];
    expect(detectOverlaps(slots)).toEqual([]);
  });

  it('detects all overlapping pairs in a group', () => {
    const slots: SpatialRect[] = [
      { x: 0, y: 0, w: 100, h: 100 },
      { x: 50, y: 50, w: 100, h: 100 },
      { x: 90, y: 90, w: 100, h: 100 },
    ];
    // 0-1 overlap, 0-2 overlap, 1-2 overlap
    expect(detectOverlaps(slots)).toEqual([[0, 1], [0, 2], [1, 2]]);
  });

  it('handles empty input', () => {
    expect(detectOverlaps([])).toEqual([]);
  });
});

// ── slotHitTest ─────────────────────────────────────────────────────────

describe('slotHitTest', () => {
  const slots: SpatialRect[] = [
    { x: 0, y: 0, w: 50, h: 50 },
    { x: 30, y: 30, w: 50, h: 50 },
  ];

  it('returns index of slot under point', () => {
    expect(slotHitTest({ x: 10, y: 10 }, slots)).toBe(0);
  });

  it('returns null for miss', () => {
    expect(slotHitTest({ x: 200, y: 200 }, slots)).toBeNull();
  });

  it('returns topmost (highest zIndex) for overlapping slots', () => {
    expect(slotHitTest({ x: 40, y: 40 }, slots, [0, 10])).toBe(1);
    expect(slotHitTest({ x: 40, y: 40 }, slots, [10, 0])).toBe(0);
  });

  it('uses array index as tiebreaker when zIndices are equal', () => {
    expect(slotHitTest({ x: 40, y: 40 }, slots, [0, 0])).toBe(1);
  });

  it('point on edge is a hit', () => {
    expect(slotHitTest({ x: 0, y: 0 }, slots)).toBe(0);
    expect(slotHitTest({ x: 50, y: 50 }, slots)).toBe(1);
  });
});

// ── partForY ────────────────────────────────────────────────────────────

describe('partForY', () => {
  const bands = [
    { kind: 'header' as const, y: 0, height: 60, visible: true },
    { kind: 'body' as const, y: 60, height: 300, visible: true },
    { kind: 'footer' as const, y: 360, height: 40, visible: true },
  ];

  it('returns correct part for Y in header', () => {
    expect(partForY(30, bands)).toBe('header');
  });

  it('returns correct part for Y in body', () => {
    expect(partForY(200, bands)).toBe('body');
  });

  it('returns correct part for Y in footer', () => {
    expect(partForY(370, bands)).toBe('footer');
  });

  it('returns null for Y beyond all bands', () => {
    expect(partForY(500, bands)).toBeNull();
  });

  it('returns null for negative Y', () => {
    expect(partForY(-10, bands)).toBeNull();
  });

  it('skips invisible bands', () => {
    const withHidden = [
      { kind: 'header' as const, y: 0, height: 0, visible: false },
      { kind: 'body' as const, y: 0, height: 200, visible: true },
    ];
    expect(partForY(5, withHidden)).toBe('body');
  });
});

// ── clampToBand ─────────────────────────────────────────────────────────

describe('clampToBand', () => {
  it('clamps rect within band bounds', () => {
    const result = clampToBand({ x: -10, y: 50, w: 100, h: 30 }, { y: 60, height: 300 }, 612);
    expect(result.x).toBe(0);
    expect(result.y).toBe(60);
  });

  it('clamps right edge to canvas width', () => {
    const result = clampToBand({ x: 600, y: 100, w: 100, h: 30 }, { y: 60, height: 300 }, 612);
    expect(result.x).toBe(512);
  });

  it('clamps bottom edge to band bottom', () => {
    const result = clampToBand({ x: 0, y: 350, w: 100, h: 30 }, { y: 60, height: 300 }, 612);
    expect(result.y).toBe(330);
  });

  it('preserves dimensions', () => {
    const result = clampToBand({ x: 50, y: 100, w: 200, h: 40 }, { y: 60, height: 300 }, 612);
    expect(result.w).toBe(200);
    expect(result.h).toBe(40);
  });
});

// ── sortByZIndex ────────────────────────────────────────────────────────

describe('sortByZIndex', () => {
  it('sorts by ascending zIndex', () => {
    const slots = [
      { zIndex: 3, order: 0 },
      { zIndex: 1, order: 1 },
      { zIndex: 2, order: 2 },
    ];
    expect(sortByZIndex(slots)).toEqual([1, 2, 0]);
  });

  it('uses order as tiebreaker', () => {
    const slots = [
      { zIndex: 1, order: 2 },
      { zIndex: 1, order: 0 },
      { zIndex: 1, order: 1 },
    ];
    expect(sortByZIndex(slots)).toEqual([1, 2, 0]);
  });

  it('treats undefined zIndex as 0', () => {
    const slots = [
      { order: 0 },
      { zIndex: -1, order: 1 },
      { zIndex: 1, order: 2 },
    ];
    expect(sortByZIndex(slots)).toEqual([1, 0, 2]);
  });

  it('handles empty input', () => {
    expect(sortByZIndex([])).toEqual([]);
  });
});
