/**
 * Pure-function tests for media renderer helpers.
 */

import { describe, it, expect } from "vitest";
import { isSafeMediaUrl, clampPx } from "./media-renderers.js";

describe("isSafeMediaUrl", () => {
  it("accepts http and https URLs", () => {
    expect(isSafeMediaUrl("https://example.com/video.mp4")).toBe(true);
    expect(isSafeMediaUrl("http://example.com/audio.mp3")).toBe(true);
  });

  it("accepts vfs:// URLs pointing at VFS-stored binaries", () => {
    expect(isSafeMediaUrl("vfs://abc123")).toBe(true);
    expect(isSafeMediaUrl("  vfs://abc123  ")).toBe(true);
  });

  it("rejects unsafe schemes", () => {
    expect(isSafeMediaUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeMediaUrl("data:video/mp4;base64,AAA")).toBe(false);
    expect(isSafeMediaUrl("file:///etc/passwd")).toBe(false);
  });

  it("rejects blank, null, or non-string values", () => {
    expect(isSafeMediaUrl("")).toBe(false);
    expect(isSafeMediaUrl("   ")).toBe(false);
    expect(isSafeMediaUrl(null)).toBe(false);
    expect(isSafeMediaUrl(undefined)).toBe(false);
  });
});

describe("clampPx", () => {
  it("returns the fallback for non-finite inputs", () => {
    expect(clampPx(undefined, 10, 100, 42)).toBe(42);
    expect(clampPx(NaN, 10, 100, 42)).toBe(42);
    expect(clampPx("nope", 10, 100, 42)).toBe(42);
  });

  it("clamps below the min", () => {
    expect(clampPx(5, 10, 100, 42)).toBe(10);
  });

  it("clamps above the max", () => {
    expect(clampPx(500, 10, 100, 42)).toBe(100);
  });

  it("floors fractional values inside the range", () => {
    expect(clampPx(33.9, 10, 100, 42)).toBe(33);
  });

  it("accepts numeric strings", () => {
    expect(clampPx("50", 10, 100, 42)).toBe(50);
  });
});
