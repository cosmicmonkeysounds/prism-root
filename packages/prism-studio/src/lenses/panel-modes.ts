/**
 * Panel mode / permission constraints — central table.
 *
 * Every built-in Studio lens bundle is annotated here with the shell
 * modes it should appear in and the minimum permission level required
 * to see it. The aggregator in `collect.ts` reads this table and
 * attaches the constraints to each bundle at load time, so individual
 * panel files stay terse — they only declare their lens manifest +
 * component, not the runtime visibility policy.
 *
 * ## Why a central table (not per-file annotation)
 *
 * Two reasons:
 *
 *   1. **Auditability.** A security reviewer who wants to know "which
 *      panels are exposed in `use` mode at `user` permission?" can
 *      answer it by reading this one file — no `git grep` across 44
 *      panel files.
 *
 *   2. **Churn.** Panels are modified constantly; touching each one to
 *      re-declare its constraints on every refactor creates merge
 *      conflicts and noise. A central table keeps the policy decision
 *      orthogonal to the panel implementation.
 *
 * ## Defaults
 *
 * Bundles without an explicit entry here inherit the library defaults
 * from `@prism/core/lens`:
 *
 *   - `availableInModes: ["build", "admin"]`
 *   - `minPermission: "user"`
 *
 * i.e. "authoring tool anyone can reach, but not visible in the
 * published-app runtime." That's the correct default for most panels,
 * so only the true outliers (admin-only tools, canvas, etc.) need to
 * show up in this table.
 *
 * ## The three axes
 *
 *   - **`use`**   Panels visible to an end-user running a published app.
 *                 Almost nothing belongs here — published apps render
 *                 content via Puck widgets inside a canvas, not via the
 *                 panel machinery. Only `canvas-panel` and `layout-panel`
 *                 opt in today.
 *
 *   - **`build`** The authoring palette end-users see when they toggle
 *                 into edit mode. Form builders, content editors, theme
 *                 pickers, publish flow. Never internal debug surfaces.
 *
 *   - **`admin`** The full IDE. Everything admin-category and every
 *                 dev-only tool. This is the legacy "everything always
 *                 visible" mode.
 *
 * ## Permission tiers
 *
 *   - **`user`**  End-users in a published build. Safe to expose —
 *                 mostly content-layer authoring (text, forms, layouts,
 *                 tokens, publish).
 *
 *   - **`dev`**   Developers. Kernel-level panels (entity builder,
 *                 schema designer, plugin registry, trust graph, daemon
 *                 inspectors, relay manager). A published build with
 *                 `permission: "user"` hides these *and* the daemon
 *                 refuses the privileged IPC calls behind them even if
 *                 the UI gate slips.
 *
 * See `docs/dev/panel-modes.md` for the full rationale behind each
 * panel's placement.
 */

import type { Permission, ShellMode } from "@prism/core/lens";

/** One row of the panel-modes table. Both fields optional. */
export interface PanelModeEntry {
  readonly availableInModes?: readonly ShellMode[];
  readonly minPermission?: Permission;
}

/**
 * Keys are lens-bundle ids (matching `manifest.id`). Any panel not
 * listed here falls through to the `@prism/core/lens` defaults:
 * `availableInModes: ["build", "admin"]`, `minPermission: "user"`.
 */
export const PANEL_MODES: Readonly<Record<string, PanelModeEntry>> = {
  // ── Available in every mode (published-app-visible authoring) ────────
  // These render live page content, so an end-user running a published
  // build needs to see them inside the LensOutlet. Authoring
  // affordances inside each panel are still gated by the React tree on
  // the kernel's `shellMode` + `permission` — they just remain
  // *reachable*.
  "canvas": {
    availableInModes: ["use", "build", "admin"],
    minPermission: "user",
  },
  "layout": {
    availableInModes: ["use", "build", "admin"],
    minPermission: "user",
  },

  // ── Authoring tools end-users can use (build + admin, `user`) ─────────
  // Safe for an end-user to reach in `build` mode. Default already
  // admits these, so this section is mostly documentation — adding an
  // entry here is load-bearing only when we want to *exclude* `use` or
  // *raise* the permission floor.
  "editor": { availableInModes: ["build", "admin"], minPermission: "user" },
  "form-builder": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "form-facet": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "table-facet": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "report-facet": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "design-tokens": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "site-nav": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "saved-view": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "spatial-canvas": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "sitemap": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "publish": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },
  "value-list": {
    availableInModes: ["build", "admin"],
    minPermission: "user",
  },

  // ── Build + admin, dev-only (authoring tools behind a dev gate) ───────
  // These expose kernel-level authoring that could corrupt the vault if
  // misused (new entity defs, new edge types, Luau scripts, visual
  // scripts). `build` mode stays available so devs don't need to flip
  // to admin to use them, but `user` permission hides them entirely.
  "entity-builder": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "relationship-builder": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "schema-designer": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "facet-designer": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "luau-facet": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "visual-script": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "behavior": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },
  "sequencer": {
    availableInModes: ["build", "admin"],
    minPermission: "dev",
  },

  // ── Admin-only, dev-only (IDE / debug / system) ──────────────────────
  // The rest of the IDE. These never appear outside the full `admin`
  // shell. Moving any of these into `build` is a conscious policy
  // change — update `docs/dev/panel-modes.md` in the same commit.
  "admin": { availableInModes: ["admin"], minPermission: "dev" },
  "analysis": { availableInModes: ["admin"], minPermission: "dev" },
  "app-builder": { availableInModes: ["admin"], minPermission: "dev" },
  "assets": { availableInModes: ["admin"], minPermission: "dev" },
  "assets-mgmt": { availableInModes: ["admin"], minPermission: "dev" },
  "automation": { availableInModes: ["admin"], minPermission: "dev" },
  "crdt": { availableInModes: ["admin"], minPermission: "dev" },
  "crm": { availableInModes: ["admin"], minPermission: "dev" },
  "finance": { availableInModes: ["admin"], minPermission: "dev" },
  "graph": { availableInModes: ["admin"], minPermission: "dev" },
  "identity": { availableInModes: ["admin"], minPermission: "dev" },
  "import": { availableInModes: ["admin"], minPermission: "dev" },
  "life": { availableInModes: ["admin"], minPermission: "dev" },
  "platform": { availableInModes: ["admin"], minPermission: "dev" },
  "plugin": { availableInModes: ["admin"], minPermission: "dev" },
  "privilege-set": { availableInModes: ["admin"], minPermission: "dev" },
  "relay": { availableInModes: ["admin"], minPermission: "dev" },
  "settings": { availableInModes: ["admin"], minPermission: "dev" },
  "shortcuts": { availableInModes: ["admin"], minPermission: "dev" },
  "trust": { availableInModes: ["admin"], minPermission: "dev" },
  "vault": { availableInModes: ["admin"], minPermission: "dev" },
  "work": { availableInModes: ["admin"], minPermission: "dev" },
};

/**
 * Look up the constraint row for a lens id, or return an empty object
 * if the panel isn't in the table (i.e. it takes the library defaults).
 */
export function lookupPanelMode(lensId: string): PanelModeEntry {
  return PANEL_MODES[lensId] ?? {};
}
