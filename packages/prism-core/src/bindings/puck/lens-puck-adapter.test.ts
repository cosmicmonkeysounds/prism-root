import { describe, expect, it } from "vitest";
import type { ComponentType } from "react";
import type { LensBundle, ShellWidgetBundle } from "@prism/core/lens";
import { lensId, type LensManifest } from "@prism/core/lens";
import { createPuckComponentRegistry } from "./component-registry.js";
import {
  registerLensBundlesInPuck,
  registerShellWidgetBundlesInPuck,
  type LensPuckConfig,
} from "./lens-puck-adapter.js";

type FakeKernel = { id: string };

function fakeComponent(label: string): ComponentType {
  const C = (() => label) as unknown as ComponentType;
  (C as { displayName?: string }).displayName = label;
  return C;
}

function makeManifest(id: string, name: string): LensManifest {
  return {
    id: lensId(id),
    name,
    icon: "",
    category: "custom",
    contributes: { views: [{ slot: "main" }], commands: [] },
  };
}

function makeLensBundle(
  id: string,
  name: string,
  puck?: LensPuckConfig,
): LensBundle<ComponentType, LensPuckConfig> {
  const manifest = makeManifest(id, name);
  const component = fakeComponent(name);
  return {
    id,
    name,
    manifest,
    component,
    ...(puck !== undefined ? { puck } : {}),
    install() {
      return () => {};
    },
  };
}

function makeShellWidgetBundle(
  id: string,
  name: string,
  puck: LensPuckConfig,
): ShellWidgetBundle<ComponentType, LensPuckConfig> {
  return {
    id,
    name,
    component: fakeComponent(name),
    puck,
    install() {
      return () => {};
    },
  };
}

describe("registerLensBundlesInPuck", () => {
  it("registers only embeddable lens bundles as direct components", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    const results = registerLensBundlesInPuck(
      [
        makeLensBundle("editor", "Editor", { embeddable: true }),
        makeLensBundle("graph", "Graph", { embeddable: false }),
        makeLensBundle("canvas", "Canvas"),
      ],
      registry,
    );

    expect(results).toHaveLength(3);
    expect(results[0]).toMatchObject({ name: "Editor", registered: true });
    expect(results[1]).toMatchObject({
      name: "Graph",
      registered: false,
      skipReason: "not-embeddable",
    });
    expect(results[2]).toMatchObject({
      name: "Canvas",
      registered: false,
      skipReason: "no-puck",
    });
    expect(registry.directNames()).toEqual(["Editor"]);
  });

  it("skips bundles missing a puck config with skipReason='no-puck'", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    const bundle: LensBundle<ComponentType, LensPuckConfig> = {
      id: "loose",
      name: "Loose",
      manifest: makeManifest("loose", "Loose"),
      component: fakeComponent("Loose"),
      install() {
        return () => {};
      },
    };
    const [result] = registerLensBundlesInPuck([bundle], registry);
    expect(result).toMatchObject({
      name: "Loose",
      registered: false,
      skipReason: "no-puck",
    });
    expect(registry.directNames()).toEqual([]);
  });

  it("uses kebab→pascal for the registered component name", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    registerLensBundlesInPuck(
      [makeLensBundle("visual-script", "Visual Script", { embeddable: true })],
      registry,
    );
    expect(registry.hasDirect("VisualScript")).toBe(true);
  });

  it("carries the label and fields into the component config", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    registerLensBundlesInPuck(
      [
        makeLensBundle("editor", "Editor", {
          label: "Editor!",
          fields: { tone: { type: "text" } },
          embeddable: true,
          defaultProps: { tone: "dark" },
        }),
      ],
      registry,
    );
    const cfg = registry.getDirect("Editor");
    expect(cfg?.label).toBe("Editor!");
    expect(cfg?.fields).toEqual({ tone: { type: "text" } });
    expect((cfg as { defaultProps?: unknown }).defaultProps).toEqual({
      tone: "dark",
    });
  });
});

describe("registerShellWidgetBundlesInPuck", () => {
  it("registers shell widgets by default", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    const results = registerShellWidgetBundlesInPuck(
      [
        makeShellWidgetBundle("activity-bar", "Activity Bar", { label: "AB" }),
        makeShellWidgetBundle("tab-bar", "Tab Bar", { label: "TB" }),
      ],
      registry,
    );
    expect(results.map((r) => r.name)).toEqual(["ActivityBar", "TabBar"]);
    expect(results.every((r) => r.registered)).toBe(true);
    expect(registry.directNames().sort()).toEqual(["ActivityBar", "TabBar"]);
  });

  it("skips shell widgets with embeddable=false", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    const [result] = registerShellWidgetBundlesInPuck(
      [
        makeShellWidgetBundle("silent", "Silent", {
          label: "Silent",
          embeddable: false,
        }),
      ],
      registry,
    );
    expect(result).toMatchObject({
      name: "Silent",
      registered: false,
      skipReason: "not-embeddable",
    });
    expect(registry.directNames()).toEqual([]);
  });
});
