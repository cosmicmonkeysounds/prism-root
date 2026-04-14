/**
 * prism-studio launcher — pure-helper unit tests.
 *
 * The launcher itself is a `.mjs` Node script under `bin/` so `npx
 * prism-studio` can run it directly without a build step. Only its
 * exported pure helpers are tested here — the child-process spawn is
 * left to a manual smoke test because it would need a real Vite
 * install + an attached TTY to behave.
 */

import { describe, expect, it } from "vitest";
// @ts-expect-error — bin entry is a plain .mjs, no ambient types.
import * as launcherModule from "../../bin/prism-studio.mjs";

interface LauncherShape {
  readonly buildBootConfigForSubcommand: (
    subcommand: string,
    argv: readonly string[],
  ) => Record<string, unknown> | null;
  readonly buildChildEnv: (
    baseEnv: Record<string, string | undefined>,
    bootConfig: Record<string, unknown> | null,
  ) => Record<string, string | undefined>;
  readonly extractProfileFlag: (argv: readonly string[]) => string | undefined;
  readonly stripProfileFlag: (argv: readonly string[]) => string[];
  readonly viteArgsForSubcommand: (
    subcommand: string,
    forwarded: readonly string[],
  ) => string[];
}

const {
  buildBootConfigForSubcommand,
  buildChildEnv,
  extractProfileFlag,
  stripProfileFlag,
  viteArgsForSubcommand,
} = launcherModule as unknown as LauncherShape;

describe("extractProfileFlag", () => {
  it("reads --profile=<value>", () => {
    expect(extractProfileFlag(["--profile=flux"])).toBe("flux");
  });

  it("reads --profile <value> (space form)", () => {
    expect(extractProfileFlag(["--profile", "lattice"])).toBe("lattice");
  });

  it("returns undefined when no profile flag is present", () => {
    expect(extractProfileFlag(["--foo", "--bar"])).toBeUndefined();
  });

  it("returns undefined for dangling --profile", () => {
    expect(extractProfileFlag(["--profile"])).toBeUndefined();
  });

  it("returns undefined for empty --profile=", () => {
    expect(extractProfileFlag(["--profile="])).toBeUndefined();
  });

  it("returns the first match when multiple profile flags are passed", () => {
    expect(extractProfileFlag(["--profile=flux", "--profile=lattice"])).toBe(
      "flux",
    );
  });
});

describe("stripProfileFlag", () => {
  it("strips --profile=<value>", () => {
    expect(stripProfileFlag(["--foo", "--profile=flux", "--bar"])).toEqual([
      "--foo",
      "--bar",
    ]);
  });

  it("strips --profile <value>", () => {
    expect(stripProfileFlag(["--profile", "flux", "--bar"])).toEqual(["--bar"]);
  });

  it("leaves unrelated args untouched", () => {
    expect(stripProfileFlag(["--port", "1234", "--open"])).toEqual([
      "--port",
      "1234",
      "--open",
    ]);
  });
});

describe("buildBootConfigForSubcommand", () => {
  it("returns the use/user baseline for run", () => {
    const cfg = buildBootConfigForSubcommand("run", []);
    expect(cfg).not.toBeNull();
    expect(cfg?.["shellMode"]).toBe("use");
    expect(cfg?.["permission"]).toBe("user");
    const launcherField = cfg?.["launcher"] as { name?: string } | undefined;
    expect(launcherField?.name).toBe("prism-studio");
  });

  it("returns the build/dev baseline for build", () => {
    const cfg = buildBootConfigForSubcommand("build", []);
    expect(cfg?.["shellMode"]).toBe("build");
    expect(cfg?.["permission"]).toBe("dev");
  });

  it("returns the admin/dev baseline for admin", () => {
    const cfg = buildBootConfigForSubcommand("admin", []);
    expect(cfg?.["shellMode"]).toBe("admin");
    expect(cfg?.["permission"]).toBe("dev");
  });

  it("returns null for dev (no boot override)", () => {
    expect(buildBootConfigForSubcommand("dev", [])).toBeNull();
  });

  it("returns null for bundle (production build, no override)", () => {
    expect(buildBootConfigForSubcommand("bundle", [])).toBeNull();
  });

  it("folds --profile into the boot config", () => {
    const cfg = buildBootConfigForSubcommand("run", ["--profile=flux"]);
    expect(cfg?.["profile"]).toBe("flux");
  });

  it("does not mutate the baseline constant between calls", () => {
    const a = buildBootConfigForSubcommand("run", ["--profile=flux"]);
    const b = buildBootConfigForSubcommand("run", []);
    expect(a?.["profile"]).toBe("flux");
    expect(b?.["profile"]).toBeUndefined();
  });
});

describe("buildChildEnv", () => {
  it("stringifies the boot config into VITE_PRISM_BOOT_CONFIG", () => {
    const env = buildChildEnv(
      { FOO: "bar" },
      { shellMode: "use", permission: "user" },
    );
    expect(env["FOO"]).toBe("bar");
    const raw = env["VITE_PRISM_BOOT_CONFIG"];
    if (raw === undefined) throw new Error("expected VITE_PRISM_BOOT_CONFIG to be set");
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    expect(parsed["shellMode"]).toBe("use");
    expect(parsed["permission"]).toBe("user");
  });

  it("leaves VITE_PRISM_BOOT_CONFIG unset for null boot config", () => {
    const env = buildChildEnv({ FOO: "bar" }, null);
    expect(env["VITE_PRISM_BOOT_CONFIG"]).toBeUndefined();
    expect(env["FOO"]).toBe("bar");
  });

  it("does not mutate the caller's env object", () => {
    const base: Record<string, string | undefined> = { FOO: "bar" };
    buildChildEnv(base, { shellMode: "use" });
    expect(base["VITE_PRISM_BOOT_CONFIG"]).toBeUndefined();
  });
});

describe("viteArgsForSubcommand", () => {
  it("maps bundle to vite build with forwarded args", () => {
    expect(viteArgsForSubcommand("bundle", ["--mode", "production"])).toEqual([
      "build",
      "--mode",
      "production",
    ]);
  });

  it("forwards args unchanged for run/build/admin/dev", () => {
    for (const cmd of ["run", "build", "admin", "dev"]) {
      expect(viteArgsForSubcommand(cmd, ["--port", "5173"])).toEqual([
        "--port",
        "5173",
      ]);
    }
  });
});
