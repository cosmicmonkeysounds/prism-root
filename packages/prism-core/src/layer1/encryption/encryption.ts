/**
 * @prism/core — Vault Encryption
 *
 * AES-GCM-256 encryption for Loro CRDT snapshots at rest.
 * Keys are derived via HKDF from the identity's Ed25519 private key,
 * scoped per-vault and optionally per-collection.
 *
 * Features:
 *   - HKDF key derivation from Ed25519 identity keypair
 *   - AES-GCM-256 encrypt/decrypt for Loro snapshots
 *   - Per-collection key derivation with rotation support
 *   - Pluggable KeyStore for secure storage (memory, Tauri keychain)
 */

import type {
  VaultKeyInfo,
  EncryptedSnapshot,
  KeyStore,
  VaultKeyManager,
  VaultKeyManagerOptions,
} from "./encryption-types.js";

// ── Helpers ─────────────────────────────────────────────────────────────────

function getSubtle(options?: { subtle?: SubtleCrypto }): SubtleCrypto {
  if (options?.subtle) return options.subtle;
  if (typeof globalThis.crypto !== "undefined" && globalThis.crypto.subtle) {
    return globalThis.crypto.subtle;
  }
  throw new Error("SubtleCrypto not available");
}

function base64urlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64urlDecode(str: string): Uint8Array {
  const padded = str.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function generateIV(): Uint8Array {
  const iv = new Uint8Array(12);
  globalThis.crypto.getRandomValues(iv);
  return iv;
}

const textEncoder = new TextEncoder();

/** Cast Uint8Array to BufferSource for Web Crypto API compatibility with strict TS. */
function buf(bytes: Uint8Array): BufferSource {
  return bytes as unknown as BufferSource;
}

// ── Memory Key Store ────────────────────────────────────────────────────────

/**
 * In-memory key store for testing. Not suitable for production —
 * key material lives in JS heap.
 */
export function createMemoryKeyStore(): KeyStore {
  const store = new Map<string, Uint8Array>();

  return {
    async set(keyId: string, material: Uint8Array): Promise<void> {
      store.set(keyId, new Uint8Array(material));
    },
    async get(keyId: string): Promise<Uint8Array | null> {
      const material = store.get(keyId);
      return material ? new Uint8Array(material) : null;
    },
    async delete(keyId: string): Promise<boolean> {
      return store.delete(keyId);
    },
    async list(): Promise<string[]> {
      return [...store.keys()];
    },
  };
}

// ── Key Derivation ──────────────────────────────────────────────────────────

async function deriveAESKey(
  subtle: SubtleCrypto,
  ikm: Uint8Array,
  salt: Uint8Array,
  info: Uint8Array,
): Promise<{ cryptoKey: CryptoKey; rawBytes: Uint8Array }> {
  const baseKey = await subtle.importKey("raw", buf(ikm), "HKDF", false, ["deriveKey", "deriveBits"]);

  const cryptoKey = await subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt: buf(salt), info: buf(info) },
    baseKey,
    { name: "AES-GCM", length: 256 },
    true,
    ["encrypt", "decrypt"],
  );

  const rawBits = await subtle.exportKey("raw", cryptoKey);
  return { cryptoKey, rawBytes: new Uint8Array(rawBits) };
}

async function importAESKey(
  subtle: SubtleCrypto,
  rawBytes: Uint8Array,
): Promise<CryptoKey> {
  return subtle.importKey("raw", buf(rawBytes), { name: "AES-GCM", length: 256 }, false, [
    "encrypt",
    "decrypt",
  ]);
}

// ── Encrypt / Decrypt ───────────────────────────────────────────────────────

async function aesGcmEncrypt(
  subtle: SubtleCrypto,
  key: CryptoKey,
  plaintext: Uint8Array,
  aad?: string,
): Promise<{ iv: Uint8Array; ciphertext: Uint8Array }> {
  const iv = generateIV();
  const params: AesGcmParams = { name: "AES-GCM", iv: buf(iv) };
  if (aad) params.additionalData = buf(textEncoder.encode(aad));

  const encrypted = await subtle.encrypt(params, key, buf(plaintext));
  return { iv, ciphertext: new Uint8Array(encrypted) };
}

async function aesGcmDecrypt(
  subtle: SubtleCrypto,
  key: CryptoKey,
  iv: Uint8Array,
  ciphertext: Uint8Array,
  aad?: string,
): Promise<Uint8Array> {
  const params: AesGcmParams = { name: "AES-GCM", iv: buf(iv) };
  if (aad) params.additionalData = buf(textEncoder.encode(aad));

  const decrypted = await subtle.decrypt(params, key, buf(ciphertext));
  return new Uint8Array(decrypted);
}

// ── Factory ─────────────────────────────────────────────────────────────────

/**
 * Create a VaultKeyManager for encrypting Loro snapshots at rest.
 *
 * Call `deriveVaultKey(privateKeyBytes)` to initialise the vault-wide key,
 * then use `encryptSnapshot()` / `decryptSnapshot()` for Loro data.
 */
export function createVaultKeyManager(
  options: VaultKeyManagerOptions,
): VaultKeyManager {
  const { vaultId } = options;
  const subtle = getSubtle(options);
  const keyStore = options.keyStore ?? createMemoryKeyStore();

  const keyInfos = new Map<string, VaultKeyInfo>();
  const cryptoKeys = new Map<string, CryptoKey>();

  const defaultKeyId = `vault-${vaultId}-default`;
  let defaultInfo: VaultKeyInfo = {
    keyId: defaultKeyId,
    collectionId: null,
    created: new Date().toISOString(),
    rotatedAt: null,
    version: 0,
  };

  // ── Derive vault key ────────────────────────────────────────────────────

  async function deriveVaultKey(privateKeyBytes: Uint8Array): Promise<VaultKeyInfo> {
    const salt = textEncoder.encode(`prism-vault-${vaultId}`);
    const info = textEncoder.encode("prism-vault-key-v1");
    const { cryptoKey, rawBytes } = await deriveAESKey(subtle, privateKeyBytes, salt, info);

    defaultInfo = {
      keyId: defaultKeyId,
      collectionId: null,
      created: new Date().toISOString(),
      rotatedAt: null,
      version: 1,
    };

    keyInfos.set(defaultKeyId, defaultInfo);
    cryptoKeys.set(defaultKeyId, cryptoKey);
    await keyStore.set(defaultKeyId, rawBytes);

    return defaultInfo;
  }

  // ── Derive collection key ───────────────────────────────────────────────

  async function deriveCollectionKey(collectionId: string): Promise<VaultKeyInfo> {
    const vaultRaw = await keyStore.get(defaultKeyId);
    if (!vaultRaw) throw new Error("Vault key not yet derived — call deriveVaultKey() first");

    const salt = textEncoder.encode(`prism-coll-${collectionId}`);
    const info = textEncoder.encode("prism-collection-key-v1");
    const { cryptoKey, rawBytes } = await deriveAESKey(subtle, vaultRaw, salt, info);

    const keyId = `coll-${vaultId}-${collectionId}`;
    const keyInfo: VaultKeyInfo = {
      keyId,
      collectionId,
      created: new Date().toISOString(),
      rotatedAt: null,
      version: 1,
    };

    keyInfos.set(keyId, keyInfo);
    cryptoKeys.set(keyId, cryptoKey);
    await keyStore.set(keyId, rawBytes);

    return keyInfo;
  }

  // ── Rotate key ──────────────────────────────────────────────────────────

  async function rotateKey(keyId: string): Promise<VaultKeyInfo> {
    const existingInfo = keyInfos.get(keyId);
    if (!existingInfo) throw new Error(`Key not found: ${keyId}`);

    const existingRaw = await keyStore.get(keyId);
    if (!existingRaw) throw new Error(`Key material not found: ${keyId}`);

    // Derive new key from existing + rotation salt
    const newVersion = existingInfo.version + 1;
    const salt = textEncoder.encode(`prism-rotate-${keyId}-v${newVersion}`);
    const info = textEncoder.encode("prism-key-rotation-v1");
    const { cryptoKey, rawBytes } = await deriveAESKey(subtle, existingRaw, salt, info);

    const newInfo: VaultKeyInfo = {
      ...existingInfo,
      rotatedAt: new Date().toISOString(),
      version: newVersion,
    };

    keyInfos.set(keyId, newInfo);
    cryptoKeys.set(keyId, cryptoKey);
    await keyStore.set(keyId, rawBytes);

    return newInfo;
  }

  // ── Encrypt / Decrypt ───────────────────────────────────────────────────

  async function getCryptoKey(keyId: string): Promise<CryptoKey> {
    const cached = cryptoKeys.get(keyId);
    if (cached) return cached;

    const raw = await keyStore.get(keyId);
    if (!raw) throw new Error(`Key not found: ${keyId}`);

    const cryptoKey = await importAESKey(subtle, raw);
    cryptoKeys.set(keyId, cryptoKey);
    return cryptoKey;
  }

  async function encryptSnapshot(
    data: Uint8Array,
    keyId?: string,
    aad?: string,
  ): Promise<EncryptedSnapshot> {
    const resolvedKeyId = keyId ?? defaultKeyId;
    const info = keyInfos.get(resolvedKeyId);
    if (!info) throw new Error(`Key not found: ${resolvedKeyId}`);

    const cryptoKey = await getCryptoKey(resolvedKeyId);
    const { iv, ciphertext } = await aesGcmEncrypt(subtle, cryptoKey, data, aad);

    return {
      keyId: resolvedKeyId,
      keyVersion: info.version,
      iv: base64urlEncode(iv),
      ciphertext,
      ...(aad ? { aad } : {}),
    };
  }

  async function decryptSnapshot(encrypted: EncryptedSnapshot): Promise<Uint8Array> {
    const cryptoKey = await getCryptoKey(encrypted.keyId);
    const iv = base64urlDecode(encrypted.iv);
    return aesGcmDecrypt(subtle, cryptoKey, iv, encrypted.ciphertext, encrypted.aad);
  }

  // ── Query ───────────────────────────────────────────────────────────────

  function getKeyInfo(keyId: string): VaultKeyInfo | undefined {
    return keyInfos.get(keyId);
  }

  function listKeys(): VaultKeyInfo[] {
    return [...keyInfos.values()];
  }

  // ── Dispose ─────────────────────────────────────────────────────────────

  async function dispose(): Promise<void> {
    for (const keyId of keyInfos.keys()) {
      await keyStore.delete(keyId);
    }
    keyInfos.clear();
    cryptoKeys.clear();
  }

  return {
    get defaultKeyInfo() {
      return defaultInfo;
    },
    deriveVaultKey,
    deriveCollectionKey,
    rotateKey,
    getKeyInfo,
    listKeys,
    encryptSnapshot,
    decryptSnapshot,
    dispose,
  };
}

// ── Standalone encrypt/decrypt ──────────────────────────────────────────────

/**
 * Encrypt a Uint8Array with a raw AES-GCM-256 key (32 bytes).
 * Standalone function for one-off encryption without a VaultKeyManager.
 */
export async function encryptSnapshot(
  data: Uint8Array,
  rawKey: Uint8Array,
  aad?: string,
  options?: { subtle?: SubtleCrypto },
): Promise<{ iv: string; ciphertext: Uint8Array }> {
  const s = getSubtle(options);
  const cryptoKey = await importAESKey(s, rawKey);
  const { iv, ciphertext } = await aesGcmEncrypt(s, cryptoKey, data, aad);
  return { iv: base64urlEncode(iv), ciphertext };
}

/**
 * Decrypt a Uint8Array with a raw AES-GCM-256 key (32 bytes).
 */
export async function decryptSnapshot(
  iv: string,
  ciphertext: Uint8Array,
  rawKey: Uint8Array,
  aad?: string,
  options?: { subtle?: SubtleCrypto },
): Promise<Uint8Array> {
  const s = getSubtle(options);
  const cryptoKey = await importAESKey(s, rawKey);
  return aesGcmDecrypt(s, cryptoKey, base64urlDecode(iv), ciphertext, aad);
}
