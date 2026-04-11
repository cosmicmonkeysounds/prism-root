/**
 * VaultDiscovery — filesystem scanning for .prism.json manifests.
 *
 * Provides a pluggable directory scanner that finds vault manifests,
 * parses them, and merges discovered vaults into a VaultRoster.
 *
 * Responsibilities:
 *   - Scan directories for MANIFEST_FILENAME files
 *   - Parse discovered manifests and build RosterEntry objects
 *   - Merge results into VaultRoster (add new, update existing)
 *   - Track scan state (scanning, idle, last scan time)
 *   - Emit discovery events for UI reactivity
 *
 * The actual filesystem I/O is abstracted behind DiscoveryAdapter,
 * which the host environment provides (Tauri IPC, Node fs, etc.).
 */

import { MANIFEST_FILENAME, parseManifest } from "@prism/core/manifest";
import type { PrismManifest } from "@prism/core/manifest";
import type { RosterEntry, VaultRoster } from "./vault-roster.js";

// ── Types ─────────────────────────────────────────────────────────────────────

/**
 * Pluggable filesystem I/O for manifest discovery.
 * Host environment provides the implementation.
 */
export interface DiscoveryAdapter {
  /**
   * List immediate subdirectories of a directory.
   * Returns absolute paths.
   */
  listDirectories(path: string): string[];

  /**
   * Read the contents of a file as a UTF-8 string.
   * Returns null if the file does not exist or is unreadable.
   */
  readFile(path: string): string | null;

  /**
   * Check if a file exists at the given path.
   */
  exists(path: string): boolean;

  /**
   * Join path segments (platform-aware).
   */
  joinPath(...segments: string[]): string;
}

/** Result of scanning a single directory for a manifest. */
export interface DiscoveredVault {
  /** Absolute path to the vault directory. */
  path: string;
  /** Parsed manifest. */
  manifest: PrismManifest;
}

export type DiscoveryEventType = "scan-start" | "scan-complete" | "vault-found" | "scan-error";

export interface DiscoveryEvent {
  type: DiscoveryEventType;
  /** For vault-found: the discovered vault. */
  vault?: DiscoveredVault;
  /** For scan-complete: all discovered vaults. */
  results?: DiscoveredVault[];
  /** For scan-error: the error message. */
  error?: string;
  /** For scan-complete: total directories scanned. */
  scannedCount?: number;
}

export type DiscoveryEventHandler = (event: DiscoveryEvent) => void;

export interface DiscoveryScanOptions {
  /** Directories to scan for vault subdirectories. */
  searchPaths: string[];
  /** Maximum directory depth to search. Default: 1 (immediate children only). */
  maxDepth?: number;
  /** Whether to merge discovered vaults into the roster automatically. Default: true. */
  mergeToRoster?: boolean;
}

// ── In-memory DiscoveryAdapter ───────────────────────────────────────────────

/**
 * In-memory DiscoveryAdapter for testing.
 * Populate via `addDirectory()` and `addFile()`.
 */
export interface MemoryDiscoveryAdapter extends DiscoveryAdapter {
  addDirectory(parentPath: string, dirName: string): void;
  addFile(path: string, content: string): void;
}

export function createMemoryDiscoveryAdapter(): MemoryDiscoveryAdapter {
  const files = new Map<string, string>();
  const directories = new Map<string, Set<string>>(); // parent → child dirs

  function normalizePath(p: string): string {
    // Ensure consistent path separators
    return p.replace(/\\/g, "/");
  }

  return {
    listDirectories(path: string): string[] {
      const norm = normalizePath(path);
      const children = directories.get(norm);
      if (!children) return [];
      return [...children].map((name) => `${norm}/${name}`).sort();
    },

    readFile(path: string): string | null {
      return files.get(normalizePath(path)) ?? null;
    },

    exists(path: string): boolean {
      return files.has(normalizePath(path));
    },

    joinPath(...segments: string[]): string {
      return normalizePath(segments.join("/"));
    },

    addDirectory(parentPath: string, dirName: string): void {
      const norm = normalizePath(parentPath);
      let children = directories.get(norm);
      if (!children) {
        children = new Set();
        directories.set(norm, children);
      }
      children.add(dirName);
    },

    addFile(path: string, content: string): void {
      files.set(normalizePath(path), content);
    },
  };
}

// ── VaultDiscovery ───────────────────────────────────────────────────────────

export interface VaultDiscovery {
  /** Scan directories for vault manifests. Returns discovered vaults. */
  scan(options: DiscoveryScanOptions): DiscoveredVault[];

  /** Whether a scan is currently in progress. */
  readonly scanning: boolean;

  /** ISO timestamp of the last completed scan. Null if never scanned. */
  readonly lastScanAt: string | null;

  /** Number of vaults found in the last scan. */
  readonly lastScanCount: number;

  /** Subscribe to discovery events. Returns unsubscribe function. */
  onEvent(handler: DiscoveryEventHandler): () => void;
}

export function createVaultDiscovery(
  adapter: DiscoveryAdapter,
  roster?: VaultRoster,
): VaultDiscovery {
  let isScanning = false;
  let lastScanAt: string | null = null;
  let lastScanCount = 0;
  const eventListeners = new Set<DiscoveryEventHandler>();

  function emit(event: DiscoveryEvent): void {
    for (const handler of eventListeners) {
      handler(event);
    }
  }

  function scanDirectory(
    dirPath: string,
    depth: number,
    maxDepth: number,
    results: DiscoveredVault[],
  ): void {
    // Check if this directory has a manifest
    const manifestPath = adapter.joinPath(dirPath, MANIFEST_FILENAME);
    if (adapter.exists(manifestPath)) {
      const content = adapter.readFile(manifestPath);
      if (content) {
        try {
          const manifest = parseManifest(content);
          const vault: DiscoveredVault = { path: dirPath, manifest };
          results.push(vault);
          emit({ type: "vault-found", vault });
        } catch {
          // Invalid manifest — skip silently
        }
      }
    }

    // Recurse into subdirectories if within depth limit
    if (depth < maxDepth) {
      try {
        const subdirs = adapter.listDirectories(dirPath);
        for (const subdir of subdirs) {
          scanDirectory(subdir, depth + 1, maxDepth, results);
        }
      } catch {
        // Directory listing failed — skip
      }
    }
  }

  function manifestToRosterEntry(
    vault: DiscoveredVault,
  ): Omit<RosterEntry, "addedAt"> & { addedAt?: string } {
    const m = vault.manifest;
    const entry: Omit<RosterEntry, "addedAt"> & { addedAt?: string } = {
      id: m.id,
      name: m.name,
      path: vault.path,
      lastOpenedAt: m.lastOpenedAt ?? m.createdAt,
      pinned: false,
      collectionCount: m.collections?.length ?? 0,
    };
    if (m.description !== undefined) entry.description = m.description;
    if (m.visibility !== undefined) entry.visibility = m.visibility;
    return entry;
  }

  function scan(options: DiscoveryScanOptions): DiscoveredVault[] {
    isScanning = true;
    emit({ type: "scan-start" });

    const maxDepth = options.maxDepth ?? 1;
    const mergeToRoster = options.mergeToRoster ?? true;
    const results: DiscoveredVault[] = [];
    let scannedCount = 0;

    try {
      for (const searchPath of options.searchPaths) {
        // Scan immediate children of each search path
        try {
          const dirs = adapter.listDirectories(searchPath);
          scannedCount += dirs.length;
          for (const dir of dirs) {
            scanDirectory(dir, 1, maxDepth, results);
          }
        } catch {
          emit({ type: "scan-error", error: `Failed to list: ${searchPath}` });
        }

        // Also check the search path itself for a manifest
        const manifestPath = adapter.joinPath(searchPath, MANIFEST_FILENAME);
        if (adapter.exists(manifestPath)) {
          const content = adapter.readFile(manifestPath);
          if (content) {
            try {
              const manifest = parseManifest(content);
              const vault: DiscoveredVault = { path: searchPath, manifest };
              // Avoid duplicates
              if (!results.some((r) => r.path === searchPath)) {
                results.push(vault);
                emit({ type: "vault-found", vault });
              }
            } catch {
              // Invalid manifest
            }
          }
        }
        scannedCount++;
      }

      // Merge into roster if requested
      if (mergeToRoster && roster) {
        for (const vault of results) {
          const existing = roster.getByPath(vault.path);
          if (existing) {
            // Update name/description/collectionCount from manifest
            const patch: Record<string, unknown> = {
              name: vault.manifest.name,
              collectionCount: vault.manifest.collections?.length ?? 0,
            };
            if (vault.manifest.description !== undefined) {
              patch.description = vault.manifest.description;
            }
            if (vault.manifest.visibility !== undefined) {
              patch.visibility = vault.manifest.visibility;
            }
            roster.update(existing.id, patch);
          } else {
            roster.add(manifestToRosterEntry(vault));
          }
        }
      }

      lastScanAt = new Date().toISOString();
      lastScanCount = results.length;
      isScanning = false;

      emit({ type: "scan-complete", results, scannedCount });
    } catch (err) {
      isScanning = false;
      emit({
        type: "scan-error",
        error: err instanceof Error ? err.message : String(err),
      });
    }

    return results;
  }

  return {
    scan,
    get scanning() {
      return isScanning;
    },
    get lastScanAt() {
      return lastScanAt;
    },
    get lastScanCount() {
      return lastScanCount;
    },
    onEvent(handler: DiscoveryEventHandler): () => void {
      eventListeners.add(handler);
      return () => {
        eventListeners.delete(handler);
      };
    },
  };
}
