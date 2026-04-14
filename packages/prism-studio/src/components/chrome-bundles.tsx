/**
 * Studio chrome as ShellWidgetBundles.
 *
 * Every piece of the current hand-coded studio-shell (ActivityBar, TabBar,
 * ObjectExplorer, ComponentPalette, InspectorPanel, UndoStatusBar,
 * PresenceIndicator) is wrapped here as a `ShellWidgetBundle`. The kernel
 * installs the bundles at boot, the lens-puck adapter auto-registers them
 * as Puck direct components, and the default shell tree references them
 * by name — so the chrome layout becomes data, not hand-coded JSX.
 *
 * Adding a new chrome widget:
 *   1. Author a React component that reads from `useKernel()` / context.
 *   2. Add one entry to `createBuiltinShellWidgetBundles()` below.
 *   3. Optionally drop it into `DEFAULT_STUDIO_SHELL_TREE` (or a profile).
 */

import { ActivityBar, TabBar } from "@prism/core/shell";
import {
  defineShellWidgetBundle,
  type ShellWidgetBundle,
} from "../lenses/bundle.js";
import { ObjectExplorer } from "./object-explorer.js";
import { ComponentPalette } from "./component-palette.js";
import { InspectorPanel } from "./inspector-panel.js";
import { UndoStatusBar } from "./undo-status-bar.js";
import { PresenceIndicator } from "./presence-indicator.js";
import { ShellModeMenu } from "./shell-mode-menu.js";

/**
 * Canonical list of Studio chrome widgets. Each bundle:
 *   - id: kebab-case (maps to PascalCase via `kebabToPascal` in the Puck
 *     registry — e.g. `object-explorer` → `ObjectExplorer`).
 *   - component: the React component.
 *   - puck: `LensPuckConfig` — empty fields are fine, the widget reads
 *     everything it needs from kernel context.
 */
export function createBuiltinShellWidgetBundles(): ShellWidgetBundle[] {
  return [
    defineShellWidgetBundle({
      id: "activity-bar",
      name: "Activity Bar",
      component: ActivityBar,
      puck: { label: "Activity Bar", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "tab-bar",
      name: "Tab Bar",
      component: TabBar,
      puck: { label: "Tab Bar", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "object-explorer",
      name: "Object Explorer",
      component: ObjectExplorer,
      puck: { label: "Object Explorer", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "component-palette",
      name: "Component Palette",
      component: ComponentPalette,
      puck: { label: "Component Palette", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "inspector-panel",
      name: "Inspector",
      component: InspectorPanel,
      puck: { label: "Inspector", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "undo-status-bar",
      name: "Undo Status",
      component: UndoStatusBar,
      puck: { label: "Undo Status", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "presence-indicator",
      name: "Presence",
      component: PresenceIndicator,
      puck: { label: "Presence", category: "Shell" },
    }),
    defineShellWidgetBundle({
      id: "shell-mode-menu",
      name: "Shell Mode Menu",
      component: ShellModeMenu,
      puck: { label: "Shell Mode Menu", category: "Shell" },
    }),
  ];
}
