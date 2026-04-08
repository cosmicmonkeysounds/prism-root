/**
 * Builder — self-replicating Studio primitives.
 *
 * An App Profile pins a slice of Studio (plugins, lenses, theme, KBar
 * commands) to a focused app such as Flux or Lattice. A BuildPlan converts
 * a profile + target into a deterministic, serializable list of steps
 * that the Prism Daemon executes to produce the final artifact.
 *
 * This module is pure TypeScript — no Tauri, no Vite, no filesystem. All
 * side-effects happen in the Studio kernel's BuilderManager (which calls
 * the daemon) or in a dry-run fallback that only emits JSON plans.
 */

// ── Built-in App Profile IDs ──────────────────────────────────────────────

export type BuiltInProfileId =
  | "studio"
  | "flux"
  | "lattice"
  | "cadence"
  | "grip"
  | "relay";

// ── Build targets ─────────────────────────────────────────────────────────

/** A surface the BuilderManager can emit for a profile. */
export type BuildTarget =
  | "web"
  | "tauri"
  | "capacitor-ios"
  | "capacitor-android"
  | "relay-node"
  | "relay-docker";

export const ALL_BUILD_TARGETS: readonly BuildTarget[] = [
  "web",
  "tauri",
  "capacitor-ios",
  "capacitor-android",
  "relay-node",
  "relay-docker",
] as const;

// ── App Profile ───────────────────────────────────────────────────────────

/** Theme overrides a focused app can apply to Studio's shell. */
export interface AppThemeConfig {
  /** Hex color for primary accents. */
  primary?: string;
  /** Secondary accent. */
  secondary?: string;
  /** Path (relative to profile root) for the brand icon. */
  brandIcon?: string;
  /** Display name shown in window titles and activity bar. */
  displayName?: string;
}

/**
 * An App Profile is a JSON-serializable document that pins a slice of
 * Studio to a focused app. Loading a profile restricts the visible
 * plugin/lens/KBar surface without touching the underlying codebase.
 */
export interface AppProfile {
  /** Unique profile id (kebab-case). */
  id: string;
  /** Human-readable name. */
  name: string;
  /** Profile version — independent of the core version. */
  version: string;
  /**
   * Plugin bundle IDs to install. Empty array means "no plugins" (useful
   * for a Relay-only profile). Undefined means "use all built-ins".
   */
  plugins?: string[];
  /** Lens IDs to expose in the activity bar. Undefined = all built-in lenses. */
  lenses?: string[];
  /** The lens to open by default. */
  defaultLens?: string;
  /** Theme overrides. */
  theme?: AppThemeConfig;
  /**
   * KBar command IDs to surface at the App depth. The Glass Flip always
   * re-enables the full command palette if invoked.
   */
  kbarCommands?: string[];
  /** Optional path (relative to profile) to a Prism Manifest JSON file. */
  manifest?: string;
  /**
   * Whether the Glass Flip toggle is allowed in the stripped build.
   * Default: true (Prime Directive — every app is an IDE).
   */
  allowGlassFlip?: boolean;
  /**
   * Modules for Relay-target profiles. Ignored for app targets. Mirrors
   * the Relay builder pattern from @prism/core/relay.
   */
  relayModules?: string[];
}

// ── Artifacts ─────────────────────────────────────────────────────────────

/** Kinds of artifacts a BuildPlan can produce. */
export type ArtifactKind =
  | "directory"
  | "file"
  | "docker-image"
  | "installer"
  | "mobile-package";

/** A declared output of a BuildPlan. */
export interface ArtifactDescriptor {
  kind: ArtifactKind;
  /** Relative path under the build output root. */
  path: string;
  /** Human-readable description. */
  description: string;
  /** Optional MIME or installer type (e.g. "application/x-apple-diskimage"). */
  mimeType?: string;
}

// ── Build steps ───────────────────────────────────────────────────────────

/** A single step in a BuildPlan. */
export type BuildStep =
  /** Emit a JSON/YAML file containing serialized profile/manifest. */
  | { kind: "emit-file"; path: string; contents: string; description: string }
  /** Run a shell command (delegated to the daemon). */
  | { kind: "run-command"; command: string; args: string[]; cwd?: string; description: string }
  /** Invoke a Tauri IPC command on the daemon. */
  | { kind: "invoke-ipc"; name: string; payload: Record<string, unknown>; description: string };

// ── BuildPlan ─────────────────────────────────────────────────────────────

/**
 * A BuildPlan is the fully-resolved recipe for producing a single target.
 * It is deterministic (same inputs → same steps) and serializable so it
 * can be inspected, stored in CI, or executed later.
 */
export interface BuildPlan {
  profileId: string;
  profileName: string;
  target: BuildTarget;
  steps: BuildStep[];
  artifacts: ArtifactDescriptor[];
  env: Record<string, string>;
  /** Root directory (relative to monorepo) where the build will run. */
  workingDir: string;
  /** Whether this plan is a dry-run (emits files only, no commands). */
  dryRun: boolean;
}

// ── Execution result ──────────────────────────────────────────────────────

export type BuildStepStatus = "pending" | "running" | "success" | "failed" | "skipped";

export interface BuildStepResult {
  step: BuildStep;
  status: BuildStepStatus;
  startedAt: number;
  finishedAt?: number;
  stdout?: string;
  stderr?: string;
  errorMessage?: string;
}

export interface BuildRun {
  id: string;
  plan: BuildPlan;
  startedAt: number;
  finishedAt?: number;
  status: BuildStepStatus;
  steps: BuildStepResult[];
  producedArtifacts: ArtifactDescriptor[];
}
