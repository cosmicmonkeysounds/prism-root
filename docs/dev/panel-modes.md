# Panel Modes — Shell Mode & Permission

Prism Studio ships as a single Vite SPA that can boot into three
different "shells" depending on who's running it and what they're
trying to do. The shell is picked by two orthogonal axes:

| Axis         | Values                       | Mutable at runtime?      |
|--------------|------------------------------|--------------------------|
| `shellMode`  | `use` / `build` / `admin`    | yes — `Cmd+Shift+E` or `setShellMode` |
| `permission` | `user` / `dev`               | **no** — frozen at boot  |

The same JS bundle drives a published end-user app, a focused
authoring palette, and the full IDE. The bundle doesn't split — only
the `(shellMode, permission)` context does.

## Shell modes

Each mode owns its own Puck shell tree (`USE_SHELL_TREE`,
`BUILD_SHELL_TREE`, `ADMIN_SHELL_TREE` — all in `@prism/core/puck`) and
its own visible lens set. Switching modes swaps both in-place without
reloading — the kernel fires both `onShellModeChange` and
`onShellTreeChange` so every subscriber re-renders.

### `use` — published end-user app

The absolute minimum chrome. Looks and feels like a plain web app —
no activity bar of developer tools, no inspector, no palette. Only
lenses whose `availableInModes` explicitly lists `"use"` show up here.
Canvas, list/table/card-grid widgets, and user-facing forms are the
typical tenants.

### `build` — focused authoring palette

The "I'm building a page / a form / a dashboard" shell. Shows the
component palette, the inspector, and the active authoring lens
(Layout / Canvas / Form Builder). Hides power-tools like the graph,
CRDT inspector, and settings catalog that would just add noise during
focused authoring.

### `admin` — full IDE (default)

Every lens the build has installed, plus the full set of power
panels (CRDT inspector, plugin registry, shortcut manager, trust
dashboard, …). Matches the unchanged legacy default — a plain
`pnpm dev` still drops you here.

## Permission tiers

### `user`

The published end-user tier. A `user` kernel:

- Can only invoke daemon commands registered with
  `register_user` (or `register_with_permission(..., User, ...)`).
  The `admin_module` is the only built-in that opts in today —
  `daemon.admin` is a read-only health snapshot.
- Hides every lens whose `minPermission` is `"dev"`.
- Cannot escalate at runtime. Changing permission requires
  restarting Studio with a different launcher subcommand / URL.

### `dev`

The developer tier. Matches the unchanged legacy behaviour: every
command is reachable, every lens is visible (subject to
`availableInModes`), and `permissionAtLeast("dev", "user")` returns
`true` so `dev` implicitly satisfies any `user`-tagged command.

## Authoring a lens for a specific mode

Any panel file that exports a `LensBundle` can declare its
availability with `withShellModes`:

```ts
import { defineLensBundle, withShellModes } from "@prism/core/lens";

export const myLensBundle = withShellModes(
  defineLensBundle(myManifest, MyPanelComponent, myPuckConfig),
  {
    availableInModes: ["build", "admin"], // hidden in "use"
    minPermission: "user",                // reachable from user tier
  },
);
```

The defaults are **`availableInModes: ["build", "admin"]`** and
**`minPermission: "user"`**, so a panel that doesn't say anything
shows up in `build` + `admin` at any tier. Opt into `use` explicitly
(and raise `minPermission` to `"dev"`) only when the panel is
genuinely safe / useful for the other context.

## Launching Studio

The `prism-studio` Node bin translates high-level subcommands into a
`BootConfig` JSON blob, which the resolver (`src/boot/load-boot-config.ts`)
reads on startup.

```sh
npx prism-studio run     [--profile=<id>]   # use mode, user permission
npx prism-studio build   [--profile=<id>]   # build mode, dev permission
npx prism-studio admin   [--profile=<id>]   # admin mode, dev permission
npx prism-studio dev                         # no boot override (legacy)
npx prism-studio bundle                      # production vite build
```

`--profile=flux|lattice|musica|...` folds into the boot config and
filters the lens set down to the ones the profile lists.

The daemon gets its own `--permission=user|dev` flag, parsed in
`prism_daemond.rs`. Transport adapters that trust the caller's tier
(Tauri IPC from an end-user build, HTTP served behind an auth
proxy, …) should call `kernel.invoke_with_permission(name, payload,
caller)` — the un-gated `kernel.invoke` is reserved for trusted
in-process embedders and tests.

## Resolution precedence

`loadBootConfig` merges four sources in order (later sources override
earlier ones):

1. `DEFAULT_BOOT_CONFIG` — `admin` + `dev` (legacy default).
2. **Build-time default** from `VITE_PRISM_BOOT_DEFAULT`. Capacitor /
   mobile builds bake `permission: "user"` here so the published app
   has a hard permission ceiling.
3. **`VITE_PRISM_BOOT_CONFIG`** from the Node launcher.
4. **Query params** — `?mode=use&permission=user&profile=flux`. Used
   only to *narrow* the build-time ceiling; a URL that tries to
   escalate permission past the ceiling is clamped back with a
   console warning.

The resolver is deliberately synchronous (runs once at module load)
so the kernel can read the result inside its synchronous factory. A
malformed query param logs a warning and falls through to the next
source rather than throwing — a bad URL never bricks Studio.

## Testing

- `packages/prism-core/src/interaction/lens/shell-mode.test.ts`
  — type guards, defaults, filter matrix, `withShellModes`
  immutability, `resolveBootConfig`.
- `packages/prism-studio/src/boot/load-boot-config.test.ts`
  — precedence, build-time ceiling clamping, fill-in.
- `packages/prism-studio/src/kernel/studio-kernel-shell.test.ts`
  — `setShellMode`, `getVisibleLensIds`, listener notification,
  shell-tree slot swap.
- `packages/prism-studio/src/bin/prism-studio-launcher.test.ts`
  — launcher helpers (boot config, profile flag, vite args, env).
- `packages/prism-daemon/src/registry.rs` + `bin/prism_daemond.rs`
  + `tests/kernel_integration.rs` — daemon-side permission gate.
