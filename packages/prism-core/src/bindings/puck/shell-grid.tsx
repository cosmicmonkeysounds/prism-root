/**
 * Shell grid primitive.
 *
 * Pure grid math + a React component that lays out the six-region shell
 * used by every Prism Shell (Studio chrome, page-builder pages, custom
 * user-authored shells). Exported so users can build their own shell
 * without importing the full `Shell` Puck component if all they need is
 * the grid behaviour.
 *
 *   ┌────┬─────────────────────────────┐
 *   │    │ top                         │
 *   │ AB ├────┬───────────────┬────────┤
 *   │    │ lf │     main      │   rt   │
 *   │    ├────┴───────────────┴────────┤
 *   │    │ bottom                      │
 *   └────┴─────────────────────────────┘
 *
 * When a bar has no content its row/column collapses to 0px.
 */

import type { CSSProperties, ReactNode } from "react";

export interface ShellBarSizes {
  activityBarWidth?: number;
  topBarHeight?: number;
  leftBarWidth?: number;
  rightBarWidth?: number;
  bottomBarHeight?: number;
}

export interface ShellGridOpts extends ShellBarSizes {
  hasActivityBar: boolean;
  hasTopBar: boolean;
  hasLeftBar: boolean;
  hasRightBar: boolean;
  hasBottomBar: boolean;
}

export interface ShellGridTemplate {
  gridTemplateColumns: string;
  gridTemplateRows: string;
  gridTemplateAreas: string;
}

const MIN_BAR = 0;
const MAX_BAR = 4000;

export function clampBar(value: number | undefined): number {
  if (value === undefined || !Number.isFinite(value)) return 0;
  if (value < MIN_BAR) return MIN_BAR;
  if (value > MAX_BAR) return MAX_BAR;
  return Math.round(value);
}

/**
 * Pure helper: compute CSS grid template strings for a six-region shell.
 * Returns a drop-in for `style`, so tests can assert against it without
 * rendering React.
 */
export function computeShellGrid(opts: ShellGridOpts): ShellGridTemplate {
  const ab = opts.hasActivityBar ? `${clampBar(opts.activityBarWidth)}px` : "0px";
  const top = opts.hasTopBar ? `${clampBar(opts.topBarHeight)}px` : "0px";
  const lf = opts.hasLeftBar ? `${clampBar(opts.leftBarWidth)}px` : "0px";
  const rt = opts.hasRightBar ? `${clampBar(opts.rightBarWidth)}px` : "0px";
  const bt = opts.hasBottomBar ? `${clampBar(opts.bottomBarHeight)}px` : "0px";
  return {
    gridTemplateColumns: `${ab} ${lf} 1fr ${rt}`,
    gridTemplateRows: `${top} 1fr ${bt}`,
    gridTemplateAreas: [
      '"activity top top top"',
      '"activity left main right"',
      '"activity bottom bottom bottom"',
    ].join(" "),
  };
}

export interface ShellGridProps extends ShellBarSizes {
  activityBar?: ReactNode;
  topBar?: ReactNode;
  leftBar?: ReactNode;
  main?: ReactNode;
  rightBar?: ReactNode;
  bottomBar?: ReactNode;
  className?: string;
  style?: CSSProperties;
  /** When true, fills the viewport (100vh). Default true. */
  fullscreen?: boolean;
  /** Per-cell style overrides (applied on top of the defaults). */
  cellStyles?: Partial<
    Record<
      "activity" | "top" | "left" | "main" | "right" | "bottom",
      CSSProperties
    >
  >;
  /** Optional test id on the root. */
  testId?: string;
  /** Children slot for arbitrary overlays (resize handles etc.). */
  overlays?: ReactNode;
}

function hasContent(n: ReactNode): boolean {
  return n !== null && n !== undefined && n !== false && n !== true;
}

/**
 * React component: renders the shell grid given slot nodes and per-bar
 * sizes. Purely visual — it does not know about resize handles or Puck.
 * Compose with `useResizeHandle` + `<ResizeHandle>` for an editable shell.
 */
export function ShellGrid(props: ShellGridProps): ReactNode {
  const {
    activityBar,
    topBar,
    leftBar,
    main,
    rightBar,
    bottomBar,
    activityBarWidth,
    topBarHeight,
    leftBarWidth,
    rightBarWidth,
    bottomBarHeight,
    className,
    style,
    fullscreen = true,
    cellStyles,
    testId = "shell-grid",
    overlays,
  } = props;

  const grid = computeShellGrid({
    ...(activityBarWidth !== undefined ? { activityBarWidth } : {}),
    ...(topBarHeight !== undefined ? { topBarHeight } : {}),
    ...(leftBarWidth !== undefined ? { leftBarWidth } : {}),
    ...(rightBarWidth !== undefined ? { rightBarWidth } : {}),
    ...(bottomBarHeight !== undefined ? { bottomBarHeight } : {}),
    hasActivityBar: hasContent(activityBar),
    hasTopBar: hasContent(topBar),
    hasLeftBar: hasContent(leftBar),
    hasRightBar: hasContent(rightBar),
    hasBottomBar: hasContent(bottomBar),
  });

  const rootStyle: CSSProperties = {
    display: "grid",
    position: "relative",
    ...(fullscreen
      ? { height: "100vh", width: "100%" }
      : { minHeight: 480, width: "100%" }),
    ...grid,
    ...style,
  };

  return (
    <div data-testid={testId} className={className} style={rootStyle}>
      {hasContent(activityBar) ? (
        <div
          data-slot="activityBar"
          style={{ gridArea: "activity", overflow: "hidden", ...cellStyles?.activity }}
        >
          {activityBar}
        </div>
      ) : null}
      {hasContent(topBar) ? (
        <div
          data-slot="topBar"
          style={{ gridArea: "top", overflow: "hidden", ...cellStyles?.top }}
        >
          {topBar}
        </div>
      ) : null}
      {hasContent(leftBar) ? (
        <div
          data-slot="leftBar"
          style={{ gridArea: "left", overflow: "hidden", ...cellStyles?.left }}
        >
          {leftBar}
        </div>
      ) : null}
      <div
        data-slot="main"
        style={{
          gridArea: "main",
          overflow: "hidden",
          minWidth: 0,
          minHeight: 0,
          ...cellStyles?.main,
        }}
      >
        {main}
      </div>
      {hasContent(rightBar) ? (
        <div
          data-slot="rightBar"
          style={{ gridArea: "right", overflow: "hidden", ...cellStyles?.right }}
        >
          {rightBar}
        </div>
      ) : null}
      {hasContent(bottomBar) ? (
        <div
          data-slot="bottomBar"
          style={{ gridArea: "bottom", overflow: "hidden", ...cellStyles?.bottom }}
        >
          {bottomBar}
        </div>
      ) : null}
      {overlays}
    </div>
  );
}
