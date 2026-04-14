/**
 * Tests for the pure `computeShellGrid` helper + `clampBar` utility.
 * Rendering behaviour is covered elsewhere — this file asserts only the
 * grid template strings emitted for the 3×4 six-region layout.
 */

import { describe, expect, it } from "vitest";
import { clampBar, computeShellGrid } from "./shell-grid.js";

describe("clampBar", () => {
  it("returns 0 for undefined / non-finite input", () => {
    expect(clampBar(undefined)).toBe(0);
    expect(clampBar(Number.NaN)).toBe(0);
    expect(clampBar(Number.POSITIVE_INFINITY)).toBe(0);
  });

  it("rounds finite numeric input", () => {
    expect(clampBar(10.4)).toBe(10);
    expect(clampBar(10.6)).toBe(11);
  });

  it("clamps to the 0..4000 range", () => {
    expect(clampBar(-10)).toBe(0);
    expect(clampBar(9999)).toBe(4000);
  });
});

describe("computeShellGrid", () => {
  it("uses the provided bar dimensions when all six regions are present", () => {
    const grid = computeShellGrid({
      activityBarWidth: 48,
      topBarHeight: 36,
      leftBarWidth: 260,
      rightBarWidth: 280,
      bottomBarHeight: 24,
      hasActivityBar: true,
      hasTopBar: true,
      hasLeftBar: true,
      hasRightBar: true,
      hasBottomBar: true,
    });
    expect(grid.gridTemplateColumns).toBe("48px 260px 1fr 280px");
    expect(grid.gridTemplateRows).toBe("36px 1fr 24px");
    expect(grid.gridTemplateAreas).toContain('"activity top top top"');
    expect(grid.gridTemplateAreas).toContain('"activity left main right"');
    expect(grid.gridTemplateAreas).toContain('"activity bottom bottom bottom"');
  });

  it("collapses hidden bars to 0px without touching present ones", () => {
    const grid = computeShellGrid({
      activityBarWidth: 48,
      topBarHeight: 36,
      leftBarWidth: 260,
      rightBarWidth: 280,
      bottomBarHeight: 24,
      hasActivityBar: false,
      hasTopBar: false,
      hasLeftBar: true,
      hasRightBar: false,
      hasBottomBar: false,
    });
    expect(grid.gridTemplateColumns).toBe("0px 260px 1fr 0px");
    expect(grid.gridTemplateRows).toBe("0px 1fr 0px");
  });

  it("defaults undefined sizes to 0px for present bars", () => {
    const grid = computeShellGrid({
      hasActivityBar: true,
      hasTopBar: true,
      hasLeftBar: true,
      hasRightBar: true,
      hasBottomBar: true,
    });
    expect(grid.gridTemplateColumns).toBe("0px 0px 1fr 0px");
    expect(grid.gridTemplateRows).toBe("0px 1fr 0px");
  });

  it("clamps oversize bar widths", () => {
    const grid = computeShellGrid({
      activityBarWidth: 99999,
      topBarHeight: -5,
      leftBarWidth: 10.5,
      rightBarWidth: 200,
      bottomBarHeight: 0,
      hasActivityBar: true,
      hasTopBar: true,
      hasLeftBar: true,
      hasRightBar: true,
      hasBottomBar: true,
    });
    expect(grid.gridTemplateColumns).toBe("4000px 11px 1fr 200px");
    expect(grid.gridTemplateRows).toBe("0px 1fr 0px");
  });
});
