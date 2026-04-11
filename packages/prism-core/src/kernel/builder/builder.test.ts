import { describe, it, expect } from "vitest";
import {
  createBuildPlan,
  listBuiltInProfiles,
  getBuiltInProfile,
  serializeAppProfile,
  parseAppProfile,
  serializeBuildPlan,
  BUILT_IN_PROFILES,
  ALL_BUILD_TARGETS,
  FLUX_PROFILE,
  STUDIO_PROFILE,
  RELAY_PROFILE,
} from "./index.js";
import type { AppProfile, BuildPlan } from "./index.js";

describe("Built-in App Profiles", () => {
  it("ships six profiles (studio, flux, lattice, cadence, grip, relay)", () => {
    const profiles = listBuiltInProfiles();
    expect(profiles.length).toBe(6);
    const ids = profiles.map((p) => p.id).sort();
    expect(ids).toEqual(["cadence", "flux", "grip", "lattice", "relay", "studio"]);
  });

  it("resolves profiles by built-in id", () => {
    expect(getBuiltInProfile("flux").name).toBe("Flux");
    expect(getBuiltInProfile("studio").id).toBe("studio");
  });

  it("studio profile is unprofiled (no plugins/lenses filter)", () => {
    expect(STUDIO_PROFILE.plugins).toBeUndefined();
    expect(STUDIO_PROFILE.lenses).toBeUndefined();
    expect(STUDIO_PROFILE.allowGlassFlip).toBe(true);
  });

  it("flux profile pins work/finance/crm plugins and record-browser default lens", () => {
    expect(FLUX_PROFILE.plugins).toEqual(["work", "finance", "crm"]);
    expect(FLUX_PROFILE.defaultLens).toBe("record-browser");
    expect(FLUX_PROFILE.theme?.primary).toBe("#6C5CE7");
  });

  it("relay profile has no plugins and explicit relay modules", () => {
    expect(RELAY_PROFILE.plugins).toEqual([]);
    expect(RELAY_PROFILE.lenses).toEqual([]);
    expect(RELAY_PROFILE.relayModules?.length).toBeGreaterThan(0);
    expect(RELAY_PROFILE.allowGlassFlip).toBe(false);
  });

  it("BUILT_IN_PROFILES map keys match profile ids", () => {
    for (const [key, profile] of Object.entries(BUILT_IN_PROFILES)) {
      expect(profile.id).toBe(key);
    }
  });
});

describe("serializeAppProfile / parseAppProfile", () => {
  it("round-trips a profile without loss", () => {
    const json = serializeAppProfile(FLUX_PROFILE);
    const parsed = parseAppProfile(json);
    expect(parsed).toEqual(FLUX_PROFILE);
  });

  it("emits canonical pretty JSON with trailing newline", () => {
    const json = serializeAppProfile(STUDIO_PROFILE);
    expect(json.endsWith("\n")).toBe(true);
    expect(json).toContain('"id": "studio"');
  });

  it("rejects non-object JSON", () => {
    expect(() => parseAppProfile("null")).toThrow();
    expect(() => parseAppProfile("[]")).toThrow();
  });

  it("rejects profiles missing required fields", () => {
    expect(() => parseAppProfile(JSON.stringify({ id: "x" }))).toThrow();
  });
});

describe("createBuildPlan — deterministic & exhaustive", () => {
  it("produces a plan for every build target without throwing", () => {
    for (const target of ALL_BUILD_TARGETS) {
      const profile: AppProfile = target.startsWith("relay") ? RELAY_PROFILE : FLUX_PROFILE;
      const plan = createBuildPlan({ profile, target });
      expect(plan.target).toBe(target);
      expect(plan.steps.length).toBeGreaterThan(0);
      expect(plan.artifacts.length).toBeGreaterThan(0);
    }
  });

  it("is deterministic — same inputs produce identical plans", () => {
    const a = createBuildPlan({ profile: FLUX_PROFILE, target: "web" });
    const b = createBuildPlan({ profile: FLUX_PROFILE, target: "web" });
    expect(a).toEqual(b);
  });

  it("web target emits the profile and runs vite build", () => {
    const plan = createBuildPlan({ profile: FLUX_PROFILE, target: "web" });
    const kinds = plan.steps.map((s) => s.kind);
    expect(kinds).toContain("emit-file");
    expect(kinds).toContain("run-command");
    expect(plan.artifacts[0].path).toContain("dist");
  });

  it("tauri target includes both vite-build and tauri-build commands", () => {
    const plan = createBuildPlan({ profile: FLUX_PROFILE, target: "tauri" });
    const commands = plan.steps
      .filter((s) => s.kind === "run-command")
      .map((s) => (s.kind === "run-command" ? s.args.join(" ") : ""));
    expect(commands.some((c) => c.includes("tauri"))).toBe(true);
    expect(plan.artifacts.some((a) => a.path.endsWith(".dmg"))).toBe(true);
  });

  it("capacitor-ios and capacitor-android emit different artifacts", () => {
    const ios = createBuildPlan({ profile: FLUX_PROFILE, target: "capacitor-ios" });
    const and = createBuildPlan({ profile: FLUX_PROFILE, target: "capacitor-android" });
    expect(ios.artifacts.some((a) => a.path.endsWith(".ipa"))).toBe(true);
    expect(and.artifacts.some((a) => a.path.endsWith(".apk"))).toBe(true);
  });

  it("relay-node emits relay.config.json from composed modules", () => {
    const plan = createBuildPlan({ profile: RELAY_PROFILE, target: "relay-node" });
    const emit = plan.steps.find(
      (s) => s.kind === "emit-file" && s.path.endsWith("relay.config.json"),
    );
    expect(emit).toBeDefined();
    if (emit && emit.kind === "emit-file") {
      const parsed = JSON.parse(emit.contents);
      expect(parsed.modules).toEqual(RELAY_PROFILE.relayModules);
      expect(parsed.mode).toBe("server");
    }
  });

  it("relay-docker extends relay-node with a docker build command", () => {
    const plan = createBuildPlan({ profile: RELAY_PROFILE, target: "relay-docker" });
    const docker = plan.steps.find(
      (s) => s.kind === "run-command" && s.command === "docker",
    );
    expect(docker).toBeDefined();
    expect(plan.artifacts.some((a) => a.kind === "docker-image")).toBe(true);
  });

  it("dry-run defaults to true", () => {
    const plan = createBuildPlan({ profile: FLUX_PROFILE, target: "web" });
    expect(plan.dryRun).toBe(true);
  });

  it("dry-run can be disabled explicitly", () => {
    const plan = createBuildPlan({ profile: FLUX_PROFILE, target: "web", dryRun: false });
    expect(plan.dryRun).toBe(false);
  });
});

describe("serializeBuildPlan", () => {
  it("emits a canonical JSON string", () => {
    const plan = createBuildPlan({ profile: FLUX_PROFILE, target: "web" });
    const json = serializeBuildPlan(plan);
    const parsed = JSON.parse(json) as BuildPlan;
    expect(parsed.profileId).toBe("flux");
    expect(parsed.target).toBe("web");
  });
});
