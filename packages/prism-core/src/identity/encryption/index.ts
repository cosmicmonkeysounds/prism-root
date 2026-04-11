export {
  createMemoryKeyStore,
  createVaultKeyManager,
  encryptSnapshot,
  decryptSnapshot,
} from "./encryption.js";

export type {
  VaultKeyInfo,
  EncryptedSnapshot,
  KeyStore,
  VaultKeyManager,
  VaultKeyManagerOptions,
} from "./encryption-types.js";
