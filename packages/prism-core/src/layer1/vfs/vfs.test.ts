import { describe, it, expect } from "vitest";
import {
  createMemoryVfsAdapter,
  createVfsManager,
  computeBinaryHash,
} from "./vfs.js";

const enc = new TextEncoder();

// ── computeBinaryHash ───────────────────────────────────────────────────────

describe("computeBinaryHash", () => {
  it("returns a hex-encoded SHA-256 hash", async () => {
    const hash = await computeBinaryHash(enc.encode("hello"));
    // SHA-256 of "hello" is well-known
    expect(hash).toBe("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
  });

  it("same content produces same hash", async () => {
    const a = await computeBinaryHash(enc.encode("data"));
    const b = await computeBinaryHash(enc.encode("data"));
    expect(a).toBe(b);
  });

  it("different content produces different hash", async () => {
    const a = await computeBinaryHash(enc.encode("aaa"));
    const b = await computeBinaryHash(enc.encode("bbb"));
    expect(a).not.toBe(b);
  });
});

// ── MemoryVfsAdapter ────────────────────────────────────────────────────────

describe("MemoryVfsAdapter", () => {
  it("writes and reads a blob", async () => {
    const adapter = createMemoryVfsAdapter();
    const data = enc.encode("image-data");
    const hash = await adapter.write(data, "image/png");

    expect(hash).toHaveLength(64); // SHA-256 hex
    const read = await adapter.read(hash);
    expect(read).toEqual(data);
  });

  it("returns null for missing blob", async () => {
    const adapter = createMemoryVfsAdapter();
    expect(await adapter.read("nonexistent")).toBeNull();
  });

  it("deduplicates identical content", async () => {
    const adapter = createMemoryVfsAdapter();
    const data = enc.encode("same-content");
    const h1 = await adapter.write(data, "text/plain");
    const h2 = await adapter.write(data, "text/plain");

    expect(h1).toBe(h2);
    expect(await adapter.count()).toBe(1);
  });

  it("stat returns file metadata", async () => {
    const adapter = createMemoryVfsAdapter();
    const data = enc.encode("metadata-test");
    const hash = await adapter.write(data, "application/pdf");

    const stat = await adapter.stat(hash);
    expect(stat).toBeDefined();
    const s = stat as NonNullable<typeof stat>;
    expect(s.hash).toBe(hash);
    expect(s.size).toBe(data.length);
    expect(s.mimeType).toBe("application/pdf");
    expect(s.createdAt).toBeTruthy();
  });

  it("stat returns null for missing blob", async () => {
    const adapter = createMemoryVfsAdapter();
    expect(await adapter.stat("missing")).toBeNull();
  });

  it("list returns all hashes", async () => {
    const adapter = createMemoryVfsAdapter();
    const h1 = await adapter.write(enc.encode("one"), "text/plain");
    const h2 = await adapter.write(enc.encode("two"), "text/plain");

    const hashes = await adapter.list();
    expect(hashes).toContain(h1);
    expect(hashes).toContain(h2);
    expect(hashes).toHaveLength(2);
  });

  it("delete removes a blob", async () => {
    const adapter = createMemoryVfsAdapter();
    const hash = await adapter.write(enc.encode("delete-me"), "text/plain");

    expect(await adapter.delete(hash)).toBe(true);
    expect(await adapter.has(hash)).toBe(false);
    expect(await adapter.read(hash)).toBeNull();
  });

  it("delete returns false for missing blob", async () => {
    const adapter = createMemoryVfsAdapter();
    expect(await adapter.delete("nope")).toBe(false);
  });

  it("has checks existence", async () => {
    const adapter = createMemoryVfsAdapter();
    const hash = await adapter.write(enc.encode("exists"), "text/plain");

    expect(await adapter.has(hash)).toBe(true);
    expect(await adapter.has("nope")).toBe(false);
  });

  it("count and totalSize track storage", async () => {
    const adapter = createMemoryVfsAdapter();
    expect(await adapter.count()).toBe(0);
    expect(await adapter.totalSize()).toBe(0);

    const d1 = enc.encode("1234567890"); // 10 bytes
    const d2 = enc.encode("abcde"); // 5 bytes
    await adapter.write(d1, "text/plain");
    await adapter.write(d2, "text/plain");

    expect(await adapter.count()).toBe(2);
    expect(await adapter.totalSize()).toBe(15);
  });

  it("stores a defensive copy of data", async () => {
    const adapter = createMemoryVfsAdapter();
    const data = enc.encode("original");
    const hash = await adapter.write(data, "text/plain");

    // Mutate the original
    data[0] = 0xff;
    const read = await adapter.read(hash);
    expect(read).toBeDefined();
    expect((read as Uint8Array)[0]).not.toBe(0xff);
  });
});

// ── VfsManager: import/export ───────────────────────────────────────────────

describe("VfsManager import/export", () => {
  it("imports a file and returns a BinaryRef", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("photo-bytes");
    const ref = await vfs.importFile(data, "photo.png", "image/png");

    expect(ref.hash).toHaveLength(64);
    expect(ref.filename).toBe("photo.png");
    expect(ref.mimeType).toBe("image/png");
    expect(ref.size).toBe(data.length);
    expect(ref.importedAt).toBeTruthy();
  });

  it("exports a file by its BinaryRef", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("export-test");
    const ref = await vfs.importFile(data, "doc.txt", "text/plain");

    const exported = await vfs.exportFile(ref);
    expect(exported).toEqual(data);
  });

  it("export returns null for missing ref", async () => {
    const vfs = createVfsManager();
    const fakeRef = {
      hash: "0000000000000000000000000000000000000000000000000000000000000000",
      filename: "missing.txt",
      mimeType: "text/plain",
      size: 0,
      importedAt: new Date().toISOString(),
    };
    expect(await vfs.exportFile(fakeRef)).toBeNull();
  });

  it("deduplicates identical imports", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("duplicate-content");

    const ref1 = await vfs.importFile(data, "a.bin", "application/octet-stream");
    const ref2 = await vfs.importFile(data, "b.bin", "application/octet-stream");

    expect(ref1.hash).toBe(ref2.hash);
    // Different filenames but same hash
    expect(ref1.filename).toBe("a.bin");
    expect(ref2.filename).toBe("b.bin");
    expect(await vfs.adapter.count()).toBe(1);
  });

  it("removes a file", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("removable");
    const ref = await vfs.importFile(data, "temp.bin", "application/octet-stream");

    expect(await vfs.removeFile(ref.hash)).toBe(true);
    expect(await vfs.exportFile(ref)).toBeNull();
  });

  it("stat returns file info", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("stat-test");
    const ref = await vfs.importFile(data, "info.txt", "text/plain");

    const stat = await vfs.stat(ref.hash);
    expect(stat).toBeDefined();
    const s = stat as NonNullable<typeof stat>;
    expect(s.hash).toBe(ref.hash);
    expect(s.size).toBe(data.length);
    expect(s.mimeType).toBe("text/plain");
  });
});

// ── VfsManager: Binary Forking Protocol (locking) ───────────────────────────

describe("VfsManager locking", () => {
  it("acquires a lock on a blob", async () => {
    const vfs = createVfsManager();
    const data = enc.encode("lockable");
    const ref = await vfs.importFile(data, "img.png", "image/png");

    const lock = vfs.acquireLock(ref.hash, "peer-alice", "editing");
    expect(lock.hash).toBe(ref.hash);
    expect(lock.lockedBy).toBe("peer-alice");
    expect(lock.reason).toBe("editing");
    expect(lock.lockedAt).toBeTruthy();
  });

  it("isLocked and getLock reflect lock state", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("x"), "x.bin", "application/octet-stream");

    expect(vfs.isLocked(ref.hash)).toBe(false);
    expect(vfs.getLock(ref.hash)).toBeNull();

    vfs.acquireLock(ref.hash, "peer-bob");

    expect(vfs.isLocked(ref.hash)).toBe(true);
    const lock = vfs.getLock(ref.hash);
    expect(lock).toBeDefined();
    expect((lock as NonNullable<typeof lock>).lockedBy).toBe("peer-bob");
  });

  it("same peer can re-acquire their own lock", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("mine"), "m.bin", "application/octet-stream");

    vfs.acquireLock(ref.hash, "peer-alice");
    const lock2 = vfs.acquireLock(ref.hash, "peer-alice");
    expect(lock2.lockedBy).toBe("peer-alice");
  });

  it("throws when another peer tries to acquire an existing lock", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("contested"), "c.bin", "application/octet-stream");

    vfs.acquireLock(ref.hash, "peer-alice");
    expect(() => vfs.acquireLock(ref.hash, "peer-bob")).toThrow(
      "already locked by peer-alice",
    );
  });

  it("releases a lock", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("release"), "r.bin", "application/octet-stream");

    vfs.acquireLock(ref.hash, "peer-alice");
    vfs.releaseLock(ref.hash, "peer-alice");

    expect(vfs.isLocked(ref.hash)).toBe(false);
  });

  it("throws when releasing a lock held by another peer", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("foreign"), "f.bin", "application/octet-stream");

    vfs.acquireLock(ref.hash, "peer-alice");
    expect(() => vfs.releaseLock(ref.hash, "peer-bob")).toThrow(
      "locked by peer-alice, not peer-bob",
    );
  });

  it("throws when releasing an unlocked blob", async () => {
    const vfs = createVfsManager();
    expect(() => vfs.releaseLock("no-such-hash", "peer-alice")).toThrow("not locked");
  });

  it("cannot remove a locked blob", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("protected"), "p.bin", "application/octet-stream");

    vfs.acquireLock(ref.hash, "peer-alice");
    await expect(vfs.removeFile(ref.hash)).rejects.toThrow("locked");
  });

  it("listLocks returns all active locks", async () => {
    const vfs = createVfsManager();
    const r1 = await vfs.importFile(enc.encode("a"), "a.bin", "application/octet-stream");
    const r2 = await vfs.importFile(enc.encode("b"), "b.bin", "application/octet-stream");

    vfs.acquireLock(r1.hash, "peer-alice");
    vfs.acquireLock(r2.hash, "peer-bob");

    const locks = vfs.listLocks();
    expect(locks).toHaveLength(2);
    const peers = locks.map(l => l.lockedBy).sort();
    expect(peers).toEqual(["peer-alice", "peer-bob"]);
  });
});

// ── VfsManager: replaceLockedFile ───────────────────────────────────────────

describe("VfsManager replaceLockedFile", () => {
  it("replaces locked blob content and returns new BinaryRef", async () => {
    const vfs = createVfsManager();
    const oldData = enc.encode("original-image");
    const ref = await vfs.importFile(oldData, "img.png", "image/png");

    vfs.acquireLock(ref.hash, "peer-alice");

    const newData = enc.encode("edited-image");
    const newRef = await vfs.replaceLockedFile(
      ref.hash, newData, "img.png", "image/png", "peer-alice",
    );

    expect(newRef.hash).not.toBe(ref.hash);
    expect(newRef.size).toBe(newData.length);

    // New content is readable
    const exported = await vfs.exportFile(newRef);
    expect(exported).toEqual(newData);

    // Old content is still preserved
    const oldExported = await vfs.adapter.read(ref.hash);
    expect(oldExported).toEqual(oldData);

    // Lock moved to new hash
    expect(vfs.isLocked(ref.hash)).toBe(false);
    expect(vfs.isLocked(newRef.hash)).toBe(true);
    const newLock = vfs.getLock(newRef.hash);
    expect(newLock).toBeDefined();
    expect((newLock as NonNullable<typeof newLock>).lockedBy).toBe("peer-alice");
  });

  it("throws when replacing an unlocked blob", async () => {
    const vfs = createVfsManager();
    await expect(
      vfs.replaceLockedFile("nope", enc.encode("x"), "x.bin", "text/plain", "peer-alice"),
    ).rejects.toThrow("not locked");
  });

  it("throws when replacing a blob locked by another peer", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("guarded"), "g.bin", "application/octet-stream");
    vfs.acquireLock(ref.hash, "peer-alice");

    await expect(
      vfs.replaceLockedFile(ref.hash, enc.encode("hacked"), "g.bin", "application/octet-stream", "peer-bob"),
    ).rejects.toThrow("locked by peer-alice, not peer-bob");
  });
});

// ── VfsManager: dispose ─────────────────────────────────────────────────────

describe("VfsManager dispose", () => {
  it("clears all locks but keeps blobs", async () => {
    const vfs = createVfsManager();
    const ref = await vfs.importFile(enc.encode("persist"), "p.bin", "application/octet-stream");
    vfs.acquireLock(ref.hash, "peer-alice");

    vfs.dispose();

    expect(vfs.listLocks()).toHaveLength(0);
    expect(vfs.isLocked(ref.hash)).toBe(false);
    // Blob is still there
    expect(await vfs.exportFile(ref)).toEqual(enc.encode("persist"));
  });
});

// ── VfsManager: custom adapter ──────────────────────────────────────────────

describe("VfsManager with custom adapter", () => {
  it("uses the provided adapter", async () => {
    const adapter = createMemoryVfsAdapter();
    const vfs = createVfsManager({ adapter });

    const ref = await vfs.importFile(enc.encode("custom"), "c.txt", "text/plain");

    // Access via the adapter directly
    const fromAdapter = await adapter.read(ref.hash);
    expect(fromAdapter).toEqual(enc.encode("custom"));
    expect(vfs.adapter).toBe(adapter);
  });
});
