/**
 * Boot config resolver — unit tests.
 *
 * The resolver supports an `overrides` bag for the three runtime sources
 * (query / env / buildTime) so tests never have to touch `window` or
 * `import.meta.env`. Each test drives the merge + clamp logic directly.
 */

import { describe, expect, it } from "vitest";
import { DEFAULT_BOOT_CONFIG } from "@prism/core/lens";
import { loadBootConfig } from "./load-boot-config.js";

describe("loadBootConfig — precedence", () => {
  it("falls back to DEFAULT_BOOT_CONFIG when every source is null", () => {
    const resolved = loadBootConfig({ query: null, env: null, buildTime: null });
    expect(resolved).toEqual(DEFAULT_BOOT_CONFIG);
  });

  it("uses env boot config when present and no query override", () => {
    const resolved = loadBootConfig({
      query: null,
      env: { shellMode: "use", permission: "user" },
      buildTime: null,
    });
    expect(resolved.shellMode).toBe("use");
    expect(resolved.permission).toBe("user");
  });

  it("query params override env", () => {
    const resolved = loadBootConfig({
      query: { shellMode: "build" },
      env: { shellMode: "use", permission: "user" },
      buildTime: null,
    });
    expect(resolved.shellMode).toBe("build");
    // Env's permission still comes through when query doesn't touch it.
    expect(resolved.permission).toBe("user");
  });

  it("env overrides buildTime, query overrides env", () => {
    const resolved = loadBootConfig({
      query: { shellMode: "admin" },
      env: { shellMode: "build", profile: "flux" },
      buildTime: { shellMode: "use", permission: "dev" },
    });
    expect(resolved.shellMode).toBe("admin");
    expect(resolved.profile).toBe("flux");
    expect(resolved.permission).toBe("dev");
  });
});

describe("loadBootConfig — build-time ceiling", () => {
  it("clamps query permission escalation to dev when ceiling is user", () => {
    const resolved = loadBootConfig({
      query: { permission: "dev" },
      env: null,
      buildTime: { permission: "user" },
    });
    expect(resolved.permission).toBe("user");
  });

  it("clamps env permission escalation to dev when ceiling is user", () => {
    const resolved = loadBootConfig({
      query: null,
      env: { permission: "dev", shellMode: "admin" },
      buildTime: { permission: "user" },
    });
    expect(resolved.permission).toBe("user");
    // shellMode is not affected by the permission clamp.
    expect(resolved.shellMode).toBe("admin");
  });

  it("allows user permission when ceiling is user", () => {
    const resolved = loadBootConfig({
      query: { permission: "user" },
      env: null,
      buildTime: { permission: "user" },
    });
    expect(resolved.permission).toBe("user");
  });

  it("allows dev permission when ceiling has no permission set", () => {
    const resolved = loadBootConfig({
      query: { permission: "dev" },
      env: null,
      buildTime: { shellMode: "admin" },
    });
    expect(resolved.permission).toBe("dev");
  });

  it("allows dev permission when ceiling is null (Tauri/desktop)", () => {
    const resolved = loadBootConfig({
      query: { permission: "dev" },
      env: null,
      buildTime: null,
    });
    expect(resolved.permission).toBe("dev");
  });
});

describe("loadBootConfig — field fill-in", () => {
  it("carries profile through from any source", () => {
    const resolved = loadBootConfig({
      query: null,
      env: { profile: "lattice" },
      buildTime: null,
    });
    expect(resolved.profile).toBe("lattice");
  });

  it("fills every missing field via resolveBootConfig()", () => {
    const resolved = loadBootConfig({
      query: { shellMode: "use" },
      env: null,
      buildTime: null,
    });
    expect(resolved.shellMode).toBe("use");
    // Defaults from resolveBootConfig.
    expect(resolved.permission).toBe("dev");
    expect(resolved.profile).toBeNull();
    expect(resolved.daemonTransport).toBe("auto");
    expect(resolved.launcher).toEqual({});
  });

  it("preserves launcher metadata from env", () => {
    const resolved = loadBootConfig({
      query: null,
      env: {
        shellMode: "use",
        permission: "user",
        launcher: { name: "prism-studio", startedAt: "2026-04-14T00:00:00Z" },
      },
      buildTime: null,
    });
    expect(resolved.launcher).toEqual({
      name: "prism-studio",
      startedAt: "2026-04-14T00:00:00Z",
    });
  });
});
