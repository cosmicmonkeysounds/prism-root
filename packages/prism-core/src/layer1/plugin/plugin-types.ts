/**
 * PrismPlugin — the universal extension unit.
 *
 * Every extension in the Prism ecosystem implements this interface.
 * The shell, build pipeline, and Lua runtime all discover
 * capabilities through plugins.
 *
 * All fields except `id` and `name` are optional. A module that only
 * contributes views and commands doesn't need to declare file types.
 */

// ── Plugin identity ──────────────────────────────────────────────────────────

export type PluginId = string & { readonly __brand?: "PluginId" };
export const pluginId = (id: string): PluginId => id as PluginId;

// ── View contributions ───────────────────────────────────────────────────────

export type ViewZone =
  | "left"
  | "right"
  | "top"
  | "bottom"
  | "content"
  | "floating"
  | "toolbar"
  | "activity-bar";

export interface ViewContributionDef {
  id: string;
  label: string;
  zone: ViewZone;
  componentId: string;
  icon?: unknown;
  defaultVisible?: boolean;
  description?: string;
  tags?: string[];
}

// ── Command contributions ────────────────────────────────────────────────────

export interface CommandContributionDef {
  id: string;
  label: string;
  category: string;
  shortcut?: string;
  description?: string;
  action: string;
  payload?: Record<string, unknown>;
  when?: string;
}

// ── Context menu contributions ──────────────────────────────────────────────

export interface ContextMenuContributionDef {
  id: string;
  label: string;
  context: string;
  when?: string;
  action: string;
  shortcut?: string;
  separatorBefore?: boolean;
  danger?: boolean;
}

// ── Keybinding contributions ─────────────────────────────────────────────────

export interface KeybindingContributionDef {
  command: string;
  key: string;
  when?: string;
}

// ── Activity bar contributions ───────────────────────────────────────────────

export interface ActivityBarContributionDef {
  id: string;
  label: string;
  icon?: unknown;
  position?: "top" | "bottom";
  priority?: number;
}

// ── Settings contributions ───────────────────────────────────────────────────

export interface SettingsContributionDef {
  id: string;
  label: string;
  componentId: string;
  order?: number;
}

// ── Toolbar contributions ────────────────────────────────────────────────────

export interface ToolbarContributionDef {
  id: string;
  position: "left" | "center" | "right";
  componentId: string;
  order?: number;
  when?: string;
}

// ── Status bar contributions ─────────────────────────────────────────────────

export interface StatusBarContributionDef {
  id: string;
  position: "left" | "right";
  componentId: string;
  order?: number;
}

// ── Weak-ref provider contributions ──────────────────────────────────────────

export interface WeakRefProviderContributionDef {
  id: string;
  label?: string;
  sourceTypes: string[];
}

// ── Unified contributions object ─────────────────────────────────────────────

export interface PluginContributions {
  views?: ViewContributionDef[];
  commands?: CommandContributionDef[];
  contextMenus?: ContextMenuContributionDef[];
  keybindings?: KeybindingContributionDef[];
  activityBar?: ActivityBarContributionDef[];
  settings?: SettingsContributionDef[];
  toolbar?: ToolbarContributionDef[];
  statusBar?: StatusBarContributionDef[];
  weakRefProviders?: WeakRefProviderContributionDef[];
  immersive?: boolean;
}

// ── The main plugin interface ────────────────────────────────────────────────

export interface PrismPlugin {
  readonly id: PluginId;
  readonly name: string;
  readonly icon?: unknown;

  readonly contributes?: PluginContributions;
  readonly requires?: PluginId[];
}
