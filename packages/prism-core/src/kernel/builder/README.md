# builder

Self-Replicating App Builder: pure data types and `createBuildPlan` for compiling a single Studio codebase into focused Prism apps (Studio, Flux, Lattice, Cadence, Grip) and Relay deployments. The `BuilderManager` wraps these primitives with a pluggable `BuildExecutor` (dry-run, Tauri IPC) to actually run plans via the Prism Daemon.

```ts
import { createBuildPlan, STUDIO_PROFILE } from "@prism/core/builder";
```

## Key exports

- `createBuildPlan({ profile, target, workingDir?, dryRun?, env? })` — pure planner: returns a `BuildPlan` of `BuildStep`s and expected `ArtifactDescriptor`s.
- `serializeBuildPlan(plan)` / `serializeAppProfile(profile)` / `parseAppProfile(json)` — deterministic JSON serialization.
- Built-in profiles: `STUDIO_PROFILE`, `FLUX_PROFILE`, `LATTICE_PROFILE`, `CADENCE_PROFILE`, `GRIP_PROFILE`, `RELAY_PROFILE`; `BUILT_IN_PROFILES`, `listBuiltInProfiles()`, `getBuiltInProfile(id)`.
- `ALL_BUILD_TARGETS` — the set of supported `BuildTarget`s (web, tauri, relay, …).
- `createBuilderManager(options)` — side-effectful runner around a `BuildExecutor`.
- `createDryRunExecutor()` / `createTauriExecutor(options)` — built-in executors.
- Types: `AppProfile`, `AppThemeConfig`, `BuildTarget`, `BuiltInProfileId`, `BuildStep`, `BuildPlan`, `BuildStepStatus`, `BuildStepResult`, `BuildRun`, `ArtifactDescriptor`, `ArtifactKind`, `BuilderManager`, `BuilderManagerOptions`, `BuildExecutor`, `BuildExecutionContext`, `TauriExecutorOptions`, `CreateBuildPlanOptions`.

## Usage

```ts
import { createBuildPlan, FLUX_PROFILE } from "@prism/core/builder";

const plan = createBuildPlan({
  profile: FLUX_PROFILE,
  target: "web",
  dryRun: true,
});
for (const step of plan.steps) console.log(step.kind, step.description);
```
