import type { ReactNode } from "react";

/**
 * A context-sensitive help entry.
 *
 * Packages register help entries into the HelpRegistry as an import
 * side-effect. HelpTooltip components throughout the workspace consume
 * them to show hover documentation and optional links to full docs.
 *
 * Ported from $legacy-inspiration-only/helm/components/src/help/types.ts
 * per ADR-005. The legacy `icon` field was a `ComponentType<{size,className}>`
 * tied to lucide-react; here it is a plain ReactNode so callers pass whatever
 * icon element they want (SVG, emoji, image) without pinning an icon library.
 */
export interface HelpEntry {
  /** Unique ID, e.g. `puck.components.record-list`, `puck.categories.layout`. */
  id: string;
  /** Short title shown in the tooltip header and search results. */
  title: string;
  /** Optional leading glyph rendered before the title. */
  icon?: ReactNode;
  /** 2–3 sentence summary shown in the hover tooltip. Plain text. */
  summary: string;
  /**
   * Logical path to the full documentation. The app-level `HelpProvider`
   * decides how to turn this into actual markdown — for Studio it is a
   * bundled map, for a server app it would be an HTTP fetch.
   */
  docPath?: string;
  /** Optional heading slug to scroll to within the doc. */
  docAnchor?: string;
}
