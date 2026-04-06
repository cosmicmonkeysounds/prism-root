/**
 * @prism/core — Encryption Types
 *
 * Vault-level encryption for Loro CRDT snapshots at rest.
 * AES-GCM-256 with HKDF key derivation from Ed25519 identity keypairs.
 */

// ── Vault Keys ──────────────────────────────────────────────────────────────

/** Metadata for a vault encryption key. */
export interface VaultKeyInfo {
  /** Unique key identifier (e.g. "vault-key-1"). */
  keyId: string;
  /** Which collection this key encrypts (null = vault-wide default). */
  collectionId: string | null;
  /** ISO-8601 creation timestamp. */
  created: string;
  /** ISO-8601 rotation timestamp (null = never rotated). */
  rotatedAt: string | null;
  /** Key version for rotation tracking. */
  version: number;
}

/** An encrypted snapshot blob with metadata for decryption. */
export interface EncryptedSnapshot {
  /** Key ID used for encryption. */
  keyId: string;
  /** Key version at time of encryption. */
  keyVersion: number;
  /** 12-byte IV as base64url. */
  iv: string;
  /** Encrypted ciphertext as Uint8Array. */
  ciphertext: Uint8Array;
  /** Optional associated data tag (e.g. collection ID). */
  aad?: string;
}

// ── Key Store ───────────────────────────────────────────────────────────────

/**
 * Interface for secure key storage. Implementations:
 * - MemoryKeyStore (testing)
 * - TauriKeychainStore (production — Tauri Secure Enclave bridge)
 */
export interface KeyStore {
  /** Store raw key material under the given ID. */
  set(keyId: string, material: Uint8Array): Promise<void>;
  /** Retrieve raw key material by ID. Returns null if not found. */
  get(keyId: string): Promise<Uint8Array | null>;
  /** Delete key material by ID. */
  delete(keyId: string): Promise<boolean>;
  /** List all key IDs. */
  list(): Promise<string[]>;
}

// ── Vault Key Manager ───────────────────────────────────────────────────────

export interface VaultKeyManager {
  /** Get the default vault-wide key info. */
  readonly defaultKeyInfo: VaultKeyInfo;
  /** Derive and store a vault-wide key from an identity's private key. */
  deriveVaultKey(privateKeyBytes: Uint8Array): Promise<VaultKeyInfo>;
  /** Derive a per-collection key from the vault key. */
  deriveCollectionKey(collectionId: string): Promise<VaultKeyInfo>;
  /** Rotate a key (vault-wide or per-collection). Generates a new version. */
  rotateKey(keyId: string): Promise<VaultKeyInfo>;
  /** Get key info by ID. */
  getKeyInfo(keyId: string): VaultKeyInfo | undefined;
  /** List all managed key infos. */
  listKeys(): VaultKeyInfo[];
  /** Encrypt a Loro snapshot. */
  encryptSnapshot(data: Uint8Array, keyId?: string, aad?: string): Promise<EncryptedSnapshot>;
  /** Decrypt a Loro snapshot. */
  decryptSnapshot(encrypted: EncryptedSnapshot): Promise<Uint8Array>;
  /** Dispose of all key material. */
  dispose(): Promise<void>;
}

// ── Options ─────────────────────────────────────────────────────────────────

export interface VaultKeyManagerOptions {
  /** Vault identifier for key derivation context. */
  vaultId: string;
  /** Key store for persisting key material. */
  keyStore?: KeyStore;
  /** Override crypto.subtle for testing. */
  subtle?: SubtleCrypto;
}
