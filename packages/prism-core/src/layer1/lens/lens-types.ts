/**
 * Lens type definitions — pure TypeScript, zero React.
 *
 * A Lens is Prism's universal extension unit. Each Lens registers
 * a manifest describing its identity, views, commands, and keybindings.
 * The shell renders lenses by looking up their components at runtime.
 */

/** Branded lens identifier. */
export type LensId = string & { readonly __brand: "LensId" };
export const lensId = (id: string): LensId => id as LensId;

/** Branded tab identifier. */
export type TabId = string & { readonly __brand: "TabId" };
export const tabId = (id: string): TabId => id as TabId;

/** Category for activity-bar grouping. */
export type LensCategory = "editor" | "visual" | "data" | "debug" | "custom" | "facet";

/** A command contribution from a lens. */
export interface LensCommand {
  id: string;
  name: string;
  shortcut?: string[];
  section?: string;
}

/** A keybinding contribution from a lens. */
export interface LensKeybinding {
  command: string;
  key: string;
  label?: string;
}

/** A view-slot contribution (where this lens can render). */
export interface LensView {
  slot: "main" | "sidebar" | "inspector";
  weight?: number;
}

/** Static manifest describing a lens. No React, no components. */
export interface LensManifest {
  id: LensId;
  name: string;
  icon: string;
  category: LensCategory;
  contributes: {
    views: LensView[];
    commands: LensCommand[];
    keybindings?: LensKeybinding[];
  };
}
