/**
 * Unit tests for the vfs:// media URL helpers.
 *
 * Pure helpers are covered synchronously; the async resolver is exercised
 * against a real `createVfsManager` + memory adapter (same pair the Studio
 * kernel uses) so we verify the full import → stat → export → blob URL path.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createVfsManager } from "@prism/core/vfs";
import {
  VFS_SCHEME,
  isVfsMediaUrl,
  parseVfsHash,
  buildVfsMediaUrl,
  isBrowserMediaUrl,
  resolveVfsMediaUrl,
  __resetVfsMediaCache,
} from "./vfs-media-url.js";

describe("vfs-media-url — pure helpers", () => {
  it("recognises vfs:// URLs", () => {
    expect(isVfsMediaUrl("vfs://abc123")).toBe(true);
    expect(isVfsMediaUrl("https://example.com/a.png")).toBe(false);
    expect(isVfsMediaUrl("vfs://")).toBe(false); // empty hash
    expect(isVfsMediaUrl(null)).toBe(false);
    expect(isVfsMediaUrl(undefined)).toBe(false);
    expect(isVfsMediaUrl(42)).toBe(false);
  });

  it("parses the hash out of a vfs:// URL", () => {
    expect(parseVfsHash("vfs://deadbeef")).toBe("deadbeef");
    expect(parseVfsHash("vfs://")).toBeNull();
    expect(parseVfsHash("https://example.com")).toBeNull();
    expect(parseVfsHash(null)).toBeNull();
  });

  it("builds canonical vfs:// URLs from a hash", () => {
    expect(buildVfsMediaUrl("abc")).toBe("vfs://abc");
    expect(buildVfsMediaUrl("abc").startsWith(VFS_SCHEME)).toBe(true);
  });

  it("treats http(s), data, and blob URLs as browser-ready", () => {
    expect(isBrowserMediaUrl("https://example.com/a.png")).toBe(true);
    expect(isBrowserMediaUrl("http://example.com/a.png")).toBe(true);
    expect(isBrowserMediaUrl("data:image/png;base64,AA==")).toBe(true);
    expect(isBrowserMediaUrl("blob:http://example.com/abc")).toBe(true);
    expect(isBrowserMediaUrl("vfs://deadbeef")).toBe(false);
    expect(isBrowserMediaUrl("javascript:alert(1)")).toBe(false);
    expect(isBrowserMediaUrl("")).toBe(false);
  });
});

describe("vfs-media-url — resolveVfsMediaUrl", () => {
  beforeEach(() => {
    __resetVfsMediaCache();
    // jsdom provides URL.createObjectURL; provide a stub for the happy-dom
    // / node env if it's missing.
    if (typeof URL.createObjectURL !== "function") {
      (URL as unknown as { createObjectURL: (b: Blob) => string }).createObjectURL = (() => {
        let n = 0;
        return () => `blob:test-${n++}`;
      })();
      (URL as unknown as { revokeObjectURL: (u: string) => void }).revokeObjectURL = () => {};
    }
  });

  afterEach(() => {
    __resetVfsMediaCache();
  });

  it("passes http(s) URLs straight through without touching the vfs", async () => {
    const vfs = createVfsManager();
    const statSpy = vi.spyOn(vfs, "stat");
    const result = await resolveVfsMediaUrl("https://example.com/a.png", vfs);
    expect(result).toBe("https://example.com/a.png");
    expect(statSpy).not.toHaveBeenCalled();
  });

  it("returns null for empty / non-string / unsupported schemes", async () => {
    const vfs = createVfsManager();
    expect(await resolveVfsMediaUrl(null, vfs)).toBeNull();
    expect(await resolveVfsMediaUrl(undefined, vfs)).toBeNull();
    expect(await resolveVfsMediaUrl("", vfs)).toBeNull();
    expect(await resolveVfsMediaUrl("javascript:alert(1)", vfs)).toBeNull();
  });

  it("resolves a vfs:// URL to a blob URL via vfs.exportFile", async () => {
    const vfs = createVfsManager();
    const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]); // PNG header
    const ref = await vfs.importFile(bytes, "tiny.png", "image/png");
    const url = await resolveVfsMediaUrl(buildVfsMediaUrl(ref.hash), vfs);
    expect(url).toMatch(/^blob:/);
  });

  it("caches blob URLs by hash — second call returns the same URL without re-reading", async () => {
    const vfs = createVfsManager();
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const ref = await vfs.importFile(bytes, "x.bin", "application/octet-stream");
    const statSpy = vi.spyOn(vfs, "stat");
    const url1 = await resolveVfsMediaUrl(buildVfsMediaUrl(ref.hash), vfs);
    const url2 = await resolveVfsMediaUrl(buildVfsMediaUrl(ref.hash), vfs);
    expect(url1).not.toBeNull();
    expect(url1).toBe(url2);
    // Second call hits the cache — only one stat() round-trip.
    expect(statSpy).toHaveBeenCalledTimes(1);
  });

  it("returns null when the hash is unknown to the vfs", async () => {
    const vfs = createVfsManager();
    const url = await resolveVfsMediaUrl(buildVfsMediaUrl("not-a-real-hash"), vfs);
    expect(url).toBeNull();
  });
});
