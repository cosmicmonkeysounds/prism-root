/**
 * @prism/core — Virtual File System Types
 *
 * Decouples the object-graph (text/CRDTs) from heavy binary assets.
 * Binary blobs are content-addressed by SHA-256 hash and stored separately
 * from the Loro CRDT document. Non-mergeable files use lock-based editing
 * via the Binary Forking Protocol.
 */

// ── Content Addressing ──────────────────────────────────────────────────────

/**
 * Content-addressed reference to a binary blob.
 * Stored in GraphObject.data to link CRDT nodes to binary assets.
 */
export interface BinaryRef {
  /** SHA-256 hash of the blob content (hex-encoded). */
  hash: string;
  /** Original filename (for display/export). */
  filename: string;
  /** MIME type. */
  mimeType: string;
  /** Size in bytes. */
  size: number;
  /** ISO-8601 timestamp when the blob was imported. */
  importedAt: string;
}

// ── File Stat ───────────────────────────────────────────────────────────────

export interface FileStat {
  /** Content-addressed hash. */
  hash: string;
  /** Size in bytes. */
  size: number;
  /** MIME type. */
  mimeType: string;
  /** ISO-8601 creation timestamp. */
  createdAt: string;
  /** ISO-8601 last-modified timestamp. */
  modifiedAt: string;
}

// ── Binary Forking Protocol (Locking) ───────────────────────────────────────

/** Lock state for non-mergeable binary files. */
export interface BinaryLock {
  /** Hash of the locked blob. */
  hash: string;
  /** DID or peer ID of the lock holder. */
  lockedBy: string;
  /** ISO-8601 timestamp when the lock was acquired. */
  lockedAt: string;
  /** Optional reason/description. */
  reason?: string;
}

// ── VFS Adapter ─────────────────────────────────────────────────────────────

/**
 * Abstract file I/O interface. Implementations:
 * - createMemoryVfsAdapter() — in-memory for testing
 * - createLocalVfsAdapter() — local filesystem via Tauri IPC (future)
 */
export interface VfsAdapter {
  /** Read a blob by its content hash. Returns null if not found. */
  read(hash: string): Promise<Uint8Array | null>;
  /** Write a blob. Returns the content-addressed hash. */
  write(data: Uint8Array, mimeType: string): Promise<string>;
  /** Get file metadata by hash. Returns null if not found. */
  stat(hash: string): Promise<FileStat | null>;
  /** List all stored blob hashes. */
  list(): Promise<string[]>;
  /** Delete a blob by hash. Returns true if it existed. */
  delete(hash: string): Promise<boolean>;
  /** Check if a blob exists. */
  has(hash: string): Promise<boolean>;
  /** Total number of stored blobs. */
  count(): Promise<number>;
  /** Total size of all stored blobs in bytes. */
  totalSize(): Promise<number>;
}

// ── VFS Manager ─────────────────────────────────────────────────────────────

export interface VfsManager {
  /** The underlying storage adapter. */
  readonly adapter: VfsAdapter;

  /**
   * Import a file into vault storage. Returns a BinaryRef for storing
   * in GraphObject.data. Deduplicates: if the content already exists
   * (same hash), returns a ref to the existing blob.
   */
  importFile(
    data: Uint8Array,
    filename: string,
    mimeType: string,
  ): Promise<BinaryRef>;

  /**
   * Export a file from vault storage by its BinaryRef.
   * Returns the raw bytes, or null if not found.
   */
  exportFile(ref: BinaryRef): Promise<Uint8Array | null>;

  /** Remove a blob from storage. Returns true if it existed. */
  removeFile(hash: string): Promise<boolean>;

  /**
   * Acquire an exclusive lock on a binary blob for editing.
   * Non-mergeable files (images, video) must be locked before modification.
   * Throws if already locked by another peer.
   */
  acquireLock(hash: string, peerId: string, reason?: string): BinaryLock;

  /** Release a lock. Throws if not locked or locked by a different peer. */
  releaseLock(hash: string, peerId: string): void;

  /** Get the current lock on a blob, or null if unlocked. */
  getLock(hash: string): BinaryLock | null;

  /** Check if a blob is currently locked. */
  isLocked(hash: string): boolean;

  /** List all active locks. */
  listLocks(): BinaryLock[];

  /**
   * Replace a locked blob with new content (the "fork" in Binary Forking Protocol).
   * Writes new content, updates the lock to the new hash, and returns a new BinaryRef.
   * The old blob is kept (not deleted) for history/undo.
   * Throws if the blob is not locked by the given peer.
   */
  replaceLockedFile(
    oldHash: string,
    newData: Uint8Array,
    filename: string,
    mimeType: string,
    peerId: string,
  ): Promise<BinaryRef>;

  /** Resolve a BinaryRef to its FileStat. */
  stat(hash: string): Promise<FileStat | null>;

  /** Dispose: clear all locks. Does NOT delete stored blobs. */
  dispose(): void;
}

// ── Options ─────────────────────────────────────────────────────────────────

export interface VfsManagerOptions {
  /** Storage adapter. Defaults to in-memory. */
  adapter?: VfsAdapter;
}
