/**
 * Unified Shell Puck component.
 *
 * This file supplies the single `Shell` component every Prism Shell (Studio
 * chrome, page-builder pages, custom user-authored shells) reduces to. The
 * grid math and resize-drag interaction live in sibling primitives so users
 * who want to build their own shell shape without the Puck wrapper can
 * import those directly:
 *
 *   - `ShellGrid`        →  `@prism/core/puck` (grid primitive)
 *   - `useResizeHandle`  →  `@prism/core/puck` (drag-to-resize hook)
 *   - `ResizeHandle`     →  `@prism/core/puck` (visual drag handle)
 *
 * Six named slots — activityBar, topBar, leftBar, main, rightBar, bottomBar —
 * map onto a 3×4 CSS grid. Any slot with no content collapses its row/column
 * to 0px. The bar dimensions are editable from the Puck inspector, and when
 * the host passes an `onCommit` callback each bar becomes drag-resizable too.
 *
 * ```
 *   ┌────┬─────────────────────────────┐
 *   │    │ topBar                      │
 *   │ AB ├────┬───────────────┬────────┤
 *   │    │ lf │     main      │   rt   │
 *   │    ├────┴───────────────┴────────┤
 *   │    │ bottomBar                   │
 *   └────┴─────────────────────────────┘
 * ```
 *
 * ## Why this file lives in `bindings/puck`
 *
 * `@prism/core/lens` stays React-free. Anything that imports `react` or
 * `@measured/puck` has to live under `bindings/`. This file sits next to
 * `component-registry.ts` and `lens-puck-adapter.ts` so all three Puck-
 * side concerns are in one place: the registry is the seam, the adapter
 * auto-registers bundles into it, and this file supplies the baseline
 * shell component.
 */

import type { Data, Fields } from "@measured/puck";
import {
  useCallback,
  useEffect,
  type ComponentType,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useLensContext, useShellStore } from "@prism/core/shell";
import type { LensPuckConfig } from "./lens-puck-adapter.js";
import { ShellGrid, type ShellBarSizes } from "./shell-grid.js";
import { useResizeHandle, ResizeHandle } from "./use-resize-handle.js";

/**
 * Slot function signature Puck passes to a component's `render()` for
 * each declared `{ type: "slot" }` field. Calling it renders whatever
 * drop-zone content the user has placed into that slot.
 */
export type SlotFn = (props?: Record<string, unknown>) => ReactNode;

/** Named shell slots, in drop-palette order. */
export const SHELL_SLOTS = [
  "activityBar",
  "topBar",
  "leftBar",
  "main",
  "rightBar",
  "bottomBar",
] as const;
export type ShellSlot = (typeof SHELL_SLOTS)[number];

/** Keys the `onCommit` callback uses to report a settled drag. */
export type ShellBarKey =
  | "activityBarWidth"
  | "topBarHeight"
  | "leftBarWidth"
  | "rightBarWidth"
  | "bottomBarHeight";

/**
 * Props the unified `Shell` Puck component receives. The six slot fields
 * become `SlotFn`s at render time; the five sizing numbers live on the root
 * so users can edit bar widths from the Puck inspector. Host apps can
 * override the theme by passing custom CSS vars via `className`.
 */
export interface ShellProps extends ShellBarSizes {
  background?: string;
  foreground?: string;
  className?: string;
  /** When true, fills the viewport (100vh). Default true. */
  fullscreen?: boolean;
  activityBar?: ReactNode | SlotFn;
  topBar?: ReactNode | SlotFn;
  leftBar?: ReactNode | SlotFn;
  main?: ReactNode | SlotFn;
  rightBar?: ReactNode | SlotFn;
  bottomBar?: ReactNode | SlotFn;
  /** Commit callback fired on pointerup after a resize drag settles. */
  onCommit?: (key: ShellBarKey, value: number) => void;
}

function hasContent(node: ReactNode): boolean {
  return node !== null && node !== undefined && node !== false && node !== true;
}

/**
 * Resolve a Puck slot prop to a ReactNode. Puck passes slot fields to
 * `render()` as callable `SlotFn`s; legacy direct `ReactNode` values are
 * still supported so tests and non-Puck callers can mount the renderer
 * directly. Returns `null` when the slot is missing or its function
 * returned nothing, which keeps `hasContent()` from classifying the slot
 * as populated and preserves the "collapse to 0px" grid behavior.
 *
 * The optional `slotProps` are forwarded to Puck's internal `SlotRender`,
 * which spreads `style`/`className` onto the wrapper `<div>` it always
 * emits around slot content. Without this, the wrapper has no inline
 * styles and collapses to height 0 — making any lens that uses
 * `height: 100%` (graph, sitemap, canvas, …) invisible and breaking
 * pan/zoom in xyflow. Passing `{ style: { height: "100%", ... } }`
 * gives the lens a real parent to fill.
 */
function resolveSlot(
  slot: ReactNode | SlotFn | undefined,
  slotProps?: { style?: CSSProperties; className?: string },
): ReactNode {
  if (slot === undefined || slot === null) return null;
  if (typeof slot === "function") {
    return (slot as SlotFn)(slotProps as Record<string, unknown> | undefined);
  }
  return slot;
}

/** Style spread onto every populated slot's Puck wrapper so lenses can fill it. */
const SLOT_FILL_STYLE: CSSProperties = {
  width: "100%",
  height: "100%",
  display: "flex",
  flexDirection: "column",
  minWidth: 0,
  minHeight: 0,
};

/** Default bar sizes used when a prop is absent. */
const DEFAULT_SIZES: Required<ShellBarSizes> = {
  activityBarWidth: 48,
  topBarHeight: 36,
  leftBarWidth: 260,
  rightBarWidth: 280,
  bottomBarHeight: 0,
};

/**
 * Unified `Shell` React component. Resolves each slot, wires up optional
 * drag-to-resize, and delegates the actual layout to `<ShellGrid>`. When
 * `onCommit` is omitted the handles don't render — the grid is still
 * editable from the Puck inspector but not from direct mouse drag.
 */
export function ShellRenderer(props: ShellProps & { puck?: unknown }): ReactNode {
  const {
    activityBarWidth: activityBarWidthProp = DEFAULT_SIZES.activityBarWidth,
    topBarHeight: topBarHeightProp = DEFAULT_SIZES.topBarHeight,
    leftBarWidth: leftBarWidthProp = DEFAULT_SIZES.leftBarWidth,
    rightBarWidth: rightBarWidthProp = DEFAULT_SIZES.rightBarWidth,
    bottomBarHeight: bottomBarHeightProp = DEFAULT_SIZES.bottomBarHeight,
    background,
    foreground,
    className,
    fullscreen = true,
    onCommit,
  } = props;

  const slotFillProps = { style: SLOT_FILL_STYLE };
  const activityBarNode = resolveSlot(props.activityBar, slotFillProps);
  const topBarNode = resolveSlot(props.topBar, slotFillProps);
  const leftBarNode = resolveSlot(props.leftBar, slotFillProps);
  const mainNode = resolveSlot(props.main, slotFillProps);
  const rightBarNode = resolveSlot(props.rightBar, slotFillProps);
  const bottomBarNode = resolveSlot(props.bottomBar, slotFillProps);

  const commit = useCallback(
    (key: ShellBarKey) => (value: number) => onCommit?.(key, value),
    [onCommit],
  );

  const activity = useResizeHandle(activityBarWidthProp, "x", 1, commit("activityBarWidth"));
  const top = useResizeHandle(topBarHeightProp, "y", 1, commit("topBarHeight"));
  const left = useResizeHandle(leftBarWidthProp, "x", 1, commit("leftBarWidth"));
  const right = useResizeHandle(rightBarWidthProp, "x", -1, commit("rightBarWidth"));
  const bottom = useResizeHandle(bottomBarHeightProp, "y", -1, commit("bottomBarHeight"));

  // Sync local drag state when props change externally (undo/redo etc.)
  useEffect(() => { activity.setValueFromProps(activityBarWidthProp); }, [activityBarWidthProp, activity]);
  useEffect(() => { top.setValueFromProps(topBarHeightProp); }, [topBarHeightProp, top]);
  useEffect(() => { left.setValueFromProps(leftBarWidthProp); }, [leftBarWidthProp, left]);
  useEffect(() => { right.setValueFromProps(rightBarWidthProp); }, [rightBarWidthProp, right]);
  useEffect(() => { bottom.setValueFromProps(bottomBarHeightProp); }, [bottomBarHeightProp, bottom]);

  const rootStyle: CSSProperties = {
    fontFamily: "system-ui",
    ...(background !== undefined ? { background } : {}),
    ...(foreground !== undefined ? { color: foreground } : {}),
  };

  const wrapWithResizeHandle = (
    node: ReactNode,
    handle: ReturnType<typeof useResizeHandle>,
    orientation: "horizontal" | "vertical",
    handleStyle: CSSProperties,
  ): ReactNode => {
    if (!onCommit || !hasContent(node)) return node;
    return (
      <>
        {node}
        <ResizeHandle
          orientation={orientation}
          active={handle.dragging}
          onPointerDown={handle.onPointerDown}
          style={handleStyle}
        />
      </>
    );
  };

  const overlays = null;

  return (
    <ShellGrid
      testId="shell"
      className={className ?? ""}
      style={rootStyle}
      fullscreen={fullscreen}
      activityBarWidth={activity.value}
      topBarHeight={top.value}
      leftBarWidth={left.value}
      rightBarWidth={right.value}
      bottomBarHeight={bottom.value}
      activityBar={wrapWithResizeHandle(
        activityBarNode,
        activity,
        "horizontal",
        { top: 0, bottom: 0, right: 0, width: 6 },
      )}
      topBar={wrapWithResizeHandle(
        topBarNode,
        top,
        "vertical",
        { left: 0, right: 0, bottom: 0, height: 6 },
      )}
      leftBar={wrapWithResizeHandle(
        leftBarNode,
        left,
        "horizontal",
        { top: 0, bottom: 0, right: 0, width: 6 },
      )}
      main={mainNode}
      rightBar={wrapWithResizeHandle(
        rightBarNode,
        right,
        "horizontal",
        { top: 0, bottom: 0, left: 0, width: 6 },
      )}
      bottomBar={wrapWithResizeHandle(
        bottomBarNode,
        bottom,
        "vertical",
        { left: 0, right: 0, top: 0, height: 6 },
      )}
      cellStyles={{
        activity: { position: "relative" },
        top: { position: "relative" },
        left: { position: "relative" },
        right: { position: "relative" },
        bottom: { position: "relative" },
      }}
      overlays={overlays}
    />
  );
}

/** Field schema for `Shell`. Six slot fields + numeric knobs + theme. */
const SHELL_FIELDS: Fields<ShellProps> = {
  activityBarWidth: { type: "number", label: "Activity bar width" },
  topBarHeight: { type: "number", label: "Top bar height" },
  leftBarWidth: { type: "number", label: "Left bar width" },
  rightBarWidth: { type: "number", label: "Right bar width" },
  bottomBarHeight: { type: "number", label: "Bottom bar height" },
  background: { type: "text", label: "Background" },
  foreground: { type: "text", label: "Foreground" },
  activityBar: { type: "slot" },
  topBar: { type: "slot" },
  leftBar: { type: "slot" },
  main: { type: "slot" },
  rightBar: { type: "slot" },
  bottomBar: { type: "slot" },
} as unknown as Fields<ShellProps>;

/**
 * Single `LensPuckConfig` for the unified `Shell`. The adapter synthesises
 * the Puck `ComponentConfig` at registration time so there is no parallel
 * hand-written declaration to keep in sync with the fields map.
 */
export const SHELL_PUCK_CONFIG: LensPuckConfig<ShellProps> = {
  label: "Shell",
  category: "Shell",
  fields: SHELL_FIELDS,
  defaultProps: {
    activityBarWidth: DEFAULT_SIZES.activityBarWidth,
    topBarHeight: DEFAULT_SIZES.topBarHeight,
    leftBarWidth: DEFAULT_SIZES.leftBarWidth,
    rightBarWidth: DEFAULT_SIZES.rightBarWidth,
    bottomBarHeight: DEFAULT_SIZES.bottomBarHeight,
  },
  embeddable: true,
  zones: SHELL_SLOTS,
  render: ShellRenderer as NonNullable<LensPuckConfig<ShellProps>["render"]>,
};

// ── LensOutlet ─────────────────────────────────────────────────────────────
//
// `LensOutlet` is the Puck component that the `Shell`'s `main` slot drops
// onto by default. At render time it resolves the currently-active tab out
// of the shell store and mounts that tab's lens component. It is the bridge
// between the imperative tab/lens model and the declarative Puck shell tree.
//
// There is exactly one `LensOutlet` in a running Studio: the shell tree
// hands off the "which panel is visible right now" decision back to the
// lens system.

export interface LensOutletProps {
  /** Optional fallback shown when no tab is open. */
  emptyMessage?: string;
}

export function LensOutletRenderer({ emptyMessage }: LensOutletProps): ReactNode {
  const { components } = useLensContext();
  const { tabs, activeTabId } = useShellStore();
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const Active: ComponentType | undefined = activeTab
    ? components.get(activeTab.lensId)
    : undefined;
  if (Active && activeTab) {
    return <Active key={activeTab.id} />;
  }
  return (
    <div
      data-testid="lens-outlet-empty"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: "#555",
        fontSize: 14,
      }}
    >
      {emptyMessage ?? "No tab open. Click a lens in the activity bar."}
    </div>
  );
}

export const LENS_OUTLET_PUCK_CONFIG: LensPuckConfig<LensOutletProps> = {
  label: "Lens Outlet",
  category: "Shell",
  fields: {
    emptyMessage: { type: "text", label: "Empty message" },
  } as Fields<LensOutletProps>,
  defaultProps: { emptyMessage: "No tab open. Click a lens in the activity bar." },
  embeddable: true,
  render: LensOutletRenderer as NonNullable<LensPuckConfig<LensOutletProps>["render"]>,
};

// ── LensZone ───────────────────────────────────────────────────────────────
//
// A lens author who wants a named drop-zone inside their own panel calls
// `LensZone` instead of authoring raw Puck slot plumbing. The component
// accepts a `name` prop and delegates to Puck's own slot rendering.
//
// `LensZone` is intentionally the *only* primitive a lens needs to be
// Puck-embeddable; declaring `zones: ["main"]` on the bundle's puck
// config plus rendering `<LensZone name="main" />` somewhere inside the
// component is enough.

export interface LensZoneProps {
  name: string;
  children?: ReactNode;
  style?: CSSProperties;
  className?: string;
}

export function LensZone({ name, children, style, className }: LensZoneProps): ReactNode {
  return (
    <div
      data-lens-zone={name}
      className={className}
      style={{ minHeight: 24, ...style }}
    >
      {children}
    </div>
  );
}

// ── DEFAULT_STUDIO_SHELL_TREE ──────────────────────────────────────────────
//
// Baseline shell tree that reproduces the hand-coded `StudioShell`
// layout 1:1. App Profiles can override this by saving a custom tree
// into the kernel; Studio's shell-builder lens (Shift+H) edits *this*
// tree live.

/**
 * Build the default Studio shell tree. Returns a Puck `Data` with a `Shell`
 * root whose slots reference the ShellWidgetBundle names. Widget names are
 * PascalCase (matching `kebabToPascal(bundle.id)`).
 *
 * Callers can override by passing a different map of slot → widget list.
 */
export function createDefaultStudioShellTree(opts?: {
  widgetIds?: {
    activityBar?: string;
    objectExplorer?: string;
    componentPalette?: string;
    tabBar?: string;
    presenceIndicator?: string;
    undoStatusBar?: string;
    inspectorPanel?: string;
  };
}): Data {
  const ids = {
    activityBar: opts?.widgetIds?.activityBar ?? "ActivityBar",
    objectExplorer: opts?.widgetIds?.objectExplorer ?? "ObjectExplorer",
    componentPalette: opts?.widgetIds?.componentPalette ?? "ComponentPalette",
    tabBar: opts?.widgetIds?.tabBar ?? "TabBar",
    presenceIndicator: opts?.widgetIds?.presenceIndicator ?? "PresenceIndicator",
    undoStatusBar: opts?.widgetIds?.undoStatusBar ?? "UndoStatusBar",
    inspectorPanel: opts?.widgetIds?.inspectorPanel ?? "InspectorPanel",
  };
  let uid = 0;
  const next = (prefix: string): string => `${prefix}-${++uid}`;
  return {
    root: {
      props: {
        activityBarWidth: DEFAULT_SIZES.activityBarWidth,
        topBarHeight: DEFAULT_SIZES.topBarHeight,
        leftBarWidth: DEFAULT_SIZES.leftBarWidth,
        rightBarWidth: DEFAULT_SIZES.rightBarWidth,
        bottomBarHeight: DEFAULT_SIZES.bottomBarHeight,
        activityBar: [
          { type: ids.activityBar, props: { id: next("activity-bar") } },
        ],
        leftBar: [
          { type: ids.objectExplorer, props: { id: next("object-explorer") } },
          { type: ids.componentPalette, props: { id: next("component-palette") } },
        ],
        topBar: [
          { type: ids.tabBar, props: { id: next("tab-bar") } },
          { type: ids.presenceIndicator, props: { id: next("presence") } },
          { type: ids.undoStatusBar, props: { id: next("undo-status") } },
        ],
        main: [{ type: "LensOutlet", props: { id: next("lens-outlet") } }],
        rightBar: [
          { type: ids.inspectorPanel, props: { id: next("inspector") } },
        ],
        bottomBar: [],
      },
    },
    content: [],
  } as unknown as Data;
}

/** Eagerly-constructed default tree; convenient for tests and seeds. */
export const DEFAULT_STUDIO_SHELL_TREE: Data = createDefaultStudioShellTree();
