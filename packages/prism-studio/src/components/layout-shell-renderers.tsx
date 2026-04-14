/**
 * Wix / HotGlue style layout primitives.
 *
 * Structural building blocks a user drags in to assemble a full site. Each
 * shell exposes its regions as `ReactNode` slot props, which Puck fills with
 * nested drop zones. Empty slots render Puck's default empty-zone placeholder;
 * the kernel ↔ Puck sync in `layout-panel.tsx` persists children per-slot via
 * a `data.__slot` tag on the child object.
 *
 * The grid math + drag-to-resize primitives live in `@prism/core/puck`
 * (`ShellGrid`, `useResizeHandle`, `ResizeHandle`) so a user can build their
 * own custom shell shape without importing the whole `Shell` Puck component.
 * PageShellRenderer and AppShellRenderer are thin presets over `ShellGrid`
 * that preserve the two user-visible palette entries: the page-shell with
 * top/left/right/bottom bars and the app-shell with the same plus a brand
 * affordance merged into the top bar.
 */

import { useCallback, useEffect, type CSSProperties, type ReactNode } from "react";
import { ShellGrid, useResizeHandle, ResizeHandle } from "@prism/core/puck";

// ── PageShell: four resizable bars wrapping a main canvas ───────────────────

export type BarDimensionKey =
  | "topBarHeight"
  | "leftBarWidth"
  | "rightBarWidth"
  | "bottomBarHeight";

export interface PageShellProps {
  topBarHeight?: number;
  leftBarWidth?: number;
  rightBarWidth?: number;
  bottomBarHeight?: number;
  stickyTopBar?: string | boolean;
  className?: string;
  topBar?: ReactNode;
  leftBar?: ReactNode;
  main?: ReactNode;
  rightBar?: ReactNode;
  bottomBar?: ReactNode;
  /** Commit callback fired on pointerup after a resize drag settles. */
  onCommit?: (key: BarDimensionKey, value: number) => void;
}

function hasContent(node: ReactNode): boolean {
  if (node === null || node === undefined || node === false || node === true) return false;
  if (typeof node === "string" || typeof node === "number") return String(node).length > 0;
  if (Array.isArray(node)) return node.some(hasContent);
  return true;
}

export function PageShellRenderer(props: PageShellProps): ReactNode {
  const {
    topBarHeight: topBarHeightProp = 0,
    leftBarWidth: leftBarWidthProp = 0,
    rightBarWidth: rightBarWidthProp = 0,
    bottomBarHeight: bottomBarHeightProp = 0,
    stickyTopBar = true,
    className,
    topBar,
    leftBar,
    main,
    rightBar,
    bottomBar,
    onCommit,
  } = props;

  const hasTop = hasContent(topBar);
  const hasLeft = hasContent(leftBar);
  const hasRight = hasContent(rightBar);
  const hasBottom = hasContent(bottomBar);
  const sticky = stickyTopBar === true || stickyTopBar === "true";

  const commit = useCallback(
    (key: BarDimensionKey) => (value: number) => onCommit?.(key, value),
    [onCommit],
  );

  const top = useResizeHandle(topBarHeightProp, "y", 1, commit("topBarHeight"));
  const left = useResizeHandle(leftBarWidthProp, "x", 1, commit("leftBarWidth"));
  const right = useResizeHandle(rightBarWidthProp, "x", -1, commit("rightBarWidth"));
  const bottom = useResizeHandle(bottomBarHeightProp, "y", -1, commit("bottomBarHeight"));

  useEffect(() => { top.setValueFromProps(topBarHeightProp); }, [topBarHeightProp, top]);
  useEffect(() => { left.setValueFromProps(leftBarWidthProp); }, [leftBarWidthProp, left]);
  useEffect(() => { right.setValueFromProps(rightBarWidthProp); }, [rightBarWidthProp, right]);
  useEffect(() => { bottom.setValueFromProps(bottomBarHeightProp); }, [bottomBarHeightProp, bottom]);

  const topBarNode = hasTop ? (
    <header
      className={
        sticky
          ? "relative border-b border-slate-200 bg-white/90 backdrop-blur h-full w-full"
          : "relative border-b border-slate-200 h-full w-full"
      }
      data-slot="topBar"
    >
      {topBar}
      {onCommit ? (
        <ResizeHandle
          orientation="vertical"
          active={top.dragging}
          onPointerDown={top.onPointerDown}
          style={{ left: 0, right: 0, bottom: 0, height: 6 }}
        />
      ) : null}
    </header>
  ) : null;

  const leftBarNode = hasLeft ? (
    <aside
      className="relative border-r border-slate-200 bg-slate-50 h-full w-full"
      data-slot="leftBar"
    >
      {leftBar}
      {onCommit ? (
        <ResizeHandle
          orientation="horizontal"
          active={left.dragging}
          onPointerDown={left.onPointerDown}
          style={{ top: 0, bottom: 0, right: 0, width: 6 }}
        />
      ) : null}
    </aside>
  ) : null;

  const rightBarNode = hasRight ? (
    <aside
      className="relative border-l border-slate-200 bg-slate-50 h-full w-full"
      data-slot="rightBar"
    >
      {rightBar}
      {onCommit ? (
        <ResizeHandle
          orientation="horizontal"
          active={right.dragging}
          onPointerDown={right.onPointerDown}
          style={{ top: 0, bottom: 0, left: 0, width: 6 }}
        />
      ) : null}
    </aside>
  ) : null;

  const bottomBarNode = hasBottom ? (
    <footer
      className="relative border-t border-slate-200 bg-slate-50 text-xs text-slate-500 h-full w-full"
      data-slot="bottomBar"
    >
      {bottomBar}
      {onCommit ? (
        <ResizeHandle
          orientation="vertical"
          active={bottom.dragging}
          onPointerDown={bottom.onPointerDown}
          style={{ left: 0, right: 0, top: 0, height: 6 }}
        />
      ) : null}
    </footer>
  ) : null;

  const mainNode = (
    <main className="relative h-full w-full" data-slot="main" style={{ overflow: "auto" }}>
      {main}
    </main>
  );

  return (
    <div
      data-testid="page-shell"
      data-has-top={hasTop || undefined}
      data-has-left={hasLeft || undefined}
      data-has-right={hasRight || undefined}
      data-has-bottom={hasBottom || undefined}
      className={
        className ??
        "relative overflow-hidden rounded-lg border border-slate-200 bg-white text-slate-900"
      }
    >
      <ShellGrid
        fullscreen={false}
        topBarHeight={top.value}
        leftBarWidth={left.value}
        rightBarWidth={right.value}
        bottomBarHeight={bottom.value}
        topBar={topBarNode}
        leftBar={leftBarNode}
        main={mainNode}
        rightBar={rightBarNode}
        bottomBar={bottomBarNode}
        cellStyles={{
          top: { position: "relative" },
          left: { position: "relative" },
          right: { position: "relative" },
          bottom: { position: "relative" },
          main: { position: "relative" },
        }}
      />
    </div>
  );
}

// ── AppShell: outer chrome wrapping every route in a Prism App ──────────────
//
// Structurally a PageShellRenderer with a different visual tag and a brand
// affordance. App Shells live one level up from Page Shells: a Prism App's
// App Shell wraps the currently-routed page's Page Shell. Same four-bar
// slot shape so the existing `SHELL_SLOTS` / `kernelToPuckData` machinery
// treats it identically — the kernel diff in `layout-panel.tsx` just needs
// to know the type name.

export interface AppShellProps extends PageShellProps {
  brand?: string;
  brandIcon?: string;
  showsActiveRoute?: string | boolean;
}

export function AppShellRenderer(props: AppShellProps): ReactNode {
  const { brand, brandIcon, showsActiveRoute, ...shellProps } = props;
  void showsActiveRoute;
  const topBarContent =
    hasContent(shellProps.topBar) || brand || brandIcon ? (
      <div className="flex h-full w-full items-center gap-3 px-4">
        {brandIcon ? (
          <span aria-hidden className="text-lg">
            {brandIcon}
          </span>
        ) : null}
        {brand ? (
          <span className="text-sm font-semibold tracking-tight text-slate-900">
            {brand}
          </span>
        ) : null}
        <div className="flex-1" data-slot="topBar">
          {shellProps.topBar}
        </div>
      </div>
    ) : (
      shellProps.topBar
    );
  return (
    <div data-testid="app-shell" className="relative">
      <PageShellRenderer
        {...shellProps}
        topBar={topBarContent}
        className={
          shellProps.className ??
          "relative overflow-hidden rounded-lg border border-purple-300 bg-white text-slate-900 shadow-sm"
        }
      />
    </div>
  );
}

// ── SiteHeader: branded horizontal bar with logo + nav slot ─────────────────

export interface SiteHeaderProps {
  brand?: string;
  tagline?: string;
  sticky?: string | boolean;
  className?: string;
  nav?: ReactNode;
}

export function SiteHeaderRenderer(props: SiteHeaderProps): ReactNode {
  const { brand = "Your Brand", tagline, sticky, className, nav } = props;
  const isSticky = sticky === true || sticky === "true";
  return (
    <header
      data-testid="site-header"
      className={
        className ??
        `flex items-center justify-between gap-6 border-b border-slate-200 bg-white px-6 py-4 ${isSticky ? "sticky top-0 z-10" : ""}`
      }
    >
      <div className="flex flex-col">
        <span className="text-lg font-semibold text-slate-900">{brand}</span>
        {tagline ? (
          <span className="text-xs text-slate-500">{tagline}</span>
        ) : null}
      </div>
      <nav className="flex items-center gap-4" data-slot="nav">
        {nav}
      </nav>
    </header>
  );
}

// ── SiteFooter: three-column link footer with per-column slots ──────────────

export interface SiteFooterProps {
  copyright?: string;
  className?: string;
  col1?: ReactNode;
  col2?: ReactNode;
  col3?: ReactNode;
}

export function SiteFooterRenderer(props: SiteFooterProps): ReactNode {
  const { copyright = "© Your Brand", className, col1, col2, col3 } = props;
  return (
    <footer
      data-testid="site-footer"
      className={
        className ??
        "border-t border-slate-800 bg-slate-900 px-6 py-10 text-slate-300"
      }
    >
      <div className="grid grid-cols-1 gap-8 md:grid-cols-3">
        <div data-slot="col1">{col1}</div>
        <div data-slot="col2">{col2}</div>
        <div data-slot="col3">{col3}</div>
      </div>
      <div className="mt-8 border-t border-slate-800 pt-6 text-center text-xs text-slate-500">
        {copyright}
      </div>
    </footer>
  );
}

// ── SideBar: standalone resizable bar with a content slot ───────────────────

export interface SideBarProps {
  width?: number;
  position?: "left" | "right" | "top" | "bottom";
  className?: string;
  content?: ReactNode;
  onCommit?: (value: number) => void;
}

export function SideBarRenderer(props: SideBarProps): ReactNode {
  const { width: widthProp = 260, position = "left", className, content, onCommit } = props;
  const isHorizontalBar = position === "top" || position === "bottom";
  const axis: "x" | "y" = isHorizontalBar ? "y" : "x";
  const direction: 1 | -1 = position === "left" || position === "top" ? 1 : -1;
  const handle = useResizeHandle(widthProp, axis, direction, onCommit);
  useEffect(() => { handle.setValueFromProps(widthProp); }, [widthProp, handle]);

  const baseClass =
    position === "left"
      ? "border-r"
      : position === "right"
        ? "border-l"
        : position === "top"
          ? "border-b"
          : "border-t";

  const sizeStyle: CSSProperties = isHorizontalBar
    ? { height: handle.value, minHeight: handle.value }
    : { width: handle.value, minWidth: handle.value };

  const handleStyle: CSSProperties =
    position === "left"
      ? { top: 0, bottom: 0, right: 0, width: 6 }
      : position === "right"
        ? { top: 0, bottom: 0, left: 0, width: 6 }
        : position === "top"
          ? { left: 0, right: 0, bottom: 0, height: 6 }
          : { left: 0, right: 0, top: 0, height: 6 };

  return (
    <aside
      data-testid="side-bar"
      data-position={position}
      className={
        className ??
        `relative flex flex-col gap-3 p-4 ${baseClass} border-slate-200 bg-slate-50`
      }
      style={{ position: "relative", ...sizeStyle }}
      data-slot="content"
    >
      {content}
      <ResizeHandle
        orientation={isHorizontalBar ? "vertical" : "horizontal"}
        active={handle.dragging}
        onPointerDown={handle.onPointerDown}
        style={handleStyle}
      />
    </aside>
  );
}

// ── NavBar: horizontal nav menu populated by a slot ─────────────────────────

export interface NavBarProps {
  align?: "start" | "center" | "end";
  className?: string;
  links?: ReactNode;
}

export function NavBarRenderer(props: NavBarProps): ReactNode {
  const { align = "start", className, links } = props;
  const alignClass =
    align === "center"
      ? "justify-center"
      : align === "end"
        ? "justify-end"
        : "justify-start";
  return (
    <nav
      data-testid="nav-bar"
      className={
        className ??
        `flex w-full items-center gap-4 border-b border-slate-200 bg-white px-6 py-3 ${alignClass}`
      }
      data-slot="links"
    >
      {links}
    </nav>
  );
}

// ── Hero: large banner whose body is a drop zone ────────────────────────────

export interface HeroProps {
  align?: "left" | "center" | "right";
  minHeight?: number;
  backgroundImage?: string;
  className?: string;
  content?: ReactNode;
}

export function HeroRenderer(props: HeroProps): ReactNode {
  const {
    align = "center",
    minHeight = 360,
    backgroundImage,
    className,
    content,
  } = props;
  const alignClass =
    align === "left"
      ? "text-left items-start"
      : align === "right"
        ? "text-right items-end"
        : "text-center items-center";
  return (
    <section
      data-testid="hero"
      className={
        className ??
        `relative flex w-full flex-col justify-center gap-4 overflow-hidden bg-slate-900 p-12 text-white ${alignClass}`
      }
      style={{
        minHeight,
        ...(backgroundImage
          ? {
              backgroundImage: `url(${backgroundImage})`,
              backgroundSize: "cover",
              backgroundPosition: "center",
            }
          : undefined),
      }}
      data-slot="content"
    >
      {content}
    </section>
  );
}

