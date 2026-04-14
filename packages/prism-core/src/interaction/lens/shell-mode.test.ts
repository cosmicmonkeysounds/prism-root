/**
 * Shell-mode & permission — pure helpers.
 *
 * Covers the three axes of the mode/permission system that don't touch
 * React, Puck, or any kernel wiring:
 *
 *   1. Runtime type guards (`isShellMode`, `isPermission`).
 *   2. Permission rank comparison (`permissionAtLeast`).
 *   3. The lens-bundle context filter (`lensBundleMatchesShellContext`,
 *      `filterLensBundlesByShellMode`, `withShellModes`).
 *   4. BootConfig defaulting (`resolveBootConfig`).
 *
 * Kernel integration tests (setShellMode / getVisibleLensIds) live
 * alongside `studio-kernel.ts` because they need the real kernel.
 */

import { describe, expect, it } from "vitest";
import {
  DEFAULT_AVAILABLE_IN_MODES,
  DEFAULT_BOOT_CONFIG,
  DEFAULT_MIN_PERMISSION,
  SHELL_MODES,
  PERMISSIONS,
  PERMISSION_RANK,
  filterLensBundlesByShellMode,
  isPermission,
  isShellMode,
  lensBundleMatchesShellContext,
  permissionAtLeast,
  resolveBootConfig,
  withShellModes,
  type LensBundle,
  type Permission,
  type ShellMode,
} from "./index.js";

// ── Type guards ────────────────────────────────────────────────────────

describe("isShellMode", () => {
  it("accepts the three canonical modes", () => {
    expect(isShellMode("use")).toBe(true);
    expect(isShellMode("build")).toBe(true);
    expect(isShellMode("admin")).toBe(true);
  });

  it("rejects anything else", () => {
    expect(isShellMode("USE")).toBe(false);
    expect(isShellMode("edit")).toBe(false);
    expect(isShellMode(undefined)).toBe(false);
    expect(isShellMode(null)).toBe(false);
    expect(isShellMode(0)).toBe(false);
    expect(isShellMode({})).toBe(false);
  });
});

describe("isPermission", () => {
  it("accepts the two canonical tiers", () => {
    expect(isPermission("user")).toBe(true);
    expect(isPermission("dev")).toBe(true);
  });

  it("rejects anything else", () => {
    expect(isPermission("root")).toBe(false);
    expect(isPermission("admin")).toBe(false);
    expect(isPermission(undefined)).toBe(false);
  });
});

describe("SHELL_MODES / PERMISSIONS constants", () => {
  it("lists shell modes in stable UI order", () => {
    expect([...SHELL_MODES]).toEqual(["use", "build", "admin"]);
  });

  it("lists permissions in ascending privilege order", () => {
    expect([...PERMISSIONS]).toEqual(["user", "dev"]);
  });

  it("ranks dev strictly above user", () => {
    expect(PERMISSION_RANK.dev).toBeGreaterThan(PERMISSION_RANK.user);
  });
});

// ── Permission comparison ──────────────────────────────────────────────

describe("permissionAtLeast", () => {
  it("treats dev as strictly more privileged than user", () => {
    expect(permissionAtLeast("dev", "user")).toBe(true);
    expect(permissionAtLeast("user", "dev")).toBe(false);
  });

  it("is reflexive for both tiers", () => {
    expect(permissionAtLeast("user", "user")).toBe(true);
    expect(permissionAtLeast("dev", "dev")).toBe(true);
  });
});

// ── Lens-bundle context filter ─────────────────────────────────────────

function makeBundle(
  id: string,
  constraints?: {
    availableInModes?: readonly ShellMode[];
    minPermission?: Permission;
  },
): LensBundle<string> {
  return {
    id,
    name: id,
    manifest: {
      id,
      name: id,
      kind: "panel",
      activityBar: { icon: "", label: id },
    },
    component: id,
    install: () => () => {},
    ...(constraints?.availableInModes !== undefined
      ? { availableInModes: constraints.availableInModes }
      : {}),
    ...(constraints?.minPermission !== undefined
      ? { minPermission: constraints.minPermission }
      : {}),
  };
}

describe("lensBundleMatchesShellContext defaults", () => {
  it("admits a constraint-free bundle in build+admin at user tier", () => {
    expect(
      lensBundleMatchesShellContext(undefined, {
        mode: "build",
        permission: "user",
      }),
    ).toBe(true);
    expect(
      lensBundleMatchesShellContext(undefined, {
        mode: "admin",
        permission: "user",
      }),
    ).toBe(true);
  });

  it("hides a constraint-free bundle in use mode (not in defaults)", () => {
    expect(
      lensBundleMatchesShellContext(undefined, {
        mode: "use",
        permission: "dev",
      }),
    ).toBe(false);
  });

  it("advertises the stable defaults", () => {
    expect([...DEFAULT_AVAILABLE_IN_MODES]).toEqual(["build", "admin"]);
    expect(DEFAULT_MIN_PERMISSION).toBe("user");
  });
});

describe("lensBundleMatchesShellContext constraints", () => {
  it("hides a bundle when its modes exclude the current mode", () => {
    expect(
      lensBundleMatchesShellContext(
        { availableInModes: ["admin"] },
        { mode: "build", permission: "dev" },
      ),
    ).toBe(false);
  });

  it("hides a bundle when its minPermission exceeds the caller", () => {
    expect(
      lensBundleMatchesShellContext(
        { minPermission: "dev" },
        { mode: "admin", permission: "user" },
      ),
    ).toBe(false);
  });

  it("admits a use-mode bundle only when explicitly opted in", () => {
    expect(
      lensBundleMatchesShellContext(
        { availableInModes: ["use", "build", "admin"] },
        { mode: "use", permission: "user" },
      ),
    ).toBe(true);
  });
});

describe("filterLensBundlesByShellMode", () => {
  const bundles: LensBundle<string>[] = [
    makeBundle("canvas", {
      availableInModes: ["use", "build", "admin"],
      minPermission: "user",
    }),
    makeBundle("editor"), // defaults: build+admin, user
    makeBundle("graph", {
      availableInModes: ["admin"],
      minPermission: "dev",
    }),
    makeBundle("settings"),
  ];

  it("use/user keeps only bundles that opted in to use mode", () => {
    const visible = filterLensBundlesByShellMode(bundles, {
      mode: "use",
      permission: "user",
    }).map((b) => b.id);
    expect(visible).toEqual(["canvas"]);
  });

  it("build/user keeps user-reachable authoring bundles", () => {
    const visible = filterLensBundlesByShellMode(bundles, {
      mode: "build",
      permission: "user",
    }).map((b) => b.id);
    expect(visible).toEqual(["canvas", "editor", "settings"]);
  });

  it("admin/dev exposes everything that lists admin", () => {
    const visible = filterLensBundlesByShellMode(bundles, {
      mode: "admin",
      permission: "dev",
    }).map((b) => b.id);
    expect(visible).toEqual(["canvas", "editor", "graph", "settings"]);
  });

  it("admin/user hides dev-only bundles", () => {
    const visible = filterLensBundlesByShellMode(bundles, {
      mode: "admin",
      permission: "user",
    }).map((b) => b.id);
    expect(visible).toEqual(["canvas", "editor", "settings"]);
  });
});

describe("withShellModes", () => {
  it("attaches a constraint to an existing bundle without mutating it", () => {
    const base = makeBundle("editor");
    const extended = withShellModes(base, {
      availableInModes: ["use", "build", "admin"],
      minPermission: "user",
    });
    expect(extended.availableInModes).toEqual(["use", "build", "admin"]);
    expect(extended.minPermission).toBe("user");
    // Original bundle is untouched.
    expect(base.availableInModes).toBeUndefined();
    expect(base.minPermission).toBeUndefined();
  });

  it("leaves previously-set fields alone when a field is undefined", () => {
    const base = withShellModes(makeBundle("graph"), {
      availableInModes: ["admin"],
      minPermission: "dev",
    });
    // Partial update: only override availableInModes.
    const override = withShellModes(base, {
      availableInModes: ["build", "admin"],
    });
    expect(override.availableInModes).toEqual(["build", "admin"]);
    expect(override.minPermission).toBe("dev");
  });
});

// ── BootConfig defaulting ──────────────────────────────────────────────

describe("resolveBootConfig", () => {
  it("falls back to the full-IDE default when the input is undefined", () => {
    expect(resolveBootConfig(undefined)).toEqual(DEFAULT_BOOT_CONFIG);
  });

  it("fills in every unset field", () => {
    const resolved = resolveBootConfig({ shellMode: "use" });
    expect(resolved.shellMode).toBe("use");
    expect(resolved.permission).toBe("dev");
    expect(resolved.profile).toBeNull();
    expect(resolved.daemonTransport).toBe("auto");
    expect(resolved.launcher).toEqual({});
  });

  it("carries the profile through when provided", () => {
    const resolved = resolveBootConfig({ profile: "flux" });
    expect(resolved.profile).toBe("flux");
  });

  it("respects an explicit user permission", () => {
    const resolved = resolveBootConfig({ permission: "user" });
    expect(resolved.permission).toBe("user");
  });
});
