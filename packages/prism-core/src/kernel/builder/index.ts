/**
 * @prism/core/builder — self-replicating Studio primitives.
 *
 * The "builder" module exposes pure data types and a pure function
 * (createBuildPlan) for producing focused Prism apps and Relay
 * deployments from the same Studio codebase. The Studio kernel's
 * BuilderManager (side-effectful, Tauri-IPC aware) wraps these
 * primitives to actually execute plans via the Prism Daemon.
 */

export type {
  AppProfile,
  AppThemeConfig,
  BuildTarget,
  BuiltInProfileId,
  BuildStep,
  BuildPlan,
  BuildStepStatus,
  BuildStepResult,
  BuildRun,
  ArtifactDescriptor,
  ArtifactKind,
  StarterAppTemplate,
  StarterRouteTemplate,
  StarterShellChild,
} from "./types.js";
export { ALL_BUILD_TARGETS } from "./types.js";

export {
  materializeStarterApp,
} from "./starter-app.js";
export type {
  StarterCreateObjectFn,
  StarterCreateObjectInput,
  StarterCreatedObject,
  MaterializedStarterApp,
} from "./starter-app.js";

export {
  STUDIO_PROFILE,
  FLUX_PROFILE,
  LATTICE_PROFILE,
  CADENCE_PROFILE,
  GRIP_PROFILE,
  RELAY_PROFILE,
  BUILT_IN_PROFILES,
  listBuiltInProfiles,
  getBuiltInProfile,
  serializeAppProfile,
  parseAppProfile,
} from "./profiles.js";

export { createBuildPlan, serializeBuildPlan } from "./build-plan.js";
export type { CreateBuildPlanOptions } from "./build-plan.js";

// ── BuilderManager (side-effectful runner) ───────────────────────────────
export type {
  BuildExecutionContext,
  BuildExecutor,
  BuilderManagerOptions,
  BuilderManager,
  TauriExecutorOptions,
} from "./builder-manager.js";
export {
  createBuilderManager,
  createDryRunExecutor,
  createTauriExecutor,
} from "./builder-manager.js";
