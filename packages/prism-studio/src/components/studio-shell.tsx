/**
 * Studio Shell — Puck-rendered chrome.
 *
 * The legacy hand-coded shell that imported ActivityBar/ObjectExplorer/
 * InspectorPanel/... directly has been replaced with a thin wrapper that
 * projects `kernel.shellTree` through Puck's `<Render>`. Every widget in
 * the tree resolves against `kernel.puckComponents`, which is seeded by
 * `createStudioKernel` with:
 *
 *   - `Shell` / `LensOutlet` (built-in, from `@prism/core/puck`)
 *   - every `ShellWidgetBundle.puck` (ActivityBar, TabBar, …) auto-
 *     registered at boot by `registerShellWidgetBundlesInPuck`
 *   - every embeddable `LensBundle.puck` — so a lens author can drop
 *     their own panel straight into the shell with zero shell-side work
 *
 * This file is therefore ~20 lines of wiring: the entire shape of the
 * shell lives in `kernel.shellTree`, which a user (or an App Profile) can
 * freely rearrange.
 */

import { useSyncExternalStore, useMemo } from "react";
import { Render, type Config } from "@measured/puck";
import {
  SHELL_PUCK_CONFIG,
  puckConfigToComponentConfig,
} from "@prism/core/puck";
import { useKernel } from "../kernel/index.js";

export function StudioShell() {
  const kernel = useKernel();

  const shellTree = useSyncExternalStore(
    (cb) => kernel.onShellTreeChange(cb),
    () => kernel.shellTree,
    () => kernel.shellTree,
  );

  const config = useMemo<Config>(() => {
    const components = kernel.puckComponents.buildComponents({
      defs: [],
      kernel,
    });
    // `DEFAULT_STUDIO_SHELL_TREE` stores slot arrays on `root.props`, and
    // Puck renders the tree root using `config.root`. We bind the built-in
    // `ShellRenderer` directly as the root here rather than looking up
    // `components["Shell"]` because the Puck component map is also used
    // by the layout panel, where the user-facing `app-shell` / `page-shell`
    // entity defs (page-level content components) reuse similar names.
    // Using `config.root` keeps the shell's own layout isolated from the
    // layout-builder palette.
    const root = puckConfigToComponentConfig(SHELL_PUCK_CONFIG, "Shell");
    return { components, root } as unknown as Config;
  }, [kernel]);

  return (
    <div data-testid="studio-shell" style={{ height: "100vh", width: "100%" }}>
      <Render config={config} data={shellTree} />
    </div>
  );
}
