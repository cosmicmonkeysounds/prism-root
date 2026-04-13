/**
 * Wix / HotGlue style layout primitives.
 *
 * Structural building blocks a user drags in to assemble a full site. Each
 * shell exposes its regions as `ReactNode` slot props, which Puck fills with
 * nested drop zones. Empty slots render Puck's default empty-zone placeholder;
 * the kernel ↔ Puck sync in `layout-panel.tsx` persists children per-slot via
 * a `data.__slot` tag on the child object.
 */

import type { ReactNode } from "react";

// ── PageShell: header / sidebar / main / footer ─────────────────────────────

export interface PageShellProps {
  layout?: "sidebar-left" | "sidebar-right" | "stacked";
  sidebarWidth?: number;
  stickyHeader?: string | boolean;
  className?: string;
  header?: ReactNode;
  sidebar?: ReactNode;
  main?: ReactNode;
  footer?: ReactNode;
}

export function PageShellRenderer(props: PageShellProps): ReactNode {
  const {
    layout = "sidebar-left",
    sidebarWidth = 240,
    stickyHeader = true,
    className,
    header,
    sidebar,
    main,
    footer,
  } = props;
  const isStacked = layout === "stacked";
  const sidebarRight = layout === "sidebar-right";
  const sticky = stickyHeader === true || stickyHeader === "true";

  const gridStyle: React.CSSProperties = isStacked
    ? { display: "grid", gridTemplateRows: "auto 1fr auto", minHeight: "100%" }
    : {
        display: "grid",
        gridTemplateColumns: sidebarRight
          ? `1fr ${sidebarWidth}px`
          : `${sidebarWidth}px 1fr`,
        gridTemplateRows: "auto 1fr auto",
        gridTemplateAreas: sidebarRight
          ? '"header header" "main sidebar" "footer footer"'
          : '"header header" "sidebar main" "footer footer"',
        minHeight: 480,
      };

  return (
    <div
      data-testid="page-shell"
      data-layout={layout}
      className={
        className ??
        "overflow-hidden rounded-lg border border-slate-200 bg-white text-slate-900"
      }
      style={gridStyle}
    >
      <header
        className={
          sticky
            ? "border-b border-slate-200 bg-white/90 px-6 py-4 backdrop-blur"
            : "border-b border-slate-200 px-6 py-4"
        }
        style={isStacked ? undefined : { gridArea: "header" }}
        data-slot="header"
      >
        {header}
      </header>
      {!isStacked ? (
        <aside
          className="border-slate-200 bg-slate-50 p-4 text-sm text-slate-600"
          style={{
            gridArea: "sidebar",
            borderLeftWidth: sidebarRight ? 1 : 0,
            borderRightWidth: sidebarRight ? 0 : 1,
          }}
          data-slot="sidebar"
        >
          {sidebar}
        </aside>
      ) : null}
      <main
        className="p-6 text-slate-700"
        style={isStacked ? undefined : { gridArea: "main" }}
        data-slot="main"
      >
        {main}
      </main>
      <footer
        className="border-t border-slate-200 bg-slate-50 px-6 py-4 text-xs text-slate-500"
        style={isStacked ? undefined : { gridArea: "footer" }}
        data-slot="footer"
      >
        {footer}
      </footer>
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

// ── SideBar: standalone vertical column with a content slot ─────────────────

export interface SideBarProps {
  width?: number;
  position?: "left" | "right";
  className?: string;
  content?: ReactNode;
}

export function SideBarRenderer(props: SideBarProps): ReactNode {
  const { width = 260, position = "left", className, content } = props;
  return (
    <aside
      data-testid="side-bar"
      className={
        className ??
        `flex flex-col gap-3 p-4 ${position === "left" ? "border-r" : "border-l"} border-slate-200 bg-slate-50`
      }
      style={{ width, minWidth: width }}
      data-slot="content"
    >
      {content}
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
