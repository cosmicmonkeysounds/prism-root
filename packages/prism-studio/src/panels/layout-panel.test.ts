/**
 * Diagnostic tests for the Puck layout panel's shell slot handling.
 *
 * These tests don't render the Puck UI (we're in a node vitest env without a
 * DOM). Instead they verify:
 *   1. `kernelToPuckData` projects shell children into per-slot content arrays
 *      shaped the way Puck 0.20 expects (nested `ComponentData[]` on props).
 *   2. Puck's own `walkTree` helper — given that same data and a slot-aware
 *      config — recognises the slot fields and walks into them. If walkTree
 *      DOESN'T descend, the editor will never render drop zones.
 */

import { describe, it, expect } from "vitest";
import { walkTree, type Config, type Fields } from "@measured/puck";
import type { ObjectId } from "@prism/core/object-model";
import { createStudioKernel, type StudioKernel } from "../kernel/index.js";
import {
  kernelToPuckData,
  SHELL_SLOTS,
  PAGE_SLOTS,
  splitRootProps,
  buildPuckCategories,
  COMPONENT_CATEGORY_MAP,
  CATEGORY_TITLES,
  kebabToPascal,
  pascalToKebab,
} from "./layout-panel-data.js";
import {
  facetIdFromName,
  uniqueFacetId,
} from "../components/facet-picker-helpers.js";
import type { FacetDefinition } from "@prism/core/facet";

function seedShellPage(kernel: StudioKernel): ObjectId {
  const page = kernel.createObject({
    type: "page",
    name: "Shell Page",
    parentId: null,
    position: 0,
    data: { title: "Shell Page", slug: "/shell", published: false },
  });
  const shell = kernel.createObject({
    type: "page-shell",
    name: "Page Shell",
    parentId: page.id,
    position: 0,
    data: {
      topBarHeight: 64,
      leftBarWidth: 240,
      rightBarWidth: 0,
      bottomBarHeight: 0,
      stickyTopBar: true,
    },
  });
  kernel.createObject({
    type: "heading",
    name: "Brand",
    parentId: shell.id,
    position: 0,
    data: { __slot: "topBar", text: "Acme", level: "h1", align: "left" },
  });
  kernel.createObject({
    type: "heading",
    name: "Menu",
    parentId: shell.id,
    position: 0,
    data: { __slot: "leftBar", text: "Menu", level: "h3", align: "left" },
  });
  kernel.createObject({
    type: "text-block",
    name: "Body",
    parentId: shell.id,
    position: 0,
    data: { __slot: "main", content: "Welcome to the shell page." },
  });
  return page.id;
}

describe("kernelToPuckData — shell slot projection", () => {
  it("emits per-slot content arrays on the PageShell's props", () => {
    const kernel = createStudioKernel();
    try {
      const pageId = seedShellPage(kernel);
      const allObjects = kernel.store.allObjects().filter((o) => !o.deletedAt);
      const data = kernelToPuckData(pageId, allObjects);

      expect(data.content).toHaveLength(1);
      const shell = data.content[0];
      expect(shell?.type).toBe("PageShell");

      const props = shell?.props as Record<string, unknown>;
      // All five shell slots must exist as arrays — otherwise Puck's
      // `defaultSlots` fallback kicks in and the data layer never sees them.
      expect(Array.isArray(props["topBar"])).toBe(true);
      expect(Array.isArray(props["leftBar"])).toBe(true);
      expect(Array.isArray(props["main"])).toBe(true);
      expect(Array.isArray(props["rightBar"])).toBe(true);
      expect(Array.isArray(props["bottomBar"])).toBe(true);

      const topBar = props["topBar"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(topBar).toHaveLength(1);
      expect(topBar[0]?.type).toBe("Heading");
      expect(topBar[0]?.props["text"]).toBe("Acme");

      const leftBar = props["leftBar"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(leftBar).toHaveLength(1);
      expect(leftBar[0]?.type).toBe("Heading");

      const main = props["main"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(main).toHaveLength(1);
      expect(main[0]?.type).toBe("TextBlock");

      const rightBar = props["rightBar"] as unknown[];
      expect(rightBar).toEqual([]);
      const bottomBar = props["bottomBar"] as unknown[];
      expect(bottomBar).toEqual([]);
    } finally {
      kernel.dispose();
    }
  });

  it("round-trips through Puck walkTree with a slot-aware config", () => {
    const kernel = createStudioKernel();
    try {
      const pageId = seedShellPage(kernel);
      const allObjects = kernel.store.allObjects().filter((o) => !o.deletedAt);
      const data = kernelToPuckData(pageId, allObjects);

      // Minimal stand-in config for the assertion: declare PageShell with
      // five slot fields and Heading / TextBlock as leaf components. This
      // isolates the data ↔ config contract from the full puckConfig.
      const slotField = { type: "slot" } as unknown as Fields[string];
      const config: Config = {
        components: {
          PageShell: {
            fields: {
              topBarHeight: { type: "number" } as unknown as Fields[string],
              leftBarWidth: { type: "number" } as unknown as Fields[string],
              rightBarWidth: { type: "number" } as unknown as Fields[string],
              bottomBarHeight: { type: "number" } as unknown as Fields[string],
              stickyTopBar: { type: "text" } as unknown as Fields[string],
              topBar: slotField,
              leftBar: slotField,
              main: slotField,
              rightBar: slotField,
              bottomBar: slotField,
            },
            defaultProps: {
              topBarHeight: 0,
              leftBarWidth: 0,
              rightBarWidth: 0,
              bottomBarHeight: 0,
              stickyTopBar: "true",
              topBar: [],
              leftBar: [],
              main: [],
              rightBar: [],
              bottomBar: [],
            },
            render: () => null,
          },
          Heading: {
            fields: {
              text: { type: "text" } as unknown as Fields[string],
              level: { type: "text" } as unknown as Fields[string],
              align: { type: "text" } as unknown as Fields[string],
            },
            defaultProps: { text: "", level: "h2", align: "left" },
            render: () => null,
          },
          TextBlock: {
            fields: {
              content: { type: "textarea" } as unknown as Fields[string],
            },
            defaultProps: { content: "" },
            render: () => null,
          },
        },
      } as unknown as Config;

      // Collect every slot zone that walkTree visits. If our data + config
      // are wired correctly, Puck should step into header/sidebar/main/footer
      // (the four PageShell slots) and into the leaf heading/text-block
      // (which have no slots, so no further zones).
      const visitedZones: string[] = [];
      walkTree(data, config, (content, options) => {
        const info = options as { parentId?: string; propName?: string };
        if (info?.propName !== undefined && info.parentId) {
          visitedZones.push(`${info.parentId}:${info.propName}`);
        }
        return content;
      });

      // Must have visited all four PageShell slots — this is what registers
      // them as dropzones in the editor.
      const slotNames = SHELL_SLOTS["page-shell"] ?? [];
      for (const slot of slotNames) {
        const matched = visitedZones.find((z) => z.endsWith(`:${slot}`));
        expect(
          matched,
          `walkTree never descended into PageShell slot "${slot}"`,
        ).toBeDefined();
      }
    } finally {
      kernel.dispose();
    }
  });
});

describe("kernelToPuckData — app-shell slot projection", () => {
  it("projects an app-shell's slot children into Puck props the same way page-shell does", () => {
    const kernel = createStudioKernel();
    try {
      // Parent an app-shell under a page (app entities aren't valid as the
      // page tree root, but SHELL_SLOTS + kernelToPuckData treat app-shell
      // identically to page-shell — that's the contract we're pinning).
      const page = kernel.createObject({
        type: "page",
        name: "App Host",
        parentId: null,
        position: 0,
        data: { title: "App Host", slug: "/app", published: false },
      });
      const appShell = kernel.createObject({
        type: "app-shell",
        name: "App Shell",
        parentId: page.id,
        position: 0,
        data: {
          brand: "Acme",
          topBarHeight: 56,
          leftBarWidth: 220,
          rightBarWidth: 0,
          bottomBarHeight: 0,
          stickyTopBar: true,
        },
      });
      kernel.createObject({
        type: "heading",
        name: "Brand",
        parentId: appShell.id,
        position: 0,
        data: { __slot: "topBar", text: "Acme", level: "h1" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Menu",
        parentId: appShell.id,
        position: 0,
        data: { __slot: "leftBar", content: "- Home\n- About" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Body",
        parentId: appShell.id,
        position: 0,
        data: { __slot: "main", content: "App content" },
      });

      const all = kernel.store.allObjects().filter((o) => !o.deletedAt);
      const data = kernelToPuckData(page.id, all);

      expect(data.content).toHaveLength(1);
      const shell = data.content[0];
      expect(shell?.type).toBe("AppShell");

      const props = shell?.props as Record<string, unknown>;
      expect(props["brand"]).toBe("Acme");
      // All five bar slots projected as arrays.
      expect(Array.isArray(props["topBar"])).toBe(true);
      expect(Array.isArray(props["leftBar"])).toBe(true);
      expect(Array.isArray(props["main"])).toBe(true);
      expect(Array.isArray(props["rightBar"])).toBe(true);
      expect(Array.isArray(props["bottomBar"])).toBe(true);

      const topBar = props["topBar"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(topBar).toHaveLength(1);
      expect(topBar[0]?.type).toBe("Heading");

      const leftBar = props["leftBar"] as Array<{ type: string }>;
      expect(leftBar).toHaveLength(1);
      expect(leftBar[0]?.type).toBe("TextBlock");

      const main = props["main"] as Array<{ type: string }>;
      expect(main).toHaveLength(1);
      expect(main[0]?.type).toBe("TextBlock");

      expect(SHELL_SLOTS["app-shell"]).toEqual(SHELL_SLOTS["page-shell"]);
    } finally {
      kernel.dispose();
    }
  });
});

describe("kernelToPuckData — page-level slot projection", () => {
  it("exposes page.data layout props and slot children on root.props", () => {
    const kernel = createStudioKernel();
    try {
      const page = kernel.createObject({
        type: "page",
        name: "Shell page",
        parentId: null,
        position: 0,
        data: {
          title: "Shell page",
          slug: "/demo",
          layout: "shell",
          topBarHeight: 64,
          leftBarWidth: 260,
          rightBarWidth: 200,
          bottomBarHeight: 32,
          stickyTopBar: true,
        },
      });
      kernel.createObject({
        type: "heading",
        name: "Brand",
        parentId: page.id,
        position: 0,
        data: { __slot: "topBar", text: "Acme", level: "h1" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Nav",
        parentId: page.id,
        position: 0,
        data: { __slot: "leftBar", content: "- Home\n- About" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Tools",
        parentId: page.id,
        position: 0,
        data: { __slot: "rightBar", content: "Inspector" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Copyright",
        parentId: page.id,
        position: 0,
        data: { __slot: "bottomBar", content: "© 2026" },
      });
      kernel.createObject({
        type: "heading",
        name: "Main heading",
        parentId: page.id,
        position: 0,
        data: { text: "Hi there", level: "h2" },
      });

      const all = kernel.store.allObjects().filter((o) => !o.deletedAt);
      const data = kernelToPuckData(page.id, all);

      // Main flow: only the non-slotted child.
      expect(data.content).toHaveLength(1);
      expect(data.content[0]?.type).toBe("Heading");

      const root = data.root?.props as Record<string, unknown>;
      expect(root["layout"]).toBe("shell");
      expect(root["topBarHeight"]).toBe(64);
      expect(root["leftBarWidth"]).toBe(260);
      expect(root["rightBarWidth"]).toBe(200);
      expect(root["bottomBarHeight"]).toBe(32);
      expect(root["stickyTopBar"]).toBe(true);

      const topBar = root["topBar"] as Array<{ type: string }>;
      const leftBar = root["leftBar"] as Array<{ type: string }>;
      const rightBar = root["rightBar"] as Array<{ type: string }>;
      const bottomBar = root["bottomBar"] as Array<{ type: string }>;
      expect(topBar).toHaveLength(1);
      expect(topBar[0]?.type).toBe("Heading");
      expect(leftBar).toHaveLength(1);
      expect(leftBar[0]?.type).toBe("TextBlock");
      expect(rightBar).toHaveLength(1);
      expect(rightBar[0]?.type).toBe("TextBlock");
      expect(bottomBar).toHaveLength(1);
      expect(bottomBar[0]?.type).toBe("TextBlock");
    } finally {
      kernel.dispose();
    }
  });
});

describe("splitRootProps", () => {
  it("partitions scalar props from per-slot arrays", () => {
    const { pageData, slots } = splitRootProps({
      title: "Hi",
      layout: "shell",
      leftBarWidth: 240,
      topBar: [{ type: "Heading", props: { id: "h1", text: "Brand" } }],
      leftBar: [],
      rightBar: [],
      bottomBar: [{ type: "TextBlock", props: { id: "t1", content: "©" } }],
    });
    expect(pageData).toEqual({ title: "Hi", layout: "shell", leftBarWidth: 240 });
    expect(slots["topBar"]).toHaveLength(1);
    expect(slots["leftBar"]).toEqual([]);
    expect(slots["rightBar"]).toEqual([]);
    expect(slots["bottomBar"]).toHaveLength(1);
  });

  it("defaults every PAGE_SLOTS key to an empty array when missing", () => {
    const { slots } = splitRootProps({ layout: "flow" });
    for (const s of PAGE_SLOTS) {
      expect(slots[s]).toEqual([]);
    }
  });

  it("coerces non-array slot values to empty arrays", () => {
    const { slots } = splitRootProps({ topBar: "not-an-array" as unknown });
    expect(slots["topBar"]).toEqual([]);
  });

  it("handles undefined root props", () => {
    const { pageData, slots } = splitRootProps(undefined);
    expect(pageData).toEqual({});
    for (const s of PAGE_SLOTS) {
      expect(slots[s]).toEqual([]);
    }
  });
});

describe("buildPuckCategories", () => {
  it("drops empty non-other buckets", () => {
    // Only a layout component and a content component → media/forms/etc.
    // should be pruned.
    const categories = buildPuckCategories(["PageShell", "Heading"]);
    expect(Object.keys(categories)).toEqual(["layout", "content"]);
    expect(categories["layout"]?.components).toContain("PageShell");
    expect(categories["content"]?.components).toContain("Heading");
  });

  it("sends unknown component types to the other bucket", () => {
    const categories = buildPuckCategories(["MysteryBlock"]);
    expect(categories["other"]).toBeDefined();
    expect(categories["other"]?.components).toEqual(["MysteryBlock"]);
    // And nothing else, since there are no layout/content/etc. entries.
    expect(Object.keys(categories)).toEqual(["other"]);
  });

  it("marks layout + content as defaultExpanded", () => {
    const categories = buildPuckCategories([
      "PageShell",
      "Heading",
      "VideoWidget",
      "Button",
    ]);
    expect(categories["layout"]?.defaultExpanded).toBe(true);
    expect(categories["content"]?.defaultExpanded).toBe(true);
    expect(categories["media"]?.defaultExpanded).toBe(false);
    expect(categories["navigation"]?.defaultExpanded).toBe(false);
  });

  it("preserves insertion order across the canonical categories", () => {
    // Feed components in deliberately scrambled order; the category
    // record should still come out in CATEGORY_TITLES order so the
    // Puck sidebar stays stable regardless of Object.keys() order on
    // the upstream components map.
    const categories = buildPuckCategories([
      "Button", // navigation
      "Heading", // content
      "VideoWidget", // media
      "PageShell", // layout
      "FacetView", // dataViews
    ]);
    const keys = Object.keys(categories);
    const expectedOrder = Object.keys(CATEGORY_TITLES)
      .filter((k) => k !== "other")
      .filter((k) => keys.includes(k));
    expect(keys).toEqual(expectedOrder);
  });

  it("covers every component registered by the studio kernel", () => {
    const kernel = createStudioKernel();
    try {
      // Layout panel treats `component` AND `section` entities as Puck
      // components — everything the visual builder can drop onto a page.
      const componentDefs = kernel.registry
        .allDefs()
        .filter((d) => d.category === "component" || d.category === "section");
      const pascalNames = componentDefs.map((d) => kebabToPascal(d.type));

      const categories = buildPuckCategories(pascalNames);

      // Every def that has an explicit mapping must end up in that bucket.
      // Unmapped ones are allowed to land in `other` — the point is that
      // nothing silently goes missing.
      for (const def of componentDefs) {
        const pascal = kebabToPascal(def.type);
        const expectedBucket = COMPONENT_CATEGORY_MAP[def.type] ?? "other";
        const bucket = categories[expectedBucket];
        expect(
          bucket,
          `bucket "${expectedBucket}" missing for component "${def.type}"`,
        ).toBeDefined();
        expect(
          bucket?.components,
          `component "${pascal}" not placed in "${expectedBucket}" bucket`,
        ).toContain(pascal);
      }

      // Sanity: the total count across buckets equals the component count.
      const total = Object.values(categories).reduce(
        (sum, bucket) => sum + (bucket?.components.length ?? 0),
        0,
      );
      expect(total).toBe(pascalNames.length);
    } finally {
      kernel.dispose();
    }
  });
});

describe("pascalToKebab / kebabToPascal round-trip", () => {
  it("round-trips component type names used by the category map", () => {
    for (const kebab of Object.keys(COMPONENT_CATEGORY_MAP)) {
      const pascal = kebabToPascal(kebab);
      expect(pascalToKebab(pascal)).toBe(kebab);
    }
  });
});

describe("facetIdFromName", () => {
  it("slugifies human names into kernel-safe ids", () => {
    expect(facetIdFromName("Contact Form", "person")).toBe("person-contact-form");
    expect(facetIdFromName("Daily Standup!", "meeting")).toBe("meeting-daily-standup");
    expect(facetIdFromName("  Trim  me  ", "note")).toBe("note-trim-me");
  });

  it("collapses runs of non-alphanumerics into a single dash", () => {
    expect(facetIdFromName("A // B -- C", "task")).toBe("task-a-b-c");
  });

  it("falls back to a generic slug when the name is empty", () => {
    expect(facetIdFromName("", "record")).toBe("record-facet");
    expect(facetIdFromName("///", "record")).toBe("record-facet");
  });
});

describe("uniqueFacetId", () => {
  const fakeFacet = (id: string): FacetDefinition =>
    ({ id } as unknown as FacetDefinition);

  it("returns the base id when nothing collides", () => {
    expect(uniqueFacetId("person-note", [])).toBe("person-note");
    expect(
      uniqueFacetId("person-note", [fakeFacet("person-bio")]),
    ).toBe("person-note");
  });

  it("suffixes -2 on the first collision", () => {
    expect(
      uniqueFacetId("person-note", [fakeFacet("person-note")]),
    ).toBe("person-note-2");
  });

  it("walks past contiguous collisions", () => {
    const existing = [
      fakeFacet("person-note"),
      fakeFacet("person-note-2"),
      fakeFacet("person-note-3"),
    ];
    expect(uniqueFacetId("person-note", existing)).toBe("person-note-4");
  });
});
