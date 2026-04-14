import type { HelpEntry } from "./types.js";

const registry = new Map<string, HelpEntry>();

/**
 * Global help content registry.
 *
 * Any package or module registers entries here. Entries are consumed by
 * HelpTooltip / DocSearch throughout the workspace. Registration happens
 * as a side-effect of importing the package's entries module — no runtime
 * setup, no DI wiring.
 *
 * @example
 *   HelpRegistry.registerMany([
 *     { id: 'puck.components.record-list', title: 'Record List',
 *       summary: 'Queries kernel records by type...' },
 *   ]);
 */
export const HelpRegistry = {
  register(entry: HelpEntry): void {
    registry.set(entry.id, entry);
  },

  registerMany(entries: readonly HelpEntry[]): void {
    for (const entry of entries) registry.set(entry.id, entry);
  },

  get(id: string): HelpEntry | undefined {
    return registry.get(id);
  },

  getAll(): HelpEntry[] {
    return [...registry.values()];
  },

  /** Remove all registered entries. Useful for testing. */
  clear(): void {
    registry.clear();
  },

  /**
   * Search entries by title and summary (case-insensitive substring
   * match). Returns entries where *every* whitespace-separated word in the
   * query appears somewhere in `title + " " + summary`. Empty query
   * returns an empty array.
   */
  search(query: string): HelpEntry[] {
    const q = query.toLowerCase().trim();
    if (!q) return [];
    const words = q.split(/\s+/);
    return [...registry.values()].filter((entry) => {
      const haystack = `${entry.title} ${entry.summary}`.toLowerCase();
      return words.every((w) => haystack.includes(w));
    });
  },
};
