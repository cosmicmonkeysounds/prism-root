/**
 * BuilderManager — end-to-end self-replication test.
 *
 * This test closes the loop between Studio's TS side and the daemon's
 * Rust side. Instead of a vi.fn() stub, the executor is wired to a
 * Node-backed `invoke` fn that faithfully mirrors the contract of
 * `packages/prism-daemon/src/commands/build.rs::run_build_step` —
 * real `fs.writeFile` for emit-file, real `child_process.spawnSync`
 * for run-command, same path resolution and stdout/stderr capture.
 *
 * We intentionally avoid running the real vite / tauri / cap commands
 * (that's what `pnpm tauri build` is for). Instead we prove:
 *   1. A real BuildPlan composed from a real AppProfile has the
 *      expected step shapes and serialized profile contents.
 *   2. When that plan flows through the tauri executor + node-backed
 *      invoke, every emit-file step writes a real file on disk and
 *      every run-command step (rewritten to a safe shell command)
 *      captures real stdout.
 *   3. The BuildRun returned to the kernel reflects the real
 *      filesystem side-effects — not a dry-run simulation.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { promises as fs } from "node:fs";
import { spawnSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, isAbsolute, dirname } from "node:path";

import {
  createBuilderManager,
  createTauriExecutor,
} from "./builder-manager.js";
import type { BuildStep, BuildPlan } from "./types.js";

// ── Node-backed invoke — mirror of daemon/src/commands/build.rs ──────────

interface DaemonInvokePayload {
  step: BuildStep;
  workingDir?: string;
  env?: Record<string, string>;
}

interface DaemonInvokeResult {
  stdout?: string;
  stderr?: string;
}

/**
 * Resolve a path the same way the daemon does: absolute paths pass
 * through, relative paths are joined onto workingDir.
 */
function resolvePath(workingDir: string, path: string): string {
  return isAbsolute(path) ? path : join(workingDir, path);
}

/**
 * Node-side implementation of `run_build_step`. The intent is contract
 * fidelity — given the same inputs, this produces the same side-effects
 * and returns the same stdout/stderr shape as the Rust version.
 */
async function daemonInvoke(
  command: string,
  rawPayload: Record<string, unknown>,
): Promise<DaemonInvokeResult> {
  if (command !== "run_build_step") {
    throw new Error(`unknown command: ${command}`);
  }
  const payload = rawPayload as unknown as DaemonInvokePayload;
  const workingDir = payload.workingDir ?? process.cwd();
  const env = payload.env ?? {};
  const step = payload.step;

  if (step.kind === "emit-file") {
    const target = resolvePath(workingDir, step.path);
    await fs.mkdir(dirname(target), { recursive: true });
    await fs.writeFile(target, step.contents);
    return {
      stdout: `wrote ${target} (${step.contents.length} bytes)`,
    };
  }

  if (step.kind === "run-command") {
    const cwd = step.cwd ? resolvePath(workingDir, step.cwd) : workingDir;
    const result = spawnSync(step.command, step.args, {
      cwd,
      env: { ...process.env, ...env },
      encoding: "utf8",
    });
    if (result.error) {
      throw new Error(
        `failed to spawn '${step.command}' in ${cwd}: ${result.error.message}`,
      );
    }
    if ((result.status ?? 0) !== 0) {
      throw new Error(
        `command '${step.command} ${step.args.join(" ")}' exited with code ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
      );
    }
    const out: DaemonInvokeResult = {};
    if (result.stdout) out.stdout = result.stdout;
    if (result.stderr) out.stderr = result.stderr;
    return out;
  }

  if (step.kind === "invoke-ipc") {
    throw new Error(
      `invoke-ipc step '${step.name}' is not yet supported by the daemon`,
    );
  }

  throw new Error(`unknown step kind: ${(step as { kind: string }).kind}`);
}

// ── Tests ────────────────────────────────────────────────────────────────

describe("BuilderManager — real plan composition", () => {
  it("planBuild('flux','web') produces steps with the documented shapes", () => {
    const manager = createBuilderManager();
    const plan = manager.planBuild("flux", "web", false);

    expect(plan.profileId).toBe("flux");
    expect(plan.target).toBe("web");
    expect(plan.dryRun).toBe(false);
    expect(plan.steps.length).toBeGreaterThan(0);

    // Every step must have a discriminating kind the daemon understands.
    for (const step of plan.steps) {
      expect(["emit-file", "run-command", "invoke-ipc"]).toContain(step.kind);
      expect(step.description).toBeTypeOf("string");
    }

    // Must at least include one emit-file step carrying the serialized profile.
    const emit = plan.steps.find((s) => s.kind === "emit-file");
    expect(emit).toBeDefined();
    if (emit?.kind === "emit-file") {
      expect(emit.path.length).toBeGreaterThan(0);
      expect(emit.contents).toContain('"id": "flux"');
    }
  });

  it("planBuilds covers all requested targets deterministically", () => {
    const manager = createBuilderManager();
    const plans = manager.planBuilds(
      "flux",
      ["web", "tauri", "capacitor-ios", "capacitor-android"],
      false,
    );
    expect(plans.map((p) => p.target)).toEqual([
      "web",
      "tauri",
      "capacitor-ios",
      "capacitor-android",
    ]);
    // Every plan has at least one step and a non-empty workingDir.
    for (const p of plans) {
      expect(p.steps.length).toBeGreaterThan(0);
      expect(p.workingDir.length).toBeGreaterThan(0);
    }
  });
});

describe("BuilderManager — end-to-end through node-backed daemon mirror", () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), "prism-build-e2e-"));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  it("executes a synthetic plan: emit-file writes to disk, run-command returns real stdout", async () => {
    // Build a hand-crafted plan with safe steps only. Using safe shell
    // commands (`printf`, `sh -c`) lets us prove the pipeline without
    // running real build toolchains.
    const plan: BuildPlan = {
      profileId: "flux",
      profileName: "Flux",
      target: "web",
      workingDir: workDir,
      env: { PRISM_BUILD_TAG: "e2e-test" },
      dryRun: false,
      artifacts: [],
      steps: [
        {
          kind: "emit-file",
          path: "profile/flux.json",
          contents: '{"id":"flux","version":"1.0.0"}',
          description: "emit serialized profile",
        },
        {
          kind: "emit-file",
          path: "nested/dir/marker.txt",
          contents: "hello",
          description: "emit into nested dir",
        },
        {
          kind: "run-command",
          command: "sh",
          args: ["-c", "printf '%s' \"$PRISM_BUILD_TAG\""],
          description: "echo build tag from env",
        },
      ],
    };

    const manager = createBuilderManager({
      executor: createTauriExecutor({ invoke: daemonInvoke }),
    });

    const run = await manager.runPlan(plan);

    expect(run.status).toBe("success");
    expect(run.steps.length).toBe(3);
    expect(run.steps.every((s) => s.status === "success")).toBe(true);

    // emit-file #1 — real file must exist with exact contents
    const profilePath = join(workDir, "profile/flux.json");
    const profileContents = await fs.readFile(profilePath, "utf8");
    expect(profileContents).toBe('{"id":"flux","version":"1.0.0"}');

    // emit-file #2 — parent dirs must be created
    const markerPath = join(workDir, "nested/dir/marker.txt");
    const markerContents = await fs.readFile(markerPath, "utf8");
    expect(markerContents).toBe("hello");

    // run-command — env var must have been passed through to the child
    const cmdStep = run.steps[2];
    expect(cmdStep?.stdout).toBe("e2e-test");
  });

  it("executes the emit-file portion of a real Flux web plan against a temp workspace", async () => {
    // Take a real BuildPlan, then swap workingDir for the tempdir and
    // drop run-command steps (we're not running pnpm/vite in the test).
    // The emit-file steps alone should write real files into workDir.
    const manager = createBuilderManager({
      executor: createTauriExecutor({ invoke: daemonInvoke }),
    });
    const basePlan = manager.planBuild("flux", "web", false);

    const safePlan: BuildPlan = {
      ...basePlan,
      workingDir: workDir,
      steps: basePlan.steps.filter((s) => s.kind === "emit-file"),
    };

    // The Flux plan must include at least one emit-file step.
    expect(safePlan.steps.length).toBeGreaterThan(0);

    const run = await manager.runPlan(safePlan);
    expect(run.status).toBe("success");
    expect(run.steps.every((s) => s.status === "success")).toBe(true);

    // At least one of the written files must contain the Flux profile.
    let foundProfile = false;
    for (const step of safePlan.steps) {
      if (step.kind !== "emit-file") continue;
      const target = resolvePath(workDir, step.path);
      const contents = await fs.readFile(target, "utf8");
      expect(contents).toBe(step.contents);
      if (contents.includes('"id": "flux"')) foundProfile = true;
    }
    expect(foundProfile).toBe(true);
  });

  it("surfaces real command failures as a failed BuildRun", async () => {
    const plan: BuildPlan = {
      profileId: "flux",
      profileName: "Flux",
      target: "web",
      workingDir: workDir,
      env: {},
      dryRun: false,
      artifacts: [],
      steps: [
        {
          kind: "emit-file",
          path: "before.txt",
          contents: "ok",
          description: "pre-failure emit",
        },
        {
          kind: "run-command",
          command: "false",
          args: [],
          description: "intentional failure",
        },
        {
          kind: "emit-file",
          path: "after.txt",
          contents: "unreachable",
          description: "should never run",
        },
      ],
    };

    const manager = createBuilderManager({
      executor: createTauriExecutor({ invoke: daemonInvoke }),
    });
    const run = await manager.runPlan(plan);

    expect(run.status).toBe("failed");
    // The first step succeeded and wrote its file…
    expect(run.steps[0]?.status).toBe("success");
    await expect(fs.readFile(join(workDir, "before.txt"), "utf8")).resolves.toBe("ok");
    // …the second failed…
    expect(run.steps[1]?.status).toBe("failed");
    expect(run.steps[1]?.errorMessage).toContain("exited with code");
    // …and the third was never attempted.
    expect(run.steps.length).toBe(2);
    await expect(fs.readFile(join(workDir, "after.txt"), "utf8")).rejects.toThrow();
  });
});
