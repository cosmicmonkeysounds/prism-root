/**
 * ContributionRegistry — generic typed registry for the plugin contribution pattern.
 *
 * The single generalization of every "plugins declare, shell consumes" pattern.
 * Commands, views, keybindings, context menus, file handlers, settings pages,
 * toolbar items, status bar widgets — they ALL follow this exact shape:
 *
 *   1. Plugin declares:   plugin.contributes.X = [{ id, ...data }]
 *   2. Registry collects:  registry.registerAll(items, pluginId)
 *   3. Consumer queries:   registry.all() or registry.query(predicate)
 *
 * One class, parameterized by the item type, handles everything.
 */

export interface ContributionEntry<T> {
  item: T;
  pluginId: string;
}

export class ContributionRegistry<T> {
  private readonly _entries: ContributionEntry<T>[] = [];
  private readonly _byId = new Map<string, ContributionEntry<T>>();
  private readonly _keyFn: (item: T) => string;

  constructor(keyFn: (item: T) => string) {
    this._keyFn = keyFn;
  }

  register(item: T, pluginId: string): void {
    const entry: ContributionEntry<T> = { item, pluginId };
    const key = this._keyFn(item);
    this._byId.set(key, entry);
    this._entries.push(entry);
  }

  registerAll(items: T[] | undefined, pluginId: string): void {
    if (!items) return;
    for (const item of items) this.register(item, pluginId);
  }

  unregister(key: string): boolean {
    const entry = this._byId.get(key);
    if (!entry) return false;
    this._byId.delete(key);
    const idx = this._entries.indexOf(entry);
    if (idx !== -1) this._entries.splice(idx, 1);
    return true;
  }

  unregisterByPlugin(pluginId: string): number {
    const toRemove = this._entries.filter((e) => e.pluginId === pluginId);
    for (const entry of toRemove) {
      const key = this._keyFn(entry.item);
      this._byId.delete(key);
      const idx = this._entries.indexOf(entry);
      if (idx !== -1) this._entries.splice(idx, 1);
    }
    return toRemove.length;
  }

  get(key: string): T | undefined {
    return this._byId.get(key)?.item;
  }

  getEntry(key: string): ContributionEntry<T> | undefined {
    return this._byId.get(key);
  }

  has(key: string): boolean {
    return this._byId.has(key);
  }

  all(): T[] {
    return this._entries.map((e) => e.item);
  }

  allEntries(): ContributionEntry<T>[] {
    return [...this._entries];
  }

  byPlugin(pluginId: string): T[] {
    return this._entries
      .filter((e) => e.pluginId === pluginId)
      .map((e) => e.item);
  }

  query(predicate: (item: T) => boolean): T[] {
    return this._entries.filter((e) => predicate(e.item)).map((e) => e.item);
  }

  get size(): number {
    return this._entries.length;
  }

  clear(): void {
    this._entries.length = 0;
    this._byId.clear();
  }
}
