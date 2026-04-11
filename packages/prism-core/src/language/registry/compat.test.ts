import { describe, it, expect } from "vitest";
import {
  LanguageRegistry,
  DocumentSurfaceRegistry,
  MARKDOWN_CONTRIBUTION,
  type LanguageDefinition,
  type DocumentContributionDef,
  type RootNode,
  type ProcessorContext,
} from "@prism/core/syntax";
import {
  contributionFromLegacy,
  resolveContribution,
} from "./compat.js";

// ── Test fixtures ─────────────────────────────────────────────────────────

function makeRootNode(text: string): RootNode {
  return {
    type: "root",
    position: {
      start: { offset: 0, line: 1, column: 0 },
      end: { offset: text.length, line: 1, column: text.length },
    },
    children: [],
    data: { raw: text },
  };
}

function makeLuauLanguageDef(): LanguageDefinition {
  return {
    id: "luau",
    extensions: [".luau", ".lua"],
    mimeTypes: ["text/x-luau"],
    parse(source: string, _ctx: ProcessorContext): RootNode {
      return makeRootNode(source);
    },
    serialize(tree: RootNode): string {
      const data = (tree as unknown as { data?: { raw?: string } }).data;
      return data?.raw ?? "";
    },
  };
}

const LUAU_CONTRIBUTION: DocumentContributionDef = {
  id: "prism:luau",
  extensions: [".luau", ".lua"],
  displayName: "Luau",
  defaultMode: "code",
  availableModes: ["code"],
  mimeType: "text/x-luau",
};

// ── contributionFromLegacy ────────────────────────────────────────────────

describe("contributionFromLegacy", () => {
  it("bridges a language + surface pair into a unified contribution", () => {
    const lang = makeLuauLanguageDef();
    const contribution = contributionFromLegacy(lang, LUAU_CONTRIBUTION);

    expect(contribution.id).toBe("prism:luau");
    expect(contribution.extensions).toEqual([".luau", ".lua"]);
    expect(contribution.displayName).toBe("Luau");
    expect(contribution.mimeType).toBe("text/x-luau");
    expect(contribution.surface.defaultMode).toBe("code");
    expect(contribution.surface.availableModes).toEqual(["code"]);
    expect(typeof contribution.parse).toBe("function");
    expect(typeof contribution.serialize).toBe("function");
  });

  it("parse round-trips through the legacy LanguageDefinition", () => {
    const lang = makeLuauLanguageDef();
    const contribution = contributionFromLegacy(lang, LUAU_CONTRIBUTION);

    const ast = contribution.parse?.("print('hi')");
    expect(ast).toBeDefined();
    const serialized = contribution.serialize?.(ast as RootNode);
    expect(serialized).toBe("print('hi')");
  });

  it("falls back to a code-only surface when surface is missing", () => {
    const lang = makeLuauLanguageDef();
    const contribution = contributionFromLegacy(lang, null);

    expect(contribution.id).toBe("luau");
    expect(contribution.surface.defaultMode).toBe("code");
    expect(contribution.surface.availableModes).toEqual(["code"]);
    expect(contribution.surface.inlineTokens).toBeUndefined();
  });

  it("omits parse/serialize when language is missing", () => {
    const contribution = contributionFromLegacy(null, MARKDOWN_CONTRIBUTION);

    expect(contribution.id).toBe("prism:markdown");
    expect(contribution.extensions).toEqual([".md", ".mdx", ".markdown"]);
    expect(contribution.surface.defaultMode).toBe("preview");
    expect(contribution.parse).toBeUndefined();
    expect(contribution.serialize).toBeUndefined();
  });

  it("carries surface inline tokens through", () => {
    const withTokens: DocumentContributionDef = {
      ...MARKDOWN_CONTRIBUTION,
      inlineTokens: [
        {
          id: "wikilink",
          pattern: /\[\[(.+?)\]\]/g,
          extract: (m) => ({ display: m[1] ?? "" }),
        },
      ],
    };
    const contribution = contributionFromLegacy(null, withTokens);
    expect(contribution.surface.inlineTokens).toHaveLength(1);
    expect(contribution.surface.inlineTokens?.[0]?.id).toBe("wikilink");
  });

  it("throws when both inputs are null", () => {
    expect(() => contributionFromLegacy(null, null)).toThrow(
      /at least one of language or surface/,
    );
  });
});

// ── resolveContribution ───────────────────────────────────────────────────

describe("resolveContribution", () => {
  function makeRegistries(): {
    languages: LanguageRegistry;
    surfaces: DocumentSurfaceRegistry;
  } {
    const languages = new LanguageRegistry();
    languages.register(makeLuauLanguageDef());

    const surfaces = new DocumentSurfaceRegistry();
    surfaces.register({ contribution: LUAU_CONTRIBUTION });
    surfaces.register({ contribution: MARKDOWN_CONTRIBUTION });

    return { languages, surfaces };
  }

  it("resolves by filename extension", () => {
    const { languages, surfaces } = makeRegistries();
    const contribution = resolveContribution({
      languages,
      surfaces,
      filename: "script.luau",
    });

    expect(contribution).not.toBeNull();
    expect(contribution?.id).toBe("prism:luau");
    expect(contribution?.parse).toBeTypeOf("function");
  });

  it("resolves Markdown despite having no registered LanguageDefinition", () => {
    const { languages, surfaces } = makeRegistries();
    const contribution = resolveContribution({
      languages,
      surfaces,
      filename: "readme.md",
    });

    expect(contribution).not.toBeNull();
    expect(contribution?.id).toBe("prism:markdown");
    expect(contribution?.surface.defaultMode).toBe("preview");
    // No LanguageDefinition → no parse. Bridge stays honest.
    expect(contribution?.parse).toBeUndefined();
  });

  it("prefers explicit documentType over filename", () => {
    const { languages, surfaces } = makeRegistries();
    const contribution = resolveContribution({
      languages,
      surfaces,
      filename: "readme.md",
      documentType: "prism:luau",
    });

    expect(contribution?.id).toBe("prism:luau");
  });

  it("returns null when nothing resolves", () => {
    const { languages, surfaces } = makeRegistries();
    const contribution = resolveContribution({
      languages,
      surfaces,
      filename: "mystery.xyz",
      documentType: undefined,
    });

    expect(contribution).toBeNull();
  });
});
