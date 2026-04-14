import { describe, it, expect } from "vitest";
import {
  Children,
  isValidElement,
  type ReactElement,
  type ReactNode,
} from "react";
import { HelpMarkdown, slugify } from "./help-markdown.js";

function flatten(node: ReactNode): ReactElement[] {
  const out: ReactElement[] = [];
  Children.forEach(node, (child) => {
    if (isValidElement(child)) out.push(child);
  });
  return out;
}

function topLevelChildren(source: string): ReactElement[] {
  const root = HelpMarkdown({ source }) as ReactElement;
  const props = root.props as { children?: ReactNode };
  return flatten(props.children);
}

function textContent(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textContent).join("");
  if (isValidElement(node)) {
    const props = node.props as { children?: ReactNode };
    return textContent(props.children);
  }
  return "";
}

describe("slugify", () => {
  it("lower-cases and hyphenates whitespace", () => {
    expect(slugify("Hello World")).toBe("hello-world");
  });

  it("strips punctuation", () => {
    expect(slugify("What's new?")).toBe("whats-new");
  });

  it("collapses repeated whitespace and hyphens", () => {
    expect(slugify("Multiple   Spaces")).toBe("multiple-spaces");
    expect(slugify("already--hyphenated")).toBe("already-hyphenated");
  });

  it("trims leading and trailing whitespace", () => {
    expect(slugify("  Padded  ")).toBe("padded");
  });

  it("keeps digits", () => {
    expect(slugify("Step 2 of 3")).toBe("step-2-of-3");
  });
});

describe("HelpMarkdown", () => {
  it("renders a paragraph as <p>", () => {
    const [p] = topLevelChildren("Just a plain paragraph.");
    expect(p?.type).toBe("p");
    expect(textContent(p)).toBe("Just a plain paragraph.");
  });

  it("renders h1/h2/h3 with matching id and data-anchor", () => {
    const blocks = topLevelChildren("# Top\n## Middle\n### Low");
    expect(blocks).toHaveLength(3);
    expect(blocks[0]?.type).toBe("h1");
    expect((blocks[0]?.props as { id: string }).id).toBe("top");
    expect(
      (blocks[0]?.props as { "data-anchor": string })["data-anchor"],
    ).toBe("top");
    expect(blocks[1]?.type).toBe("h2");
    expect((blocks[1]?.props as { id: string }).id).toBe("middle");
    expect(blocks[2]?.type).toBe("h3");
    expect((blocks[2]?.props as { id: string }).id).toBe("low");
  });

  it("groups consecutive unordered items into a single <ul>", () => {
    const blocks = topLevelChildren("- one\n- two\n- three");
    expect(blocks).toHaveLength(1);
    const ul = blocks[0];
    expect(ul?.type).toBe("ul");
    const items = flatten((ul?.props as { children: ReactNode }).children);
    expect(items).toHaveLength(3);
    expect(items.every((i) => i.type === "li")).toBe(true);
    expect(items.map((i) => textContent(i))).toEqual(["one", "two", "three"]);
  });

  it("groups consecutive ordered items into a single <ol>", () => {
    const blocks = topLevelChildren("1. first\n2. second");
    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.type).toBe("ol");
  });

  it("renders a task list with checkbox inputs", () => {
    const blocks = topLevelChildren("- [x] done\n- [ ] pending");
    expect(blocks[0]?.type).toBe("ul");
    const items = flatten(
      (blocks[0]?.props as { children: ReactNode }).children,
    );
    expect(items).toHaveLength(2);
    const [first, second] = items;
    const firstChildren = flatten(
      (first?.props as { children: ReactNode }).children,
    );
    const firstBox = firstChildren.find((c) => c.type === "input");
    expect(firstBox).toBeDefined();
    expect((firstBox?.props as { checked: boolean }).checked).toBe(true);
    const secondChildren = flatten(
      (second?.props as { children: ReactNode }).children,
    );
    const secondBox = secondChildren.find((c) => c.type === "input");
    expect((secondBox?.props as { checked: boolean }).checked).toBe(false);
  });

  it("renders a code block as <pre><code> with optional language", () => {
    const blocks = topLevelChildren("```ts\nconst x = 1;\n```");
    expect(blocks).toHaveLength(1);
    const pre = blocks[0];
    expect(pre?.type).toBe("pre");
    expect((pre?.props as { "data-lang": string })["data-lang"]).toBe("ts");
    expect(textContent(pre)).toBe("const x = 1;");
  });

  it("renders a blockquote", () => {
    const blocks = topLevelChildren("> a quoted line");
    expect(blocks[0]?.type).toBe("blockquote");
    expect(textContent(blocks[0])).toBe("a quoted line");
  });

  it("renders a horizontal rule", () => {
    const blocks = topLevelChildren("before\n\n---\n\nafter");
    const hrs = blocks.filter((b) => b.type === "hr");
    expect(hrs).toHaveLength(1);
  });

  it("renders inline bold, italic and code", () => {
    const [p] = topLevelChildren("A **bold** and *italic* and `mono` word.");
    expect(p?.type).toBe("p");
    const children = flatten((p?.props as { children: ReactNode }).children);
    const types = children.map((c) => c.type);
    expect(types).toContain("strong");
    expect(types).toContain("em");
    expect(types).toContain("code");
  });

  it("renders an inline link with safe target/rel", () => {
    const [p] = topLevelChildren("See [docs](https://example.com) for more.");
    const children = flatten((p?.props as { children: ReactNode }).children);
    const anchor = children.find((c) => c.type === "a");
    expect(anchor).toBeDefined();
    const props = anchor?.props as {
      href: string;
      target: string;
      rel: string;
    };
    expect(props.href).toBe("https://example.com");
    expect(props.target).toBe("_blank");
    expect(props.rel).toBe("noopener noreferrer");
  });

  it("renders a wiki link with data-wiki-id and display text", () => {
    const [p] = topLevelChildren("See [[task-42|the task]] now.");
    const children = flatten((p?.props as { children: ReactNode }).children);
    const wiki = children.find(
      (c) => typeof c.type === "string" && c.type === "span",
    );
    expect(wiki).toBeDefined();
    expect(
      (wiki?.props as { "data-wiki-id": string })["data-wiki-id"],
    ).toBe("task-42");
    expect(textContent(wiki)).toBe("the task");
  });

  it("skips empty blocks and renders mixed content in order", () => {
    const blocks = topLevelChildren("# Title\n\npara one\n\n- a\n- b\n\npara two");
    const types = blocks.map((b) => b.type);
    expect(types).toEqual(["h1", "p", "ul", "p"]);
  });
});
