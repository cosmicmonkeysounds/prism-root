/**
 * Widget builder DSL — the single way Studio's entity-to-Puck layer declares
 * a component. Collapses the giant hand-wired `ComponentConfig` table that
 * used to live in `entity-puck-config.tsx` into a flat table of `widget()`
 * calls, one per entity type.
 *
 * Every widget in the old pipeline shared the same three layers of ceremony:
 *
 *   1. Field declarations like
 *        `{ type: "text" } as unknown as Fields[string]`
 *      in a map keyed by the entity prop name.
 *   2. A `render: (props) => { const p = props as Record<string, unknown>; ...}`
 *      block that pulled each value out with `(p["foo"] as string) ?? "default"`.
 *   3. A wrapper `<div data-testid="puck-X" style={{ margin: "4px 0" }}>…</div>`
 *      plus, for data widgets, an inline
 *      `kernel.store.allObjects().filter(o => o.type === … && !o.deletedAt)`
 *      query and a `useSyncExternalStore`-based selected-id hook for row
 *      highlighting.
 *
 * This file folds all three into `widget()`:
 *
 * - `f.*` helpers return the concrete `Fields[string]` shape so callers
 *   write `collectionType: f.text()` instead of seven lines of casts.
 * - `widget()` wraps the render function so every caller automatically
 *   gets the `puck-<type>` wrapper, the reactive `selectedId`, a scoped
 *   `objects` array derived from the declared `query`, and strongly-typed
 *   `props` via the generic `P`. No `(p["foo"] as string)` at call sites.
 * - `query.kind === "by-prop"` reads the declared prop (default
 *   `collectionType`) and filters the store by its value; `"fixed"` hard-
 *   codes a record type; `"none"` skips the query entirely. Widgets that
 *   don't touch the object store stay as simple render functions.
 *
 * The builder is deliberately tiny — no lifecycle, no schema, no runtime
 * checks beyond what the closure does naturally. All of it is pure data
 * massage; the kernel is handed in once at boot and closed over from then
 * on. Tests can call `widget(fakeKernel, spec)` directly without mounting
 * Puck.
 */

import { useSyncExternalStore, type ReactNode } from "react";
import type { ComponentConfig, Fields } from "@measured/puck";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { StudioKernel } from "./studio-kernel.js";

// ── Field helpers ───────────────────────────────────────────────────────────

type Field = Fields[string];

function withLabel<T extends object>(base: T, label: string | undefined): T {
  return label !== undefined ? { ...base, label } : base;
}

/**
 * Typed shortcuts for every Puck field shape the old config hand-rolled.
 * Each returns a concrete `Fields[string]`, so call sites write
 * `collectionType: f.text()` instead of `{ type: "text" } as Fields[string]`.
 *
 * The `*Bool` variants (yes/no, show/hide, true/false) are distinguished
 * because the existing data uses different serializations:
 *   - `yesNo`      → boolean default (e.g. showStatus: true)
 *   - `showHide`   → boolean default, "Show"/"Hide" labels
 *   - `stringBool` → string "true"/"false" default (legacy radio with
 *                    `p["x"] === "true" || p["x"] === true` checks)
 */
export const f = {
  text: (label?: string): Field => withLabel({ type: "text" } as Field, label),
  area: (label?: string): Field =>
    withLabel({ type: "textarea" } as Field, label),
  num: (label?: string): Field =>
    withLabel({ type: "number" } as Field, label),
  select: (
    options: ReadonlyArray<readonly [string, string]>,
    label?: string,
  ): Field =>
    withLabel(
      {
        type: "select",
        options: options.map(([value, label]) => ({ value, label })),
      } as Field,
      label,
    ),
  yesNo: (label?: string): Field =>
    withLabel(
      {
        type: "radio",
        options: [
          { label: "Yes", value: true },
          { label: "No", value: false },
        ],
      } as Field,
      label,
    ),
  showHide: (label?: string): Field =>
    withLabel(
      {
        type: "radio",
        options: [
          { label: "Show", value: true },
          { label: "Hide", value: false },
        ],
      } as Field,
      label,
    ),
  stringBool: (
    opts: { trueLabel?: string; falseLabel?: string; label?: string } = {},
  ): Field =>
    withLabel(
      {
        type: "radio",
        options: [
          { label: opts.trueLabel ?? "Yes", value: "true" },
          { label: opts.falseLabel ?? "No", value: "false" },
        ],
      } as Field,
      opts.label,
    ),
  slot: (): Field => ({ type: "slot" } as Field),
} as const;

/**
 * Coerce the many serializations of a boolean-ish prop (literal boolean,
 * "true"/"false" string from a `stringBool` radio, or undefined) to a
 * real boolean. Mirrors the `p["x"] === "true" || p["x"] === true`
 * expression that used to clutter every widget render.
 */
export function asBool(value: unknown, fallback = false): boolean {
  if (value === true || value === "true") return true;
  if (value === false || value === "false") return false;
  return fallback;
}

// ── Selected-id hook ───────────────────────────────────────────────────────

/**
 * Read the currently-selected object id reactively from the kernel's atom
 * store via `useSyncExternalStore`. Data-widget renderers use this to
 * highlight the matching row without having to thread selection through
 * React props. Deliberately defined here (rather than imported from
 * `kernel-context.tsx`) to avoid the
 * `studio-kernel → entity-puck-config → kernel-context → studio-kernel`
 * import cycle — this file is loaded during kernel construction.
 */
export function useKernelSelectedId(kernel: StudioKernel): ObjectId | null {
  return useSyncExternalStore(
    (cb) => kernel.atoms.subscribe(cb),
    () => kernel.atoms.getState().selectedId,
  );
}

// ── Widget builder ──────────────────────────────────────────────────────────

/**
 * Strategy for populating the `objects` array handed to the render function.
 *
 * - `none`    — no query, `ctx.objects` is always empty. For pure-UI widgets
 *               (divider, spacer, code-block, etc.) that never touch the
 *               kernel store.
 * - `by-prop` — read the prop `prop` (defaults to `"collectionType"`) from
 *               the widget's props and filter `kernel.store.allObjects()` by
 *               `o.type === value && !o.deletedAt`. Empty when the prop is
 *               unset. For generic data-view widgets (list, table, kanban…).
 * - `fixed`   — hardcode the object type, regardless of props. For record-
 *               specific widgets (tasks-widget, notes-widget, …) that always
 *               read the same collection.
 */
export type ObjectQuery =
  | { kind: "none" }
  | { kind: "by-prop"; prop?: string }
  | { kind: "fixed"; type: string };

export interface WidgetContext<P> {
  readonly props: P;
  readonly kernel: StudioKernel;
  readonly selectedId: ObjectId | null;
  readonly objects: GraphObject[];
  readonly select: (id: string) => void;
  readonly update: (id: string, patch: Partial<GraphObject>) => void;
  readonly create: (
    obj: Omit<GraphObject, "id" | "createdAt" | "updatedAt">,
  ) => GraphObject;
}

export interface WidgetSpec<P extends Record<string, unknown>> {
  /** Kebab-case entity type. Used for the auto-generated `data-testid`. */
  readonly type: string;
  /** Map of prop name → Puck field. Use `f.*` helpers. */
  readonly fields: Record<string, Field>;
  /** Default prop values. Type parameter `P` ties these to the render. */
  readonly defaults: P;
  /** Object-query strategy. Defaults to `{ kind: "none" }`. */
  readonly query?: ObjectQuery;
  /**
   * Skip the standard `<div data-testid="puck-X" style={{ margin: "4px 0" }}>`
   * wrapper. Form-input widgets and anything else that renders a single leaf
   * pass `bare: true` so the builder doesn't add an outer div.
   */
  readonly bare?: boolean;
  /** Render the widget body. The builder adds the wrapper div by default. */
  readonly render: (ctx: WidgetContext<P>) => ReactNode;
}

/**
 * Turn a `WidgetSpec` into a Puck `ComponentConfig`. This is the function
 * each entry in the widget table calls — the output gets stored in
 * `kernel.puckComponents` via `registerDirect`.
 *
 * The returned render function is itself a component (Puck invokes
 * `render` with props, treating it as a function component) so the
 * `useKernelSelectedId` hook is legal — it runs once per Puck render.
 */
export function widget<P extends Record<string, unknown>>(
  kernel: StudioKernel,
  spec: WidgetSpec<P>,
): ComponentConfig {
  const query: ObjectQuery = spec.query ?? { kind: "none" };
  const testId = `puck-${spec.type}`;

  const renderInner = (rawProps: unknown): ReactNode => {
    const selectedId = useKernelSelectedId(kernel);
    const props = rawProps as P;

    let objects: GraphObject[] = [];
    if (query.kind === "by-prop") {
      const propName = query.prop ?? "collectionType";
      const typeValue = (props as Record<string, unknown>)[propName];
      if (typeof typeValue === "string" && typeValue !== "") {
        objects = kernel.store
          .allObjects()
          .filter((o) => o.type === typeValue && !o.deletedAt);
      }
    } else if (query.kind === "fixed") {
      objects = kernel.store
        .allObjects()
        .filter((o) => o.type === query.type && !o.deletedAt);
    }

    const ctx: WidgetContext<P> = {
      props,
      kernel,
      selectedId,
      objects,
      select: (id) => kernel.select(id as ObjectId),
      update: (id, patch) => {
        kernel.updateObject(id as ObjectId, patch);
      },
      create: (obj) => kernel.createObject(obj),
    };

    const body = spec.render(ctx);
    if (spec.bare) return <>{body}</>;
    return (
      <div data-testid={testId} style={{ margin: "4px 0" }}>
        {body}
      </div>
    );
  };

  return {
    fields: spec.fields as Fields,
    defaultProps: spec.defaults as Record<string, unknown>,
    render: renderInner as ComponentConfig["render"],
  };
}
