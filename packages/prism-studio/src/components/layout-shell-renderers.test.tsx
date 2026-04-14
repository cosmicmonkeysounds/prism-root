/**
 * @vitest-environment jsdom
 *
 * Regression tests for layout-shell-renderers:
 *
 * PageShellRenderer resize commit — pointerdown → pointermove → pointerup
 * must commit the new value via onCommit and clear `active` (the handle
 * must stop sticking to the cursor after release). This is the bug
 * described in ADR-005 Phase A: useResizeHandle previously had `value`
 * and `onCommit` in its dep array, which remounted the window listeners
 * on every pointermove and lost the pointerup event.
 *
 * The pure grid math is now owned by `@prism/core/puck` (`computeShellGrid`
 * in `shell-grid.tsx`) and is tested in `shell-grid.test.ts`.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";

// Required so React 18's createRoot doesn't complain that the test environment
// is not act-aware. Must be set before any render.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true;
import { PageShellRenderer } from "./layout-shell-renderers.js";

describe("PageShellRenderer resize commit (ADR-005 Phase A)", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  function pointerEvent(type: string, clientX: number, clientY: number): PointerEvent {
    // jsdom has no PointerEvent constructor, fall back to MouseEvent with
    // pointer-shape properties. The production hook only reads clientX/clientY
    // plus pointerId via setPointerCapture (which jsdom no-ops).
    const event = new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      clientX,
      clientY,
    });
    Object.defineProperty(event, "pointerId", { value: 1, configurable: true });
    return event as unknown as PointerEvent;
  }

  it("commits the new top-bar height on pointerup after a drag", () => {
    const commits: Array<{ key: string; value: number }> = [];
    act(() => {
      root.render(
        <PageShellRenderer
          topBarHeight={40}
          topBar={<div data-testid="top-content">top</div>}
          main={<div>main</div>}
          onCommit={(key, value) => commits.push({ key, value })}
        />,
      );
    });

    const handle = container.querySelector<HTMLElement>(
      "[data-testid='shell-resize-vertical']",
    );
    if (!handle) throw new Error("resize handle not rendered");

    // Start drag at y=40 (the initial bottom edge of the top bar).
    act(() => {
      handle.dispatchEvent(pointerEvent("pointerdown", 0, 40));
    });

    // The handle should now be active (coloured background).
    expect(handle.style.background).toBe("rgb(59, 130, 246)");

    // Drag 24px down — several pointermoves, each of which would previously
    // have torn down the listener effect.
    act(() => {
      window.dispatchEvent(pointerEvent("pointermove", 0, 48));
    });
    act(() => {
      window.dispatchEvent(pointerEvent("pointermove", 0, 56));
    });
    act(() => {
      window.dispatchEvent(pointerEvent("pointermove", 0, 64));
    });

    // Release — onCommit should fire with the final delta (40 + 24 = 64).
    act(() => {
      window.dispatchEvent(pointerEvent("pointerup", 0, 64));
    });

    expect(commits).toEqual([{ key: "topBarHeight", value: 64 }]);

    // And the handle should no longer be active — i.e. not sticking to cursor.
    const handleAfter = container.querySelector<HTMLElement>(
      "[data-testid='shell-resize-vertical']",
    );
    if (!handleAfter) throw new Error("resize handle missing after release");
    expect(handleAfter.style.background).toBe("transparent");

    // One more pointermove AFTER release must not change anything.
    act(() => {
      window.dispatchEvent(pointerEvent("pointermove", 0, 999));
    });
    expect(commits.length).toBe(1);
  });

  it("commits via pointercancel as well as pointerup", () => {
    const commits: Array<{ key: string; value: number }> = [];
    act(() => {
      root.render(
        <PageShellRenderer
          leftBarWidth={200}
          leftBar={<div>left</div>}
          main={<div>main</div>}
          onCommit={(key, value) => commits.push({ key, value })}
        />,
      );
    });

    const handle = container.querySelector<HTMLElement>(
      "[data-testid='shell-resize-horizontal']",
    );
    if (!handle) throw new Error("horizontal resize handle not rendered");

    act(() => {
      handle.dispatchEvent(pointerEvent("pointerdown", 200, 0));
    });
    act(() => {
      window.dispatchEvent(pointerEvent("pointermove", 250, 0));
    });
    act(() => {
      window.dispatchEvent(pointerEvent("pointercancel", 250, 0));
    });

    expect(commits).toEqual([{ key: "leftBarWidth", value: 250 }]);
  });
});
