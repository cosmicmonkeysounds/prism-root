import { describe, it, expect, vi } from "vitest";
import { LoroDoc } from "loro-crdt";
import { createPuckLoroBridge } from "./loro-puck-bridge.js";
import type { Data } from "@measured/puck";

describe("createPuckLoroBridge", () => {
  it("should return empty Puck data when no state exists", () => {
    const bridge = createPuckLoroBridge();
    const data = bridge.getData();
    expect(data).toEqual({ content: [], root: { props: {} } });
  });

  it("should store and retrieve Puck data via Loro", () => {
    const bridge = createPuckLoroBridge();
    const testData: Data = {
      content: [
        {
          type: "Heading",
          props: { text: "Hello", level: "h1", id: "heading-1" },
        },
      ],
      root: { props: {} },
    };

    bridge.setData(testData);
    const retrieved = bridge.getData();

    expect(retrieved.content).toHaveLength(1);
    expect(retrieved.content[0]?.type).toBe("Heading");
    expect(retrieved.content[0]?.props?.text).toBe("Hello");
  });

  it("should overwrite existing data", () => {
    const bridge = createPuckLoroBridge();

    bridge.setData({
      content: [
        { type: "Text", props: { content: "First", id: "text-1" } },
      ],
      root: { props: {} },
    });

    bridge.setData({
      content: [
        { type: "Text", props: { content: "Second", id: "text-2" } },
      ],
      root: { props: {} },
    });

    const data = bridge.getData();
    expect(data.content).toHaveLength(1);
    expect(data.content[0]?.props?.content).toBe("Second");
  });

  it("should notify subscribers on data change", () => {
    const bridge = createPuckLoroBridge();
    const callback = vi.fn();

    bridge.subscribe(callback);
    bridge.setData({
      content: [
        { type: "Card", props: { title: "Test", id: "card-1" } },
      ],
      root: { props: {} },
    });

    expect(callback).toHaveBeenCalled();
    const callData = callback.mock.calls[0]?.[0] as Data;
    expect(callData.content).toHaveLength(1);
  });

  it("should allow unsubscribing", () => {
    const bridge = createPuckLoroBridge();
    const callback = vi.fn();

    const unsub = bridge.subscribe(callback);
    bridge.setData({ content: [], root: { props: {} } });
    expect(callback).toHaveBeenCalledTimes(1);

    unsub();
    bridge.setData({ content: [], root: { props: {} } });
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it("should use a provided LoroDoc", () => {
    const doc = new LoroDoc();
    const bridge = createPuckLoroBridge(doc);

    bridge.setData({
      content: [{ type: "Text", props: { content: "Shared", id: "t-1" } }],
      root: { props: {} },
    });

    // Verify data is in the provided doc
    const root = doc.getMap("root");
    const raw = root.get("puck_layout");
    expect(typeof raw).toBe("string");
  });

  it("should round-trip slot-shaped data (Puck 0.20 slots)", () => {
    const bridge = createPuckLoroBridge();
    // Puck 0.20 stores slot content as nested ComponentData[] on props.
    const nested: Data = {
      content: [
        {
          type: "PageShell",
          props: {
            id: "shell-1",
            topBarHeight: 64,
            leftBarWidth: 240,
            rightBarWidth: 200,
            bottomBarHeight: 32,
            topBar: [
              { type: "SiteHeader", props: { id: "h-1", brand: "Acme" } },
            ],
            leftBar: [
              { type: "Heading", props: { id: "sb-h", text: "Menu", level: "h3" } },
              { type: "NavBar", props: { id: "sb-n", align: "start", links: [] } },
            ],
            main: [
              {
                type: "Hero",
                props: {
                  id: "hero-1",
                  align: "center",
                  minHeight: 300,
                  content: [
                    { type: "Heading", props: { id: "h2", text: "Welcome", level: "h1" } },
                  ],
                },
              },
            ],
            rightBar: [],
            bottomBar: [],
          },
        },
      ],
      root: { props: {} },
    };
    bridge.setData(nested);
    const out = bridge.getData();
    const shell = out.content[0];
    expect(shell?.type).toBe("PageShell");
    const shellProps = shell?.props as Record<string, unknown>;
    expect(shellProps["leftBarWidth"]).toBe(240);
    expect(Array.isArray(shellProps["topBar"])).toBe(true);
    expect((shellProps["topBar"] as unknown[]).length).toBe(1);
    const main = shellProps["main"] as Array<{
      type: string;
      props: { content?: unknown };
    }>;
    expect(main[0]?.type).toBe("Hero");
    const heroContent = main[0]?.props.content as unknown[] | undefined;
    expect(Array.isArray(heroContent)).toBe(true);
    expect((heroContent ?? [])[0]).toMatchObject({
      type: "Heading",
      props: { text: "Welcome" },
    });
  });

  it("should merge Puck state from two peers via CRDT", () => {
    const doc1 = new LoroDoc();
    doc1.setPeerId(1n);
    const bridge1 = createPuckLoroBridge(doc1);

    const doc2 = new LoroDoc();
    doc2.setPeerId(2n);
    const bridge2 = createPuckLoroBridge(doc2);

    bridge1.setData({
      content: [{ type: "Heading", props: { text: "From Peer 1", id: "h-1" } }],
      root: { props: {} },
    });

    // Sync doc1 → doc2
    const snapshot = doc1.export({ mode: "snapshot" });
    doc2.import(snapshot);

    const data2 = bridge2.getData();
    expect(data2.content[0]?.props?.text).toBe("From Peer 1");
  });
});
