/**
 * Site Navigation panel (Tier 8D).
 *
 * Operates on the set of `page` objects in the collection: lists every
 * page, lets the author reorder / toggle visibility / pick a home page,
 * and exposes the resulting navigation structure for the `site-nav`
 * widget to consume via `buildSiteNav(kernel)`.
 *
 * Multi-page navigation also needs a way to navigate between pages in
 * preview mode — this panel is the single source of truth for what
 * shows up in that menu.
 */

import { useCallback, useMemo, useSyncExternalStore } from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
/** One flat row in the rendered site navigation. */
export interface SiteNavItem {
  id: string;
  label: string;
  slug: string;
  depth: number;
  isHome: boolean;
  hidden: boolean;
}

/** Collect every non-deleted page as a flat navigation list. */
export function buildSiteNav(pages: ReadonlyArray<GraphObject>): SiteNavItem[] {
  const sorted = pages
    .filter((p) => p.type === "page" && !p.deletedAt)
    .sort((a, b) => a.position - b.position);

  return sorted.map((p) => {
    const data = p.data as Record<string, unknown>;
    return {
      id: p.id as unknown as string,
      label: (data.title as string) || p.name,
      slug: (data.slug as string) || "",
      depth: 0,
      isHome: Boolean(data.isHome),
      hidden: Boolean(data.hiddenInNav),
    };
  });
}

export function SiteNavPanel() {
  const kernel = useKernel();

  const storeVersion = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects().length,
  );

  const items = useMemo(() => {
    void storeVersion;
    return buildSiteNav(kernel.store.allObjects());
  }, [kernel, storeVersion]);

  const setHome = useCallback(
    (id: string) => {
      for (const item of items) {
        const obj = kernel.store.getObject(item.id as unknown as ObjectId);
        if (!obj) continue;
        const nextHome = (obj.id as unknown as string) === id;
        if (((obj.data as Record<string, unknown>).isHome ?? false) !== nextHome) {
          kernel.updateObject(obj.id, {
            data: { ...obj.data, isHome: nextHome },
          });
        }
      }
    },
    [items, kernel],
  );

  const toggleHidden = useCallback(
    (id: string) => {
      const obj = kernel.store.getObject(id as unknown as ObjectId);
      if (!obj) return;
      const hiddenInNav = !((obj.data as Record<string, unknown>).hiddenInNav ?? false);
      kernel.updateObject(obj.id, {
        data: { ...obj.data, hiddenInNav },
      });
    },
    [kernel],
  );

  const move = useCallback(
    (id: string, direction: -1 | 1) => {
      const sorted = items;
      const idx = sorted.findIndex((o) => o.id === id);
      const target = idx + direction;
      if (target < 0 || target >= sorted.length) return;
      const targetItem = sorted[target];
      if (!targetItem) return;
      const current = kernel.store.getObject(id as unknown as ObjectId);
      const other = kernel.store.getObject(targetItem.id as unknown as ObjectId);
      if (!current || !other) return;
      kernel.updateObject(current.id, { position: other.position });
      kernel.updateObject(other.id, { position: current.position });
    },
    [items, kernel],
  );

  return (
    <div
      data-testid="site-nav-panel"
      style={{
        height: "100%",
        overflow: "auto",
        padding: 16,
        background: "#1e1e1e",
        color: "#ccc",
        fontSize: 12,
      }}
    >
      <h2 style={{ fontSize: 14, margin: 0, marginBottom: 12, color: "#e5e5e5" }}>
        Site Navigation
      </h2>
      <div style={{ color: "#888", fontSize: 11, marginBottom: 12 }}>
        Every <code>page</code> object in this vault. Reorder, toggle visibility, or
        pick the home page here — the <strong>site-nav</strong> widget mirrors this
        configuration automatically.
      </div>

      {items.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic" }}>
          No pages in this vault yet.
        </div>
      )}

      <ul style={{ margin: 0, padding: 0, listStyle: "none" }}>
        {items.map((item) => (
          <li
            key={item.id}
            data-testid={`site-nav-item-${item.id}`}
            style={{
              padding: "6px 8px",
              marginBottom: 4,
              background: "#252526",
              border: "1px solid #333",
              borderRadius: 3,
              display: "flex",
              alignItems: "center",
              gap: 6,
              opacity: item.hidden ? 0.5 : 1,
            }}
          >
            <span
              onClick={() => kernel.select(item.id as Parameters<typeof kernel.select>[0])}
              style={{ flex: 1, cursor: "pointer" }}
            >
              <strong>{item.label}</strong>
              {item.slug && (
                <span style={{ color: "#666", marginLeft: 6, fontSize: 10 }}>
                  /{item.slug}
                </span>
              )}
              {item.isHome && (
                <span style={{ color: "#4ade80", marginLeft: 6, fontSize: 10 }}>
                  HOME
                </span>
              )}
            </span>
            <button
              data-testid={`set-home-${item.id}`}
              onClick={() => setHome(item.id)}
              title="Set as home"
              style={iconBtn(item.isHome ? "#4ade80" : "#ccc")}
            >
              ⌂
            </button>
            <button
              data-testid={`toggle-hidden-${item.id}`}
              onClick={() => toggleHidden(item.id)}
              title={item.hidden ? "Show in nav" : "Hide from nav"}
              style={iconBtn("#ccc")}
            >
              {item.hidden ? "⦸" : "◉"}
            </button>
            <button
              data-testid={`nav-up-${item.id}`}
              onClick={() => move(item.id, -1)}
              style={iconBtn("#ccc")}
            >
              ↑
            </button>
            <button
              data-testid={`nav-down-${item.id}`}
              onClick={() => move(item.id, 1)}
              style={iconBtn("#ccc")}
            >
              ↓
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function iconBtn(color: string): React.CSSProperties {
  return {
    padding: "2px 6px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 2,
    color,
    cursor: "pointer",
  };
}


// ── Lens registration ──────────────────────────────────────────────────────

export const SITE_NAV_LENS_ID = lensId("site-nav");

export const siteNavLensManifest: LensManifest = {

  id: SITE_NAV_LENS_ID,
  name: "Site Navigation",
  icon: "\u{1F5FA}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-site-nav", name: "Switch to Site Navigation", shortcut: ["shift+n"], section: "Navigation" },
    ],
  },
};

export const siteNavLensBundle: LensBundle = defineLensBundle(
  siteNavLensManifest,
  SiteNavPanel,
);
