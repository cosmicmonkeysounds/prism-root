/**
 * Admin Panel integration tests.
 *
 * We can't render the Puck editor in node vitest (no DOM), so these tests
 * exercise the integration surface:
 *   - The lens bundle manifest is well-formed
 *   - A kernel-backed admin data source projects a real StudioKernel into
 *     a valid AdminSnapshot whose numbers match the seeded objects
 *   - Bus events from the kernel propagate into the admin data source's
 *     activity feed when subscribed
 */

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createKernelDataSource } from "@prism/admin-kit";
import { createStudioKernel, type StudioKernel } from "../kernel/index.js";
import { adminLensBundle, adminLensManifest, ADMIN_LENS_ID } from "./admin-panel.js";

describe("adminLensBundle", () => {
  it("has a stable lens id and manifest", () => {
    expect(adminLensManifest.id).toBe(ADMIN_LENS_ID);
    expect(adminLensManifest.name).toBe("Admin");
    expect(adminLensManifest.category).toBe("custom");
    expect(adminLensManifest.contributes?.views).toEqual([{ slot: "main" }]);
    expect(adminLensManifest.contributes?.commands?.[0]?.shortcut).toEqual(["Shift+A"]);
  });

  it("exports a lens bundle whose install wires the manifest and component", () => {
    expect(adminLensBundle.id).toBe(ADMIN_LENS_ID);
    expect(adminLensBundle.name).toBe("Admin");
    expect(typeof adminLensBundle.install).toBe("function");
  });
});

describe("admin panel + kernel data source", () => {
  let kernel: StudioKernel;

  beforeEach(() => {
    kernel = createStudioKernel();
  });

  afterEach(() => {
    kernel.dispose();
  });

  it("projects a seeded kernel into a realistic snapshot", async () => {
    kernel.createObject({
      type: "page",
      name: "Home",
      parentId: null,
      position: 0,
      data: { title: "Home", slug: "/", published: false },
    });
    kernel.createObject({
      type: "page",
      name: "About",
      parentId: null,
      position: 1,
      data: { title: "About", slug: "/about", published: false },
    });

    const source = createKernelDataSource(kernel, { id: "test", label: "Test" });
    const snap = await source.snapshot();

    expect(snap.sourceId).toBe("test");
    expect(snap.sourceLabel).toBe("Test");
    const byId = Object.fromEntries(snap.metrics.map((m) => [m.id, m.value]));
    expect(typeof byId["objects"]).toBe("number");
    expect(byId["objects"]).toBeGreaterThanOrEqual(2);
    expect(snap.services.some((s) => s.id === "object-store")).toBe(true);
    expect(snap.services.some((s) => s.id === "relay")).toBe(true);
    expect(snap.services.some((s) => s.id === "presence")).toBe(true);
  });

  it("streams snapshots to subscribers when the kernel mutates", async () => {
    const source = createKernelDataSource(kernel, { id: "stream", label: "Stream" });
    const seen: number[] = [];
    if (!source.subscribe) throw new Error("kernel source should expose subscribe()");
    const unsub = source.subscribe((snap) => {
      const objectsMetric = snap.metrics.find((m) => m.id === "objects");
      if (typeof objectsMetric?.value === "number") {
        seen.push(objectsMetric.value);
      }
    });

    kernel.createObject({
      type: "page",
      name: "New",
      parentId: null,
      position: 0,
      data: {},
    });

    // Subscription fires immediately with current state, then again on
    // object.created — we should have seen the count grow.
    expect(seen.length).toBeGreaterThanOrEqual(2);
    const last = seen[seen.length - 1] ?? 0;
    const first = seen[0] ?? 0;
    expect(last).toBeGreaterThanOrEqual(first + 1);

    unsub();
    source.dispose?.();
  });

  it("installs into a lens registry and a component map", async () => {
    const { createLensRegistry } = await import("@prism/core/lens");
    const registry = createLensRegistry();
    const componentMap = new Map();
    const uninstall = adminLensBundle.install({
      lensRegistry: registry,
      componentMap,
    });
    expect(registry.get(ADMIN_LENS_ID)).toBe(adminLensManifest);
    expect(componentMap.get(ADMIN_LENS_ID)).toBeDefined();
    uninstall();
    expect(registry.has(ADMIN_LENS_ID)).toBe(false);
  });
});
