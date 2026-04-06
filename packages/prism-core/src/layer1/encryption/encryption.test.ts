import { describe, it, expect } from "vitest";
import {
  createMemoryKeyStore,
  createVaultKeyManager,
  encryptSnapshot,
  decryptSnapshot,
} from "./encryption.js";
import { createIdentity } from "../identity/identity.js";

// ── MemoryKeyStore ──────────────────────────────────────────────────────────

describe("MemoryKeyStore", () => {
  it("stores and retrieves key material", async () => {
    const store = createMemoryKeyStore();
    const material = new Uint8Array([1, 2, 3, 4]);

    await store.set("key-1", material);
    const retrieved = await store.get("key-1");
    expect(retrieved).toEqual(material);
  });

  it("returns null for missing key", async () => {
    const store = createMemoryKeyStore();
    expect(await store.get("nonexistent")).toBeNull();
  });

  it("deletes key material", async () => {
    const store = createMemoryKeyStore();
    await store.set("key-1", new Uint8Array([1]));

    const deleted = await store.delete("key-1");
    expect(deleted).toBe(true);
    expect(await store.get("key-1")).toBeNull();
  });

  it("lists all key IDs", async () => {
    const store = createMemoryKeyStore();
    await store.set("a", new Uint8Array([1]));
    await store.set("b", new Uint8Array([2]));

    const keys = await store.list();
    expect(keys).toContain("a");
    expect(keys).toContain("b");
  });

  it("stores a defensive copy", async () => {
    const store = createMemoryKeyStore();
    const original = new Uint8Array([1, 2, 3]);
    await store.set("key-1", original);

    original[0] = 99;
    const retrieved = await store.get("key-1");
    expect(retrieved?.[0]).toBe(1); // not mutated
  });
});

// ── VaultKeyManager ─────────────────────────────────────────────────────────

describe("VaultKeyManager", () => {
  async function setupManager() {
    const identity = await createIdentity();
    // Export the private key bytes for key derivation
    const pkcs8 = await globalThis.crypto.subtle.exportKey("pkcs8", identity.keyHandle.signingKey);
    const privateKeyBytes = new Uint8Array(pkcs8);

    const manager = createVaultKeyManager({ vaultId: "test-vault" });
    await manager.deriveVaultKey(privateKeyBytes);
    return { identity, manager, privateKeyBytes };
  }

  it("derives a vault key", async () => {
    const { manager } = await setupManager();
    const info = manager.defaultKeyInfo;

    expect(info.keyId).toContain("test-vault");
    expect(info.version).toBe(1);
    expect(info.collectionId).toBeNull();
    expect(info.rotatedAt).toBeNull();
  });

  it("derives per-collection keys", async () => {
    const { manager } = await setupManager();
    const collInfo = await manager.deriveCollectionKey("notes");

    expect(collInfo.keyId).toContain("notes");
    expect(collInfo.collectionId).toBe("notes");
    expect(collInfo.version).toBe(1);
  });

  it("derives different keys for different collections", async () => {
    const { manager } = await setupManager();
    await manager.deriveCollectionKey("notes");
    await manager.deriveCollectionKey("tasks");

    const keys = manager.listKeys();
    expect(keys.length).toBe(3); // default + notes + tasks

    const collIds = keys.map(k => k.collectionId).filter(Boolean);
    expect(collIds).toContain("notes");
    expect(collIds).toContain("tasks");
  });

  it("throws when deriving collection key before vault key", async () => {
    const manager = createVaultKeyManager({ vaultId: "no-vault" });
    await expect(manager.deriveCollectionKey("notes")).rejects.toThrow("not yet derived");
  });

  it("rotates a key", async () => {
    const { manager } = await setupManager();
    const original = manager.defaultKeyInfo;

    const rotated = await manager.rotateKey(original.keyId);
    expect(rotated.version).toBe(2);
    expect(rotated.rotatedAt).not.toBeNull();
  });

  it("throws when rotating unknown key", async () => {
    const { manager } = await setupManager();
    await expect(manager.rotateKey("nonexistent")).rejects.toThrow("Key not found");
  });

  it("encrypts and decrypts a snapshot", async () => {
    const { manager } = await setupManager();
    const plaintext = new TextEncoder().encode("loro-snapshot-data-here");

    const encrypted = await manager.encryptSnapshot(plaintext);
    expect(encrypted.keyId).toContain("test-vault");
    expect(encrypted.keyVersion).toBe(1);
    expect(encrypted.iv).toBeTruthy();
    expect(encrypted.ciphertext).toBeInstanceOf(Uint8Array);
    expect(encrypted.ciphertext.length).toBeGreaterThan(plaintext.length); // GCM adds auth tag

    const decrypted = await manager.decryptSnapshot(encrypted);
    expect(decrypted).toEqual(plaintext);
  });

  it("encrypts with AAD and requires same AAD for decryption", async () => {
    const { manager } = await setupManager();
    const plaintext = new TextEncoder().encode("authenticated-data");

    const encrypted = await manager.encryptSnapshot(plaintext, undefined, "collection-123");
    expect(encrypted.aad).toBe("collection-123");

    const decrypted = await manager.decryptSnapshot(encrypted);
    expect(decrypted).toEqual(plaintext);

    // Tamper with AAD — should fail
    const tampered = { ...encrypted, aad: "collection-456" };
    await expect(manager.decryptSnapshot(tampered)).rejects.toThrow();
  });

  it("encrypts with a per-collection key", async () => {
    const { manager } = await setupManager();
    const collInfo = await manager.deriveCollectionKey("secrets");
    const plaintext = new TextEncoder().encode("secret-snapshot");

    const encrypted = await manager.encryptSnapshot(plaintext, collInfo.keyId);
    expect(encrypted.keyId).toBe(collInfo.keyId);

    const decrypted = await manager.decryptSnapshot(encrypted);
    expect(decrypted).toEqual(plaintext);
  });

  it("produces different ciphertext for same plaintext (random IV)", async () => {
    const { manager } = await setupManager();
    const plaintext = new TextEncoder().encode("same-data");

    const a = await manager.encryptSnapshot(plaintext);
    const b = await manager.encryptSnapshot(plaintext);

    expect(a.iv).not.toBe(b.iv);
    expect(a.ciphertext).not.toEqual(b.ciphertext);
  });

  it("decrypts after key rotation using rotated key", async () => {
    const { manager } = await setupManager();
    const plaintext = new TextEncoder().encode("pre-rotation-data");

    // Rotate the key
    await manager.rotateKey(manager.defaultKeyInfo.keyId);

    // Encrypt with the rotated key
    const encrypted = await manager.encryptSnapshot(plaintext);
    expect(encrypted.keyVersion).toBe(2);

    const decrypted = await manager.decryptSnapshot(encrypted);
    expect(decrypted).toEqual(plaintext);
  });

  it("getKeyInfo returns info for existing keys", async () => {
    const { manager } = await setupManager();
    const info = manager.getKeyInfo(manager.defaultKeyInfo.keyId);
    expect(info).toBeDefined();
    expect(info?.version).toBe(1);
  });

  it("getKeyInfo returns undefined for unknown keys", async () => {
    const { manager } = await setupManager();
    expect(manager.getKeyInfo("unknown")).toBeUndefined();
  });

  it("dispose clears all keys", async () => {
    const { manager } = await setupManager();
    await manager.deriveCollectionKey("notes");
    expect(manager.listKeys().length).toBe(2);

    await manager.dispose();
    expect(manager.listKeys()).toHaveLength(0);
  });
});

// ── Standalone encrypt/decrypt ──────────────────────────────────────────────

describe("standalone encryptSnapshot / decryptSnapshot", () => {
  it("encrypts and decrypts with a raw key", async () => {
    const rawKey = new Uint8Array(32);
    globalThis.crypto.getRandomValues(rawKey);

    const plaintext = new TextEncoder().encode("standalone-test");

    const { iv, ciphertext } = await encryptSnapshot(plaintext, rawKey);
    expect(iv).toBeTruthy();
    expect(ciphertext.length).toBeGreaterThan(0);

    const decrypted = await decryptSnapshot(iv, ciphertext, rawKey);
    expect(decrypted).toEqual(plaintext);
  });

  it("standalone encrypt with AAD", async () => {
    const rawKey = new Uint8Array(32);
    globalThis.crypto.getRandomValues(rawKey);

    const plaintext = new TextEncoder().encode("aad-test");
    const { iv, ciphertext } = await encryptSnapshot(plaintext, rawKey, "my-aad");

    const decrypted = await decryptSnapshot(iv, ciphertext, rawKey, "my-aad");
    expect(decrypted).toEqual(plaintext);

    // Wrong AAD fails
    await expect(decryptSnapshot(iv, ciphertext, rawKey, "wrong")).rejects.toThrow();
  });

  it("fails with wrong key", async () => {
    const key1 = new Uint8Array(32);
    const key2 = new Uint8Array(32);
    globalThis.crypto.getRandomValues(key1);
    globalThis.crypto.getRandomValues(key2);

    const plaintext = new TextEncoder().encode("key-mismatch");
    const { iv, ciphertext } = await encryptSnapshot(plaintext, key1);

    await expect(decryptSnapshot(iv, ciphertext, key2)).rejects.toThrow();
  });
});
