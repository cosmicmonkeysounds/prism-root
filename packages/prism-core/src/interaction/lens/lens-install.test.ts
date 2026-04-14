import { describe, it, expect, vi } from "vitest";
import { createLensRegistry } from "./lens-registry.js";
import {
  defineLensBundle,
  installLensBundles,
  defineShellWidgetBundle,
  installShellWidgetBundles,
  type LensBundle,
  type LensInstallContext,
  type ShellWidgetBundle,
} from "./lens-install.js";
import { lensId, type LensManifest } from "./lens-types.js";

type FakeComponent = { tag: string };

function makeManifest(id: string, name: string): LensManifest {
  return {
    id: lensId(id),
    name,
    icon: "",
    category: "custom",
    contributes: { views: [{ slot: "main" }], commands: [] },
  };
}

function makeCtx(): LensInstallContext<FakeComponent> {
  return {
    lensRegistry: createLensRegistry(),
    componentMap: new Map(),
  };
}

describe("defineLensBundle", () => {
  it("registers the manifest and component on install", () => {
    const manifest = makeManifest("foo", "Foo");
    const comp: FakeComponent = { tag: "FooComp" };
    const bundle = defineLensBundle(manifest, comp);

    const ctx = makeCtx();
    const uninstall = bundle.install(ctx);

    expect(ctx.lensRegistry.get(manifest.id)).toBe(manifest);
    expect(ctx.componentMap.get(manifest.id)).toBe(comp);

    uninstall();
    expect(ctx.lensRegistry.has(manifest.id)).toBe(false);
    expect(ctx.componentMap.has(manifest.id)).toBe(false);
  });

  it("derives bundle id and name from manifest", () => {
    const manifest = makeManifest("bar", "Bar Lens");
    const bundle = defineLensBundle(manifest, { tag: "x" });
    expect(bundle.id).toBe(manifest.id);
    expect(bundle.name).toBe("Bar Lens");
  });

  it("exposes manifest and component as plain data", () => {
    const manifest = makeManifest("qux", "Qux");
    const comp: FakeComponent = { tag: "QuxComp" };
    const bundle = defineLensBundle(manifest, comp);
    expect(bundle.manifest).toBe(manifest);
    expect(bundle.component).toBe(comp);
    expect(bundle.puck).toBeUndefined();
  });

  it("carries an opaque puck config when supplied", () => {
    const manifest = makeManifest("zap", "Zap");
    const puck = { label: "Zap!", fields: { tone: { type: "text" } } };
    const bundle = defineLensBundle(manifest, { tag: "Z" }, puck);
    expect(bundle.puck).toBe(puck);
  });
});

describe("installLensBundles", () => {
  it("installs all bundles and returns a combined uninstall", () => {
    const a = defineLensBundle<FakeComponent>(makeManifest("a", "A"), { tag: "A" });
    const b = defineLensBundle<FakeComponent>(makeManifest("b", "B"), { tag: "B" });
    const c = defineLensBundle<FakeComponent>(makeManifest("c", "C"), { tag: "C" });

    const ctx = makeCtx();
    const uninstall = installLensBundles([a, b, c], ctx);

    expect(ctx.lensRegistry.allLenses()).toHaveLength(3);
    expect(ctx.componentMap.size).toBe(3);

    uninstall();
    expect(ctx.lensRegistry.allLenses()).toHaveLength(0);
    expect(ctx.componentMap.size).toBe(0);
  });

  it("uninstalls in reverse order", () => {
    const order: string[] = [];
    const makeTracking = (id: string): LensBundle<FakeComponent> => ({
      id,
      name: id,
      install() {
        return () => {
          order.push(id);
        };
      },
    });

    const uninstall = installLensBundles(
      [makeTracking("1"), makeTracking("2"), makeTracking("3")],
      makeCtx(),
    );
    uninstall();
    expect(order).toEqual(["3", "2", "1"]);
  });

  it("propagates bundle install errors", () => {
    const failing: LensBundle<FakeComponent> = {
      id: "fail",
      name: "fail",
      install: vi.fn(() => {
        throw new Error("boom");
      }),
    };
    expect(() => installLensBundles([failing], makeCtx())).toThrow("boom");
  });
});

describe("defineShellWidgetBundle", () => {
  it("installs and uninstalls a chrome widget into a component map", () => {
    const widget = defineShellWidgetBundle<FakeComponent, { label: string }>({
      id: "activity-bar",
      name: "Activity Bar",
      component: { tag: "ActivityBar" },
      puck: { label: "Activity Bar" },
    });

    const componentMap = new Map<string, FakeComponent>();
    const uninstall = widget.install({ componentMap });
    expect(componentMap.get("activity-bar")).toEqual({ tag: "ActivityBar" });
    expect(widget.puck.label).toBe("Activity Bar");

    uninstall();
    expect(componentMap.has("activity-bar")).toBe(false);
  });

  it("installShellWidgetBundles tears down in reverse order", () => {
    const order: string[] = [];
    const makeTracking = (id: string): ShellWidgetBundle<FakeComponent> => ({
      id,
      name: id,
      component: { tag: id },
      puck: {},
      install() {
        return () => {
          order.push(id);
        };
      },
    });
    const componentMap = new Map<string, FakeComponent>();
    const uninstall = installShellWidgetBundles(
      [makeTracking("a"), makeTracking("b"), makeTracking("c")],
      { componentMap },
    );
    uninstall();
    expect(order).toEqual(["c", "b", "a"]);
  });
});
