/**
 * Shell-driven kernel tests.
 *
 * Covers the Puck-shell wiring added in the shell-builder refactor:
 * - `createStudioKernel` seeds a `PuckComponentRegistry` with `AppShell`
 *   and `LensOutlet`.
 * - `shellWidgetBundles` install into `shellWidgets` AND auto-register
 *   as Puck direct components.
 * - `lensBundles` with an `embeddable` puck config auto-register, too.
 * - `appProfile.lenses` filters the installed lens bundles.
 * - `shellTree` defaults to `DEFAULT_STUDIO_SHELL_TREE` and is mutable
 *   via `setShellTree`, notifying subscribers.
 */

import { describe, it, expect } from "vitest";
import type { ComponentType } from "react";
import {
  createStudioKernel,
  filterLensBundlesForProfile,
} from "./studio-kernel.js";
import {
  defineLensBundle,
  defineShellWidgetBundle,
  type LensBundle,
} from "../lenses/bundle.js";
import { lensId, type LensManifest } from "@prism/core/lens";
import { DEFAULT_STUDIO_SHELL_TREE } from "@prism/core/puck";
import type { AppProfile } from "@prism/core/builder";

function Dummy(label: string): ComponentType {
  const C = (() => null) as unknown as ComponentType;
  (C as { displayName?: string }).displayName = label;
  return C;
}

function manifest(id: string, name: string): LensManifest {
  return {
    id: lensId(id),
    name,
    icon: "",
    category: "custom",
    contributes: { views: [{ slot: "main" }], commands: [] },
  };
}

describe("filterLensBundlesForProfile", () => {
  const all: LensBundle[] = [
    defineLensBundle(manifest("editor", "Editor"), Dummy("Editor")),
    defineLensBundle(manifest("graph", "Graph"), Dummy("Graph")),
    defineLensBundle(manifest("layout", "Layout"), Dummy("Layout")),
  ];

  it("returns every bundle when no profile is supplied", () => {
    const out = filterLensBundlesForProfile(all, undefined);
    expect(out.map((b) => b.id)).toEqual(["editor", "graph", "layout"]);
  });

  it("returns every bundle when profile.lenses is omitted", () => {
    const profile: AppProfile = {
      id: "studio",
      name: "Studio",
      version: "1",
    } as AppProfile;
    expect(filterLensBundlesForProfile(all, profile).map((b) => b.id)).toEqual([
      "editor",
      "graph",
      "layout",
    ]);
  });

  it("returns every bundle when profile.lenses is empty", () => {
    const profile = {
      id: "studio",
      name: "Studio",
      version: "1",
      lenses: [],
    } as unknown as AppProfile;
    expect(filterLensBundlesForProfile(all, profile).map((b) => b.id)).toEqual([
      "editor",
      "graph",
      "layout",
    ]);
  });

  it("keeps only profile-listed ids in source order", () => {
    const profile = {
      id: "focus",
      name: "Focus",
      version: "1",
      lenses: ["graph", "editor"],
    } as unknown as AppProfile;
    expect(
      filterLensBundlesForProfile(all, profile).map((b) => b.id),
    ).toEqual(["editor", "graph"]);
  });
});

describe("createStudioKernel — shell wiring", () => {
  it("seeds the Puck component registry with Shell and LensOutlet", () => {
    const kernel = createStudioKernel();
    expect(kernel.puckComponents.hasDirect("Shell")).toBe(true);
    expect(kernel.puckComponents.hasDirect("LensOutlet")).toBe(true);
    kernel.dispose();
  });

  it("defaults shellTree to DEFAULT_STUDIO_SHELL_TREE", () => {
    const kernel = createStudioKernel();
    expect(kernel.shellTree).toBe(DEFAULT_STUDIO_SHELL_TREE);
    kernel.dispose();
  });

  it("auto-registers shell widget bundles as Puck direct components", () => {
    const kernel = createStudioKernel({
      shellWidgetBundles: [
        defineShellWidgetBundle({
          id: "my-bar",
          name: "My Bar",
          component: Dummy("MyBar"),
          puck: { label: "My Bar" },
        }),
      ],
    });
    expect(kernel.shellWidgets.get("my-bar")).toBeDefined();
    expect(kernel.puckComponents.hasDirect("MyBar")).toBe(true);
    kernel.dispose();
  });

  it("auto-registers embeddable lens bundles as Puck direct components", () => {
    const kernel = createStudioKernel({
      lensBundles: [
        defineLensBundle(manifest("tasks", "Tasks"), Dummy("Tasks"), {
          label: "Tasks",
          embeddable: true,
        }),
        defineLensBundle(manifest("chart", "Chart"), Dummy("Chart")),
      ],
    });
    expect(kernel.puckComponents.hasDirect("Tasks")).toBe(true);
    expect(kernel.puckComponents.hasDirect("Chart")).toBe(false);
    kernel.dispose();
  });

  it("filters lens bundles by appProfile.lenses", () => {
    const kernel = createStudioKernel({
      lensBundles: [
        defineLensBundle(manifest("editor", "Editor"), Dummy("Editor")),
        defineLensBundle(manifest("graph", "Graph"), Dummy("Graph")),
        defineLensBundle(manifest("canvas", "Canvas"), Dummy("Canvas")),
      ],
      appProfile: {
        id: "focus",
        name: "Focus",
        version: "1",
        lenses: ["editor"],
      } as unknown as AppProfile,
    });
    const lensIds = kernel.lensRegistry.allLenses().map((l) => l.id);
    expect(lensIds).toEqual(["editor"]);
    kernel.dispose();
  });

  it("notifies subscribers when shellTree changes", () => {
    const kernel = createStudioKernel();
    let fires = 0;
    const unsub = kernel.onShellTreeChange(() => {
      fires++;
    });

    const newTree = {
      root: { props: { headerHeight: 0 } },
      content: [],
    };
    kernel.setShellTree(newTree as never);
    expect(fires).toBe(1);
    expect(kernel.shellTree).toBe(newTree);

    unsub();
    kernel.setShellTree(DEFAULT_STUDIO_SHELL_TREE);
    expect(fires).toBe(1);
    kernel.dispose();
  });
});
