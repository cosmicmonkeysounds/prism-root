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
    data: { layout: "sidebar-left", sidebarWidth: 240, stickyHeader: true },
  });
  kernel.createObject({
    type: "heading",
    name: "Brand",
    parentId: shell.id,
    position: 0,
    data: { __slot: "header", text: "Acme", level: "h1", align: "left" },
  });
  kernel.createObject({
    type: "heading",
    name: "Menu",
    parentId: shell.id,
    position: 0,
    data: { __slot: "sidebar", text: "Menu", level: "h3", align: "left" },
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
      // All four shell slots must exist as arrays — otherwise Puck's
      // `defaultSlots` fallback kicks in and the data layer never sees them.
      expect(Array.isArray(props["header"])).toBe(true);
      expect(Array.isArray(props["sidebar"])).toBe(true);
      expect(Array.isArray(props["main"])).toBe(true);
      expect(Array.isArray(props["footer"])).toBe(true);

      const header = props["header"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(header).toHaveLength(1);
      expect(header[0]?.type).toBe("Heading");
      expect(header[0]?.props["text"]).toBe("Acme");

      const sidebar = props["sidebar"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(sidebar).toHaveLength(1);
      expect(sidebar[0]?.type).toBe("Heading");

      const main = props["main"] as Array<{ type: string; props: Record<string, unknown> }>;
      expect(main).toHaveLength(1);
      expect(main[0]?.type).toBe("TextBlock");

      const footer = props["footer"] as unknown[];
      expect(footer).toEqual([]);
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
      // four slot fields and Heading / TextBlock as leaf components. This
      // isolates the data ↔ config contract from the full puckConfig.
      const slotField = { type: "slot" } as unknown as Fields[string];
      const config: Config = {
        components: {
          PageShell: {
            fields: {
              layout: { type: "text" } as unknown as Fields[string],
              sidebarWidth: { type: "number" } as unknown as Fields[string],
              stickyHeader: { type: "text" } as unknown as Fields[string],
              header: slotField,
              sidebar: slotField,
              main: slotField,
              footer: slotField,
            },
            defaultProps: {
              layout: "sidebar-left",
              sidebarWidth: 240,
              stickyHeader: "true",
              header: [],
              sidebar: [],
              main: [],
              footer: [],
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

describe("kernelToPuckData — page-level slot projection", () => {
  it("exposes page.data layout props and slot children on root.props", () => {
    const kernel = createStudioKernel();
    try {
      const page = kernel.createObject({
        type: "page",
        name: "Sidebar page",
        parentId: null,
        position: 0,
        data: {
          title: "Sidebar page",
          slug: "/demo",
          layout: "sidebar-left",
          sidebarWidth: 260,
          stickyHeader: true,
        },
      });
      kernel.createObject({
        type: "heading",
        name: "Brand",
        parentId: page.id,
        position: 0,
        data: { __slot: "header", text: "Acme", level: "h1" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Nav",
        parentId: page.id,
        position: 0,
        data: { __slot: "sidebar", content: "- Home\n- About" },
      });
      kernel.createObject({
        type: "text-block",
        name: "Copyright",
        parentId: page.id,
        position: 0,
        data: { __slot: "footer", content: "© 2026" },
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
      expect(root["layout"]).toBe("sidebar-left");
      expect(root["sidebarWidth"]).toBe(260);
      expect(root["stickyHeader"]).toBe(true);

      const header = root["header"] as Array<{ type: string }>;
      const sidebar = root["sidebar"] as Array<{ type: string }>;
      const footer = root["footer"] as Array<{ type: string }>;
      expect(header).toHaveLength(1);
      expect(header[0]?.type).toBe("Heading");
      expect(sidebar).toHaveLength(1);
      expect(sidebar[0]?.type).toBe("TextBlock");
      expect(footer).toHaveLength(1);
      expect(footer[0]?.type).toBe("TextBlock");
    } finally {
      kernel.dispose();
    }
  });
});

describe("splitRootProps", () => {
  it("partitions scalar props from per-slot arrays", () => {
    const { pageData, slots } = splitRootProps({
      title: "Hi",
      layout: "sidebar-left",
      sidebarWidth: 240,
      header: [{ type: "Heading", props: { id: "h1", text: "Brand" } }],
      sidebar: [],
      footer: [{ type: "TextBlock", props: { id: "t1", content: "©" } }],
    });
    expect(pageData).toEqual({ title: "Hi", layout: "sidebar-left", sidebarWidth: 240 });
    expect(slots["header"]).toHaveLength(1);
    expect(slots["sidebar"]).toEqual([]);
    expect(slots["footer"]).toHaveLength(1);
  });

  it("defaults every PAGE_SLOTS key to an empty array when missing", () => {
    const { slots } = splitRootProps({ layout: "flow" });
    for (const s of PAGE_SLOTS) {
      expect(slots[s]).toEqual([]);
    }
  });

  it("coerces non-array slot values to empty arrays", () => {
    const { slots } = splitRootProps({ header: "not-an-array" as unknown });
    expect(slots["header"]).toEqual([]);
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
