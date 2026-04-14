/**
 * Tests for the lens auto-aggregator's pure helpers. We don't test
 * `import.meta.glob` itself — Vite/vitest own that — but we do pin the
 * scanning rules so renaming a panel export or dropping a bundle never
 * silently drops a lens from the kernel.
 */

import { describe, expect, it } from "vitest";
import type { LensBundle } from "./bundle.js";
import {
  buildLensBundleList,
  collectLensBundlesFromModule,
} from "./collect.js";

function fakeBundle(id: string): LensBundle {
  return {
    id,
    name: id,
    install: () => () => {},
  };
}

describe("collectLensBundlesFromModule", () => {
  it("picks up every *LensBundle export", () => {
    const mod = {
      editorLensBundle: fakeBundle("editor"),
      graphLensBundle: fakeBundle("graph"),
      EDITOR_LENS_ID: "editor",
      SomeOtherThing: {},
    };
    const bundles = collectLensBundlesFromModule(mod);
    expect(bundles.map((b) => b.id)).toEqual(["editor", "graph"]);
  });

  it("ignores non-LensBundle exports even if the key ends in LensBundle", () => {
    // A stray string / null / malformed object shouldn't crash the scan.
    const mod = {
      brokenLensBundle: { id: 42 }, // non-string id
      alsoBrokenLensBundle: null,
      stillBrokenLensBundle: { id: "x" /* missing install */ },
      okLensBundle: fakeBundle("ok"),
    };
    const bundles = collectLensBundlesFromModule(mod);
    expect(bundles.map((b) => b.id)).toEqual(["ok"]);
  });

  it("ignores keys that don't end in LensBundle", () => {
    const mod = {
      lensBundleForThings: fakeBundle("nope"),
      EditorLensBundleHelper: fakeBundle("nope2"),
      editorLensBundle: fakeBundle("yes"),
    };
    const bundles = collectLensBundlesFromModule(mod);
    expect(bundles.map((b) => b.id)).toEqual(["yes"]);
  });
});

describe("buildLensBundleList", () => {
  it("sorts by file path for deterministic ordering", () => {
    const modules = {
      "../panels/graph-panel.tsx": { graphLensBundle: fakeBundle("graph") },
      "../panels/editor-panel.tsx": { editorLensBundle: fakeBundle("editor") },
      "../panels/canvas-panel.tsx": { canvasLensBundle: fakeBundle("canvas") },
    };
    const bundles = buildLensBundleList(modules);
    expect(bundles.map((b) => b.id)).toEqual(["canvas", "editor", "graph"]);
  });

  it("deduplicates bundles that share an id across modules", () => {
    const shared = fakeBundle("shared");
    const modules = {
      "../panels/a-panel.tsx": { aLensBundle: shared },
      "../panels/b-panel.tsx": { bLensBundle: shared },
    };
    const bundles = buildLensBundleList(modules);
    expect(bundles).toHaveLength(1);
    expect(bundles[0]?.id).toBe("shared");
  });

  it("tolerates undefined / empty module entries", () => {
    const modules: Record<string, Record<string, unknown>> = {
      "../panels/editor-panel.tsx": { editorLensBundle: fakeBundle("editor") },
      "../panels/empty-panel.tsx": {},
    };
    const bundles = buildLensBundleList(modules);
    expect(bundles.map((b) => b.id)).toEqual(["editor"]);
  });

  it("returns a flat list across multiple panel modules", () => {
    const modules = {
      "../panels/editor-panel.tsx": {
        editorLensBundle: fakeBundle("editor"),
      },
      "../panels/graph-panel.tsx": {
        graphLensBundle: fakeBundle("graph"),
        // A panel may export more than one bundle if two lenses share a file.
        graphExtraLensBundle: fakeBundle("graph-extra"),
      },
    };
    const bundles = buildLensBundleList(modules);
    expect(bundles.map((b) => b.id)).toEqual([
      "editor",
      "graph",
      "graph-extra",
    ]);
  });

  it("attaches PANEL_MODES shell-mode constraints to known bundles", () => {
    // `canvas` is explicitly listed in panel-modes.ts as visible in every
    // mode at user permission — pin it so a rename/removal surfaces here.
    const modules = {
      "../panels/canvas-panel.tsx": { canvasLensBundle: fakeBundle("canvas") },
      "../panels/graph-panel.tsx": { graphLensBundle: fakeBundle("graph") },
      "../panels/unknown-panel.tsx": {
        unknownLensBundle: fakeBundle("not-in-table"),
      },
    };
    const bundles = buildLensBundleList(modules);
    const byId = Object.fromEntries(bundles.map((b) => [b.id, b]));

    expect(byId["canvas"]?.availableInModes).toEqual([
      "use",
      "build",
      "admin",
    ]);
    expect(byId["canvas"]?.minPermission).toBe("user");

    // `graph` is admin-only / dev in panel-modes.ts.
    expect(byId["graph"]?.availableInModes).toEqual(["admin"]);
    expect(byId["graph"]?.minPermission).toBe("dev");

    // Bundles absent from the table pass through unchanged so the
    // library defaults (["build","admin"], "user") apply.
    expect(byId["not-in-table"]?.availableInModes).toBeUndefined();
    expect(byId["not-in-table"]?.minPermission).toBeUndefined();
  });
});
