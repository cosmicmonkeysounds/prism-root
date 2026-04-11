import { describe, it, expect } from "vitest";
import { LoroDoc } from "loro-crdt";
import {
  createTextFile,
  createGraphFile,
  createBinaryFile,
  isTextBody,
  isGraphBody,
  isBinaryBody,
  type FileBody,
  type PrismFile,
} from "./prism-file.js";
import type { GraphObject } from "@prism/core/object-model";
import type { BinaryRef } from "@prism/core/vfs";

describe("PrismFile", () => {
  describe("createTextFile", () => {
    it("builds a text file from a plain string", () => {
      const file = createTextFile({
        path: "/notes/hello.md",
        text: "# Hello",
        languageId: "prism:markdown",
      });

      expect(file.path).toBe("/notes/hello.md");
      expect(file.languageId).toBe("prism:markdown");
      expect(file.body.kind).toBe("text");
      if (file.body.kind === "text") {
        expect(file.body.ref).toBe("# Hello");
      }
    });

    it("accepts a LoroText reference", () => {
      const doc = new LoroDoc();
      const text = doc.getText("body");
      text.insert(0, "hello world");

      const file = createTextFile({
        path: "/doc/readme.md",
        text,
        languageId: "prism:markdown",
      });

      expect(file.body.kind).toBe("text");
      if (file.body.kind === "text" && typeof file.body.ref !== "string") {
        expect(file.body.ref.toString()).toBe("hello world");
      }
    });

    it("carries schema and metadata through", () => {
      const file = createTextFile({
        path: "/config.yaml",
        text: "enabled: true",
        languageId: "prism:yaml",
        schema: {
          id: "app-config",
          name: "App Config",
          fields: [],
          sections: [],
        },
        metadata: { owner: "did:key:abc" },
      });

      expect(file.schema?.id).toBe("app-config");
      expect(file.metadata).toEqual({ owner: "did:key:abc" });
    });
  });

  describe("createGraphFile", () => {
    it("builds a graph file", () => {
      const object = {
        id: "obj:task:1",
        type: "task",
        fields: { title: "Ship Phase 1" },
      } as unknown as GraphObject;

      const file = createGraphFile({
        path: "obj:task:1",
        object,
        languageId: "prism:flux-task",
      });

      expect(file.body.kind).toBe("graph");
      if (file.body.kind === "graph") {
        expect(file.body.ref).toBe(object);
      }
    });
  });

  describe("createBinaryFile", () => {
    it("builds a binary file from a BinaryRef", () => {
      const ref: BinaryRef = {
        hash: "abcdef1234",
        filename: "logo.png",
        mimeType: "image/png",
        size: 1024,
        importedAt: "2026-04-11T00:00:00Z",
      };

      const file = createBinaryFile({
        path: "/assets/logo.png",
        ref,
      });

      expect(file.body.kind).toBe("binary");
      if (file.body.kind === "binary") {
        expect(file.body.ref.hash).toBe("abcdef1234");
      }
    });
  });

  describe("narrowing helpers", () => {
    const textBody: FileBody = { kind: "text", ref: "abc" };
    const graphBody: FileBody = {
      kind: "graph",
      ref: { id: "obj:1" } as unknown as GraphObject,
    };
    const binaryBody: FileBody = {
      kind: "binary",
      ref: {
        hash: "h",
        filename: "f",
        mimeType: "application/octet-stream",
        size: 0,
        importedAt: "2026-04-11T00:00:00Z",
      },
    };

    it("isTextBody narrows to text variant", () => {
      expect(isTextBody(textBody)).toBe(true);
      expect(isTextBody(graphBody)).toBe(false);
      expect(isTextBody(binaryBody)).toBe(false);
    });

    it("isGraphBody narrows to graph variant", () => {
      expect(isGraphBody(graphBody)).toBe(true);
      expect(isGraphBody(textBody)).toBe(false);
      expect(isGraphBody(binaryBody)).toBe(false);
    });

    it("isBinaryBody narrows to binary variant", () => {
      expect(isBinaryBody(binaryBody)).toBe(true);
      expect(isBinaryBody(textBody)).toBe(false);
      expect(isBinaryBody(graphBody)).toBe(false);
    });

    it("discriminated union covers every case exhaustively", () => {
      const describe = (body: FileBody): string => {
        switch (body.kind) {
          case "text":
            return "text";
          case "graph":
            return "graph";
          case "binary":
            return "binary";
        }
      };
      expect(describe(textBody)).toBe("text");
      expect(describe(graphBody)).toBe("graph");
      expect(describe(binaryBody)).toBe("binary");
    });
  });

  it("typechecks the PrismFile shape end-to-end", () => {
    const file: PrismFile = {
      path: "/tmp/example.md",
      languageId: "prism:markdown",
      body: { kind: "text", ref: "body" },
    };
    expect(file.path).toBe("/tmp/example.md");
  });
});
