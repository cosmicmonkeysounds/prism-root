/**
 * PluginRegistry — manages registered PrismPlugin instances.
 *
 * Mirrors the LensRegistry pattern: register/unregister/query with events.
 * The shell iterates all plugins at startup and auto-registers each
 * contribution into the appropriate ContributionRegistry.
 */

import type {
  PrismPlugin,
  PluginId,
  ViewContributionDef,
  CommandContributionDef,
  KeybindingContributionDef,
  ContextMenuContributionDef,
} from "./plugin-types.js";
import { ContributionRegistry } from "./contribution-registry.js";

// ── Event types ──────────────────────────────────────────────────────────────

export type PluginRegistryEventType = "registered" | "unregistered";

export interface PluginRegistryEvent {
  type: PluginRegistryEventType;
  pluginId: PluginId;
}

export type PluginRegistryListener = (event: PluginRegistryEvent) => void;

// ── PluginRegistry ───────────────────────────────────────────────────────────

export class PluginRegistry<T extends PrismPlugin = PrismPlugin> {
  private readonly plugins = new Map<string, T>();
  private readonly listeners: PluginRegistryListener[] = [];

  readonly views = new ContributionRegistry<ViewContributionDef>(
    (v) => v.id,
  );
  readonly commands = new ContributionRegistry<CommandContributionDef>(
    (c) => c.id,
  );
  readonly keybindings = new ContributionRegistry<KeybindingContributionDef>(
    (k) => `${k.command}:${k.key}`,
  );
  readonly contextMenus = new ContributionRegistry<ContextMenuContributionDef>(
    (m) => m.id,
  );

  register(plugin: T): () => void {
    this.plugins.set(plugin.id, plugin);

    if (plugin.contributes) {
      this.views.registerAll(plugin.contributes.views, plugin.id);
      this.commands.registerAll(plugin.contributes.commands, plugin.id);
      this.keybindings.registerAll(plugin.contributes.keybindings, plugin.id);
      this.contextMenus.registerAll(
        plugin.contributes.contextMenus,
        plugin.id,
      );
    }

    this.emit({ type: "registered", pluginId: plugin.id });

    return () => this.unregister(plugin.id);
  }

  unregister(id: string): boolean {
    const plugin = this.plugins.get(id);
    if (!plugin) return false;

    this.plugins.delete(id);
    this.views.unregisterByPlugin(id);
    this.commands.unregisterByPlugin(id);
    this.keybindings.unregisterByPlugin(id);
    this.contextMenus.unregisterByPlugin(id);

    this.emit({ type: "unregistered", pluginId: id as PluginId });
    return true;
  }

  get(id: string): T | undefined {
    return this.plugins.get(id);
  }

  has(id: string): boolean {
    return this.plugins.has(id);
  }

  all(): T[] {
    return [...this.plugins.values()];
  }

  get size(): number {
    return this.plugins.size;
  }

  subscribe(listener: PluginRegistryListener): () => void {
    this.listeners.push(listener);
    return () => {
      const idx = this.listeners.indexOf(listener);
      if (idx !== -1) this.listeners.splice(idx, 1);
    };
  }

  private emit(event: PluginRegistryEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }
}
