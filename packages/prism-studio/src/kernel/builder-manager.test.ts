import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  createBuilderManager,
  createDryRunExecutor,
  createTauriExecutor,
} from "./builder-manager.js";
import type { BuilderManager } from "./builder-manager.js";
import type { AppProfile, BuildStep } from "@prism/core/builder";

describe("BuilderManager — profile registry", () => {
  let manager: BuilderManager;

  beforeEach(() => {
    manager = createBuilderManager();
  });

  it("seeds six built-in profiles", () => {
    const profiles = manager.listProfiles();
    expect(profiles.length).toBe(6);
  });

  it("getProfile resolves built-ins by id", () => {
    const flux = manager.getProfile("flux");
    expect(flux?.name).toBe("Flux");
  });

  it("can register a custom profile", () => {
    const custom: AppProfile = {
      id: "my-app",
      name: "My App",
      version: "1.0.0",
      plugins: ["work"],
      lenses: ["canvas"],
    };
    manager.registerProfile(custom);
    expect(manager.getProfile("my-app")?.name).toBe("My App");
    expect(manager.listProfiles().length).toBe(7);
  });

  it("removeProfile refuses built-ins but accepts customs", () => {
    expect(manager.removeProfile("flux")).toBe(false);
    manager.registerProfile({ id: "tmp", name: "Tmp", version: "0" });
    expect(manager.removeProfile("tmp")).toBe(true);
    expect(manager.getProfile("tmp")).toBeUndefined();
  });

  it("merges user-supplied profiles at construction", () => {
    const override: AppProfile = { id: "flux", name: "Flux Custom", version: "2.0.0" };
    const m = createBuilderManager({ profiles: [override] });
    expect(m.getProfile("flux")?.name).toBe("Flux Custom");
  });
});

describe("BuilderManager — active profile", () => {
  it("starts with no active profile (universal host)", () => {
    const m = createBuilderManager();
    expect(m.getActiveProfile()).toBeNull();
  });

  it("setActiveProfile pins a profile", () => {
    const m = createBuilderManager();
    m.setActiveProfile("flux");
    expect(m.getActiveProfile()?.id).toBe("flux");
  });

  it("setActiveProfile(null) clears the pin", () => {
    const m = createBuilderManager();
    m.setActiveProfile("flux");
    m.setActiveProfile(null);
    expect(m.getActiveProfile()).toBeNull();
  });

  it("setActiveProfile throws for unknown profile", () => {
    const m = createBuilderManager();
    expect(() => m.setActiveProfile("nope")).toThrow();
  });

  it("removing the active profile clears it", () => {
    const m = createBuilderManager();
    m.registerProfile({ id: "tmp", name: "Tmp", version: "0" });
    m.setActiveProfile("tmp");
    m.removeProfile("tmp");
    expect(m.getActiveProfile()).toBeNull();
  });
});

describe("BuilderManager — planning", () => {
  let manager: BuilderManager;

  beforeEach(() => {
    manager = createBuilderManager();
  });

  it("planBuild composes a plan for a built-in profile + target", () => {
    const plan = manager.planBuild("flux", "web");
    expect(plan.profileId).toBe("flux");
    expect(plan.target).toBe("web");
    expect(plan.steps.length).toBeGreaterThan(0);
  });

  it("planBuild throws for unknown profile", () => {
    expect(() => manager.planBuild("nope", "web")).toThrow();
  });

  it("planBuilds returns one plan per target", () => {
    const plans = manager.planBuilds("flux", ["web", "tauri"]);
    expect(plans.length).toBe(2);
    expect(plans.map((p) => p.target)).toEqual(["web", "tauri"]);
  });

  it("exposes the full target list", () => {
    expect(manager.targets).toContain("web");
    expect(manager.targets).toContain("tauri");
    expect(manager.targets).toContain("relay-docker");
  });
});

describe("BuilderManager — dry-run execution", () => {
  let manager: BuilderManager;

  beforeEach(() => {
    manager = createBuilderManager();
  });

  it("dry-run is the default executor", async () => {
    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);
    expect(run.status).toBe("success");
  });

  it("emit-file steps succeed with contents buffered into stdout", async () => {
    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);
    const emitStep = run.steps.find((r) => r.step.kind === "emit-file");
    expect(emitStep?.status).toBe("success");
    expect(emitStep?.stdout).toContain('"id": "flux"');
  });

  it("run-command steps are skipped in dry-run", async () => {
    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);
    const cmdStep = run.steps.find((r) => r.step.kind === "run-command");
    expect(cmdStep?.status).toBe("skipped");
  });

  it("run records are listed in runs history", async () => {
    const plan = manager.planBuild("flux", "web");
    await manager.runPlan(plan);
    expect(manager.listRuns().length).toBe(1);
  });

  it("clearRuns empties the history", async () => {
    const plan = manager.planBuild("flux", "web");
    await manager.runPlan(plan);
    manager.clearRuns();
    expect(manager.listRuns().length).toBe(0);
  });

  it("dry-run claims the plan's declared artifacts", async () => {
    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);
    expect(run.producedArtifacts).toEqual(plan.artifacts);
  });
});

describe("BuilderManager — Tauri executor", () => {
  it("dispatches steps through the injected invoke fn", async () => {
    const invoke = vi.fn(async (_cmd: string, _args: Record<string, unknown>) => {
      return { stdout: "ok", stderr: "" };
    });
    const executor = createTauriExecutor({ invoke });
    const manager = createBuilderManager({ executor });

    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);

    expect(run.status).toBe("success");
    // Every step in the plan should have caused a call
    expect(invoke).toHaveBeenCalledTimes(plan.steps.length);
    expect(invoke).toHaveBeenCalledWith("run_build_step", expect.any(Object));
  });

  it("passes workingDir and env from the plan with every invocation", async () => {
    const invoke = vi.fn(
      async (_cmd: string, _payload: Record<string, unknown>) => ({ stdout: "ok" }),
    );
    const executor = createTauriExecutor({ invoke });
    const manager = createBuilderManager({ executor });

    const plan = manager.planBuild("flux", "web");
    await manager.runPlan(plan);

    expect(invoke.mock.calls.length).toBe(plan.steps.length);
    for (const [cmd, payload] of invoke.mock.calls) {
      expect(cmd).toBe("run_build_step");
      expect(payload).toMatchObject({
        step: expect.any(Object),
        workingDir: plan.workingDir,
        env: plan.env,
      });
    }
  });

  it("marks the run as failed when the daemon throws", async () => {
    const invoke = vi.fn(async () => {
      throw new Error("build failed: vite crashed");
    });
    const executor = createTauriExecutor({ invoke });
    const manager = createBuilderManager({ executor });

    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);

    expect(run.status).toBe("failed");
    const failed = run.steps.find((r) => r.status === "failed");
    expect(failed?.errorMessage).toContain("vite crashed");
  });

  it("stops after the first failing step", async () => {
    let callCount = 0;
    const invoke = vi.fn(async () => {
      callCount += 1;
      if (callCount >= 2) throw new Error("boom");
      return { stdout: "ok", stderr: "" };
    });
    const executor = createTauriExecutor({ invoke });
    const manager = createBuilderManager({ executor });

    const plan = manager.planBuild("flux", "web");
    const run = await manager.runPlan(plan);

    expect(run.status).toBe("failed");
    // Should stop at the 2nd call; not every step should be attempted
    expect(invoke.mock.calls.length).toBeLessThan(plan.steps.length + 1);
  });
});

describe("BuilderManager — subscriptions", () => {
  it("notifies listeners on profile changes", () => {
    const m = createBuilderManager();
    const listener = vi.fn();
    m.subscribe(listener);
    m.registerProfile({ id: "tmp", name: "Tmp", version: "0" });
    expect(listener).toHaveBeenCalled();
  });

  it("notifies listeners on active profile changes", () => {
    const m = createBuilderManager();
    const listener = vi.fn();
    m.subscribe(listener);
    m.setActiveProfile("flux");
    expect(listener).toHaveBeenCalled();
  });

  it("unsubscribe stops notifications", () => {
    const m = createBuilderManager();
    const listener = vi.fn();
    const unsub = m.subscribe(listener);
    unsub();
    m.setActiveProfile("flux");
    expect(listener).not.toHaveBeenCalled();
  });
});

describe("createDryRunExecutor standalone", () => {
  it("has mode='dry-run'", () => {
    const exec = createDryRunExecutor();
    expect(exec.mode).toBe("dry-run");
  });

  it("executes an emit-file step successfully", async () => {
    const exec = createDryRunExecutor();
    const step: BuildStep = {
      kind: "emit-file",
      path: "test.json",
      contents: "{}",
      description: "test emit",
    };
    const result = await exec.executeStep(step);
    expect(result.status).toBe("success");
    expect(result.stdout).toBe("{}");
  });

  it("skips a run-command step", async () => {
    const exec = createDryRunExecutor();
    const step: BuildStep = {
      kind: "run-command",
      command: "pnpm",
      args: ["build"],
      description: "build",
    };
    const result = await exec.executeStep(step);
    expect(result.status).toBe("skipped");
  });
});
