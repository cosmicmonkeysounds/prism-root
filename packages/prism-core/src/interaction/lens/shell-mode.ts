/**
 * Shell mode & permission types.
 *
 * A running Studio instance has two orthogonal runtime axes:
 *
 *   - `ShellMode`  — what's on screen *now*. Toggled in-place via hotkey
 *                    (Cmd+Shift+E) or a File-menu entry. Swapping modes
 *                    swaps which Puck shell tree is rendered and which
 *                    lenses the activity bar exposes.
 *
 *   - `Permission` — what's *available* at all. Set once at boot and
 *                    never toggled by the user. Lower-privilege boots
 *                    hide admin/dev panels and, more importantly, the
 *                    daemon refuses privileged IPC calls from them.
 *                    The UI filter is for ergonomics; the daemon is the
 *                    actual security boundary.
 *
 * Both axes are declared per-lens-bundle via optional `availableInModes`
 * and `minPermission` fields (defaults: `["build", "admin"]`, `"user"`).
 * The kernel composes these into a single lens filter.
 *
 * A `BootConfig` is the wire-format that kernel launchers (CLI, Tauri,
 * browser query params, Capacitor build-time defines) use to describe
 * the initial shell mode, permission, profile, and daemon transport.
 *
 * This module lives in `@prism/core/lens` because lens bundles are what
 * carry mode/permission metadata, and because the boot resolver needs a
 * stable shared schema visible to Studio, the CLI, and the daemon.
 */

/**
 * Runtime shell modes. Every LensBundle must be visible in at least one.
 *
 *   - `use`   — rendered app only. No component palette, no inspector,
 *               no activity bar clutter. This is what an end-user sees
 *               when they open a published Flux/Lattice/Musica build.
 *
 *   - `build` — in-place visual authoring. Component palette + inspector
 *               are visible; clicking a rendered widget selects it for
 *               editing. Available to both `user` and `dev` permission
 *               levels, but the palette contents are filtered by
 *               `minPermission`.
 *
 *   - `admin` — full IDE chrome: every panel, every tool, every debug
 *               surface. Only reachable with `dev` permission.
 */
export type ShellMode = "use" | "build" | "admin";

/** Ordered list of all shell modes. Stable iteration order for UIs. */
export const SHELL_MODES: readonly ShellMode[] = ["use", "build", "admin"];

/**
 * Runtime permission levels. The daemon stamps its own permission onto
 * every IPC envelope and rejects privileged calls when `user`. Studio
 * mirrors the level into the kernel so it can hide surfaces the daemon
 * would refuse anyway — but the kernel flag is advisory, not enforcing.
 *
 *   - `user`  — end-user: restricted build palette, no admin, no daemon
 *               build-step execution, no plugin install.
 *
 *   - `dev`   — developer: everything.
 */
export type Permission = "user" | "dev";

/** Ordered list of all permission levels (ascending privilege). */
export const PERMISSIONS: readonly Permission[] = ["user", "dev"];

/** Permission rank lookup — higher means more privileged. */
export const PERMISSION_RANK: Record<Permission, number> = {
  user: 0,
  dev: 1,
};

/**
 * `true` if `actual` meets or exceeds the `required` permission. Used by
 * the kernel lens filter and by the daemon's IPC gate.
 */
export function permissionAtLeast(
  actual: Permission,
  required: Permission,
): boolean {
  return PERMISSION_RANK[actual] >= PERMISSION_RANK[required];
}

/**
 * Optional metadata a LensBundle can declare to control where it shows
 * up. A bundle with no constraints is visible in `build` + `admin` at
 * `user` permission — i.e. the default is "authoring tool anyone can
 * reach". Panels that should only appear in the full IDE (plugin
 * manager, trust graph, daemon inspectors) must explicitly declare
 * `availableInModes: ["admin"]` and/or `minPermission: "dev"`.
 *
 * The kernel reads these via `filterLensBundlesByShellMode`.
 */
export interface ShellModeConstraints {
  readonly availableInModes?: readonly ShellMode[];
  readonly minPermission?: Permission;
}

/** Default modes a lens bundle is visible in when no annotation is set. */
export const DEFAULT_AVAILABLE_IN_MODES: readonly ShellMode[] = [
  "build",
  "admin",
];

/** Default minimum permission a lens bundle requires when not annotated. */
export const DEFAULT_MIN_PERMISSION: Permission = "user";

/**
 * BootConfig — the shared schema that any launcher (CLI, Tauri shell,
 * browser query params, Capacitor build defines, daemon spawn) uses to
 * tell Studio how to boot.
 *
 * All fields are optional so a bare boot (no config at all) still
 * produces a sensible default: full IDE, dev permission, no app
 * profile — i.e. the pre-mode-system behaviour.
 */
export interface BootConfig {
  /**
   * Focused-app profile id (matches an `AppProfile.id` from
   * `@prism/core/builder`). When present, the kernel filters lens
   * bundles to the profile's declared ids before the mode/permission
   * filter runs.
   */
  readonly profile?: string;

  /** Initial shell mode. Defaults to `admin` (full IDE). */
  readonly shellMode?: ShellMode;

  /** Permission level. Defaults to `dev` so plain `pnpm dev` is unchanged. */
  readonly permission?: Permission;

  /**
   * Daemon transport hint. Studio's ipc-bridge auto-detects at runtime,
   * but a launcher can pin it for testing (`"noop"` in unit tests,
   * `"tauri"` in packaged builds).
   */
  readonly daemonTransport?: "auto" | "tauri" | "capacitor" | "wasm" | "noop";

  /**
   * Free-form launcher metadata — useful for the daemon to record which
   * CLI invocation spawned this Studio instance. Never read by the
   * kernel itself.
   */
  readonly launcher?: {
    readonly name?: string;
    readonly version?: string;
    readonly startedAt?: string;
  };
}

/** The fully-defaulted boot config shape used internally by the kernel. */
export interface ResolvedBootConfig {
  readonly profile: string | null;
  readonly shellMode: ShellMode;
  readonly permission: Permission;
  readonly daemonTransport: NonNullable<BootConfig["daemonTransport"]>;
  readonly launcher: NonNullable<BootConfig["launcher"]>;
}

/** Default boot config: full IDE as a developer, no profile filter. */
export const DEFAULT_BOOT_CONFIG: ResolvedBootConfig = {
  profile: null,
  shellMode: "admin",
  permission: "dev",
  daemonTransport: "auto",
  launcher: {},
};

/** Fill in defaults for any unset BootConfig fields. */
export function resolveBootConfig(
  partial: BootConfig | undefined,
): ResolvedBootConfig {
  if (!partial) return DEFAULT_BOOT_CONFIG;
  return {
    profile: partial.profile ?? null,
    shellMode: partial.shellMode ?? DEFAULT_BOOT_CONFIG.shellMode,
    permission: partial.permission ?? DEFAULT_BOOT_CONFIG.permission,
    daemonTransport:
      partial.daemonTransport ?? DEFAULT_BOOT_CONFIG.daemonTransport,
    launcher: partial.launcher ?? DEFAULT_BOOT_CONFIG.launcher,
  };
}

/** Runtime guard — true when `value` is one of the three shell modes. */
export function isShellMode(value: unknown): value is ShellMode {
  return value === "use" || value === "build" || value === "admin";
}

/** Runtime guard — true when `value` is one of the two permission levels. */
export function isPermission(value: unknown): value is Permission {
  return value === "user" || value === "dev";
}

/**
 * Test whether a lens bundle's mode/permission constraints admit the
 * given runtime context. Pure so the kernel, tests, and Puck palette
 * filters can all share a single implementation.
 */
export function lensBundleMatchesShellContext(
  constraints: ShellModeConstraints | undefined,
  context: { mode: ShellMode; permission: Permission },
): boolean {
  const modes = constraints?.availableInModes ?? DEFAULT_AVAILABLE_IN_MODES;
  if (!modes.includes(context.mode)) return false;
  const minPermission = constraints?.minPermission ?? DEFAULT_MIN_PERMISSION;
  return permissionAtLeast(context.permission, minPermission);
}
