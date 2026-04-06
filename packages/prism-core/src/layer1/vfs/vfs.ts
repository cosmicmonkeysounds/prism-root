/**
 * @prism/core — Virtual File System
 *
 * Content-addressed blob storage decoupled from the Loro CRDT document.
 * Binary assets (images, video, audio, PDFs, etc.) are stored by SHA-256
 * hash and referenced from GraphObject.data via BinaryRef.
 *
 * Features:
 *   - Content-addressed deduplication (same content = same hash = one blob)
 *   - VfsAdapter abstraction for pluggable storage backends
 *   - Binary Forking Protocol: lock-based editing for non-mergeable files
 *   - importFile/exportFile for moving binaries in/out of vault storage
 *   - In-memory adapter for testing; local FS adapter via Tauri IPC (future)
 */

import type {
  BinaryRef,
  FileStat,
  BinaryLock,
  VfsAdapter,
  VfsManager,
  VfsManagerOptions,
} from "./vfs-types.js";

// ── SHA-256 hashing ─────────────────────────────────────────────────────────

function hexEncode(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) {
    hex += b.toString(16).padStart(2, "0");
  }
  return hex;
}

async function sha256(data: Uint8Array): Promise<string> {
  const hashBuffer = await globalThis.crypto.subtle.digest("SHA-256", data as unknown as BufferSource);
  return hexEncode(new Uint8Array(hashBuffer));
}

// ── Memory VFS Adapter ──────────────────────────────────────────────────────

interface StoredBlob {
  data: Uint8Array;
  stat: FileStat;
}

/**
 * In-memory VFS adapter for testing. All blobs live in a Map.
 */
export function createMemoryVfsAdapter(): VfsAdapter {
  const blobs = new Map<string, StoredBlob>();

  return {
    async read(hash: string): Promise<Uint8Array | null> {
      const blob = blobs.get(hash);
      return blob ? new Uint8Array(blob.data) : null;
    },

    async write(data: Uint8Array, mimeType: string): Promise<string> {
      const hash = await sha256(data);

      if (!blobs.has(hash)) {
        const now = new Date().toISOString();
        blobs.set(hash, {
          data: new Uint8Array(data),
          stat: {
            hash,
            size: data.length,
            mimeType,
            createdAt: now,
            modifiedAt: now,
          },
        });
      }

      return hash;
    },

    async stat(hash: string): Promise<FileStat | null> {
      const blob = blobs.get(hash);
      return blob ? { ...blob.stat } : null;
    },

    async list(): Promise<string[]> {
      return [...blobs.keys()];
    },

    async delete(hash: string): Promise<boolean> {
      return blobs.delete(hash);
    },

    async has(hash: string): Promise<boolean> {
      return blobs.has(hash);
    },

    async count(): Promise<number> {
      return blobs.size;
    },

    async totalSize(): Promise<number> {
      let total = 0;
      for (const blob of blobs.values()) {
        total += blob.stat.size;
      }
      return total;
    },
  };
}

// ── VFS Manager ─────────────────────────────────────────────────────────────

/**
 * Create a VFS manager for binary asset storage.
 *
 * Usage:
 *   const vfs = createVfsManager();
 *   const ref = await vfs.importFile(pngBytes, "photo.png", "image/png");
 *   // Store ref in GraphObject.data
 *   const bytes = await vfs.exportFile(ref);
 */
export function createVfsManager(options: VfsManagerOptions = {}): VfsManager {
  const adapter = options.adapter ?? createMemoryVfsAdapter();
  const locks = new Map<string, BinaryLock>();

  // ── Import / Export ───────────────────────────────────────────────────

  async function importFile(
    data: Uint8Array,
    filename: string,
    mimeType: string,
  ): Promise<BinaryRef> {
    const hash = await adapter.write(data, mimeType);

    return {
      hash,
      filename,
      mimeType,
      size: data.length,
      importedAt: new Date().toISOString(),
    };
  }

  async function exportFile(ref: BinaryRef): Promise<Uint8Array | null> {
    return adapter.read(ref.hash);
  }

  async function removeFile(hash: string): Promise<boolean> {
    if (locks.has(hash)) {
      throw new Error(`Cannot remove locked blob: ${hash}`);
    }
    return adapter.delete(hash);
  }

  // ── Locking (Binary Forking Protocol) ─────────────────────────────────

  function acquireLock(hash: string, peerId: string, reason?: string): BinaryLock {
    const existing = locks.get(hash);
    if (existing) {
      if (existing.lockedBy === peerId) return existing;
      throw new Error(
        `Blob ${hash} is already locked by ${existing.lockedBy}`,
      );
    }

    const lock: BinaryLock = {
      hash,
      lockedBy: peerId,
      lockedAt: new Date().toISOString(),
      ...(reason ? { reason } : {}),
    };
    locks.set(hash, lock);
    return lock;
  }

  function releaseLock(hash: string, peerId: string): void {
    const existing = locks.get(hash);
    if (!existing) {
      throw new Error(`Blob ${hash} is not locked`);
    }
    if (existing.lockedBy !== peerId) {
      throw new Error(
        `Blob ${hash} is locked by ${existing.lockedBy}, not ${peerId}`,
      );
    }
    locks.delete(hash);
  }

  function getLock(hash: string): BinaryLock | null {
    return locks.get(hash) ?? null;
  }

  function isLocked(hash: string): boolean {
    return locks.has(hash);
  }

  function listLocks(): BinaryLock[] {
    return [...locks.values()];
  }

  // ── Replace Locked File ───────────────────────────────────────────────

  async function replaceLockedFile(
    oldHash: string,
    newData: Uint8Array,
    filename: string,
    mimeType: string,
    peerId: string,
  ): Promise<BinaryRef> {
    const existing = locks.get(oldHash);
    if (!existing) {
      throw new Error(`Blob ${oldHash} is not locked`);
    }
    if (existing.lockedBy !== peerId) {
      throw new Error(
        `Blob ${oldHash} is locked by ${existing.lockedBy}, not ${peerId}`,
      );
    }

    // Write new content (old blob is kept for history)
    const newHash = await adapter.write(newData, mimeType);

    // Move lock from old hash to new hash
    locks.delete(oldHash);
    const newLock: BinaryLock = {
      hash: newHash,
      lockedBy: peerId,
      lockedAt: existing.lockedAt,
      ...(existing.reason ? { reason: existing.reason } : {}),
    };
    locks.set(newHash, newLock);

    return {
      hash: newHash,
      filename,
      mimeType,
      size: newData.length,
      importedAt: new Date().toISOString(),
    };
  }

  // ── Stat ──────────────────────────────────────────────────────────────

  async function stat(hash: string): Promise<FileStat | null> {
    return adapter.stat(hash);
  }

  // ── Dispose ───────────────────────────────────────────────────────────

  function dispose(): void {
    locks.clear();
  }

  return {
    adapter,
    importFile,
    exportFile,
    removeFile,
    acquireLock,
    releaseLock,
    getLock,
    isLocked,
    listLocks,
    replaceLockedFile,
    stat,
    dispose,
  };
}

// ── Utility: compute hash without storing ───────────────────────────────────

/**
 * Compute SHA-256 hash of data without storing it.
 * Useful for checking if a blob already exists before importing.
 */
export async function computeBinaryHash(data: Uint8Array): Promise<string> {
  return sha256(data);
}
