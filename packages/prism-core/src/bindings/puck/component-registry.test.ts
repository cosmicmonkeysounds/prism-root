import { describe, expect, it } from "vitest";
import type { EntityDef } from "@prism/core/object-model";
import type { ComponentConfig } from "@measured/puck";
import {
  PuckComponentRegistry,
  createPuckComponentRegistry,
  kebabToPascal,
  type PuckComponentProvider,
} from "./component-registry.js";

type FakeKernel = { id: string };

function makeDef(type: string, category = "component"): EntityDef {
  return { type, category, label: type };
}

function makeProvider(
  type: string,
  label: string,
): PuckComponentProvider<FakeKernel> {
  return {
    type,
    buildConfig: ({ def, kernel }) => ({
      fields: {
        kernelId: { type: "text" },
        defType: { type: "text" },
      },
      defaultProps: {
        kernelId: kernel.id,
        defType: def.type,
      },
      render: () => label as unknown as ReturnType<ComponentConfig["render"]>,
    }),
  };
}

describe("kebabToPascal", () => {
  it("uppercases each hyphen-separated word", () => {
    expect(kebabToPascal("record-list")).toBe("RecordList");
    expect(kebabToPascal("tasks-widget")).toBe("TasksWidget");
  });

  it("passes single words through with capitalised first letter", () => {
    expect(kebabToPascal("button")).toBe("Button");
  });

  it("tolerates empty segments without throwing", () => {
    expect(kebabToPascal("foo--bar")).toBe("FooBar");
  });
});

describe("PuckComponentRegistry", () => {
  it("registers providers and exposes them by type", () => {
    const registry = createPuckComponentRegistry<FakeKernel>();
    const provider = makeProvider("record-list", "rl");

    registry.register(provider);

    expect(registry.has("record-list")).toBe(true);
    expect(registry.get("record-list")).toBe(provider);
    expect(registry.types()).toEqual(["record-list"]);
  });

  it("register() returns this so calls can be chained", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    const result = registry
      .register(makeProvider("a", "a"))
      .register(makeProvider("b", "b"));

    expect(result).toBe(registry);
    expect(registry.types()).toEqual(["a", "b"]);
  });

  it("registerAll accepts an array of providers", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.registerAll([
      makeProvider("a", "a"),
      makeProvider("b", "b"),
      makeProvider("c", "c"),
    ]);
    expect(registry.types().sort()).toEqual(["a", "b", "c"]);
  });

  it("later registrations override earlier ones for the same type", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    const first = makeProvider("x", "first");
    const second = makeProvider("x", "second");
    registry.register(first).register(second);
    expect(registry.get("x")).toBe(second);
  });

  it("unregister removes a provider and returns whether it existed", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.register(makeProvider("a", "a"));
    expect(registry.unregister("a")).toBe(true);
    expect(registry.unregister("a")).toBe(false);
    expect(registry.has("a")).toBe(false);
  });

  it("buildComponents only emits entries for registered types", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.register(makeProvider("record-list", "rl"));

    const defs = [
      makeDef("record-list"),
      makeDef("heading"),
      makeDef("tasks-widget"),
    ];

    const components = registry.buildComponents({
      defs,
      kernel: { id: "test-kernel" },
    });

    expect(Object.keys(components)).toEqual(["RecordList"]);
    expect(components["RecordList"]?.defaultProps).toEqual({
      kernelId: "test-kernel",
      defType: "record-list",
    });
  });

  it("buildComponents passes the def and kernel to provider.buildConfig", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    let capturedDefType = "";
    let capturedKernelId = "";
    registry.register({
      type: "probe",
      buildConfig: ({ def, kernel }) => {
        capturedDefType = def.type;
        capturedKernelId = kernel.id;
        return { fields: {}, render: () => null };
      },
    });

    registry.buildComponents({
      defs: [makeDef("probe")],
      kernel: { id: "k-42" },
    });

    expect(capturedDefType).toBe("probe");
    expect(capturedKernelId).toBe("k-42");
  });

  it("buildComponents emits PascalCase keys for kebab-case entity types", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.registerAll([
      makeProvider("record-list", "rl"),
      makeProvider("multi-word-widget", "mw"),
    ]);

    const components = registry.buildComponents({
      defs: [makeDef("record-list"), makeDef("multi-word-widget")],
      kernel: { id: "k" },
    });

    expect(Object.keys(components).sort()).toEqual([
      "MultiWordWidget",
      "RecordList",
    ]);
  });

  it("buildComponents returns an empty object when nothing matches", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.register(makeProvider("unused", "u"));

    const components = registry.buildComponents({
      defs: [makeDef("heading"), makeDef("button")],
      kernel: { id: "k" },
    });

    expect(components).toEqual({});
  });

  it("registerDirect puts a full ComponentConfig under an explicit name", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    const config: ComponentConfig = {
      label: "Activity Bar",
      fields: {},
      render: () => null,
    };
    registry.registerDirect("ActivityBar", config);

    expect(registry.hasDirect("ActivityBar")).toBe(true);
    expect(registry.getDirect("ActivityBar")).toBe(config);
    expect(registry.directNames()).toEqual(["ActivityBar"]);
  });

  it("buildComponents merges direct entries with provider-derived entries", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.register(makeProvider("record-list", "rl"));
    registry.registerDirect("ActivityBar", {
      label: "Activity Bar",
      fields: {},
      render: () => null,
    });

    const components = registry.buildComponents({
      defs: [makeDef("record-list")],
      kernel: { id: "k" },
    });

    expect(Object.keys(components).sort()).toEqual([
      "ActivityBar",
      "RecordList",
    ]);
  });

  it("direct entries override provider-derived entries on name collision", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.register(makeProvider("activity-bar", "from-provider"));
    const overriding: ComponentConfig = {
      label: "Override",
      fields: {},
      render: () => null,
    };
    registry.registerDirect("ActivityBar", overriding);

    const components = registry.buildComponents({
      defs: [makeDef("activity-bar")],
      kernel: { id: "k" },
    });

    expect(components["ActivityBar"]).toBe(overriding);
  });

  it("unregisterDirect removes a direct entry", () => {
    const registry = new PuckComponentRegistry<FakeKernel>();
    registry.registerDirect("Foo", { fields: {}, render: () => null });
    expect(registry.unregisterDirect("Foo")).toBe(true);
    expect(registry.unregisterDirect("Foo")).toBe(false);
    expect(registry.hasDirect("Foo")).toBe(false);
  });
});
