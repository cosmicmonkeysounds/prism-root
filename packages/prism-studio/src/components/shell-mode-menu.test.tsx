/**
 * @vitest-environment jsdom
 *
 * ShellModeMenu — verifies the top-bar dropdown cycles the active
 * shell mode via `kernel.setShellMode` and re-renders when the kernel
 * notifies `onShellModeChange`. Uses a minimal hand-rolled kernel stub
 * (the real `createStudioKernel` pulls in half of core's runtime and
 * isn't worth booting for a UI unit test).
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { Permission, ShellMode } from "@prism/core/lens";
import { KernelProvider } from "../kernel/kernel-context.js";
import type { StudioKernel } from "../kernel/studio-kernel.js";
import { ShellModeMenu } from "./shell-mode-menu.js";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true;

interface FakeKernel {
  shellMode: ShellMode;
  permission: Permission;
  setShellMode(mode: ShellMode): void;
  onShellModeChange(listener: () => void): () => void;
}

function createFakeKernel(
  initial: ShellMode,
  permission: Permission = "dev",
): FakeKernel {
  let current = initial;
  const listeners = new Set<() => void>();
  return {
    get shellMode() {
      return current;
    },
    permission,
    setShellMode(mode) {
      if (mode === current) return;
      current = mode;
      for (const fn of listeners) fn();
    },
    onShellModeChange(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

describe("ShellModeMenu", () => {
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

  function renderWith(kernel: FakeKernel) {
    act(() => {
      root.render(
        <KernelProvider kernel={kernel as unknown as StudioKernel}>
          <ShellModeMenu />
        </KernelProvider>,
      );
    });
  }

  it("renders the current mode label and permission badge", () => {
    const kernel = createFakeKernel("build", "user");
    renderWith(kernel);
    const button = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-button"]',
    );
    expect(button).not.toBeNull();
    expect(button!.textContent).toContain("Build");
    expect(button!.textContent).toContain("user");
  });

  it("toggles the dropdown open on click and lists all three modes", () => {
    const kernel = createFakeKernel("admin");
    renderWith(kernel);
    const button = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-button"]',
    )!;
    expect(
      container.querySelector('[data-testid="shell-mode-menu"]'),
    ).toBeNull();
    act(() => button.click());
    expect(
      container.querySelector('[data-testid="shell-mode-menu"]'),
    ).not.toBeNull();
    const items = container.querySelectorAll(
      '[data-testid^="shell-mode-menu-item-"]',
    );
    expect(items).toHaveLength(3);
  });

  it("calls setShellMode and re-renders when a new mode is picked", () => {
    const kernel = createFakeKernel("admin");
    renderWith(kernel);
    const button = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-button"]',
    )!;
    act(() => button.click());
    const useItem = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-item-use"]',
    )!;
    act(() => useItem.click());
    expect(kernel.shellMode).toBe("use");
    // Menu closes and the button label now reflects the new mode.
    expect(
      container.querySelector('[data-testid="shell-mode-menu"]'),
    ).toBeNull();
    const refreshedButton = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-button"]',
    )!;
    expect(refreshedButton.textContent).toContain("Use");
  });

  it("marks the active mode as checked in the dropdown", () => {
    const kernel = createFakeKernel("build");
    renderWith(kernel);
    const button = container.querySelector<HTMLButtonElement>(
      '[data-testid="shell-mode-menu-button"]',
    )!;
    act(() => button.click());
    const active = container.querySelector(
      '[data-testid="shell-mode-menu-item-build"]',
    );
    expect(active?.getAttribute("aria-checked")).toBe("true");
    const other = container.querySelector(
      '[data-testid="shell-mode-menu-item-use"]',
    );
    expect(other?.getAttribute("aria-checked")).toBe("false");
  });
});
