# identity/encryption

AES-GCM-256 encryption for Loro CRDT snapshots at rest. Keys are HKDF-derived from the identity's Ed25519 private key material, scoped per-vault and optionally per-collection, with a rotation path and a pluggable `KeyStore` that a Tauri keychain adapter can drop into.

```ts
import { createVaultKeyManager } from "@prism/core/encryption";
```

## Key exports

- `createVaultKeyManager(options)` — build a `VaultKeyManager` bound to a `vaultId`, with methods `deriveVaultKey(privateKeyBytes)`, `deriveCollectionKey(collectionId)`, `rotateKey(keyId)`, `encryptSnapshot(data, keyId?, aad?)`, `decryptSnapshot(encrypted)`, `getKeyInfo`, `listKeys`, and `dispose`.
- `encryptSnapshot(data, rawKey, aad?, options?)` / `decryptSnapshot(iv, ciphertext, rawKey, aad?, options?)` — standalone AES-GCM-256 helpers for one-off encryption without a manager.
- `createMemoryKeyStore()` — in-memory `KeyStore` implementation for tests; production uses a Tauri keychain-backed `KeyStore`.
- Types: `VaultKeyInfo`, `EncryptedSnapshot`, `KeyStore`, `VaultKeyManager`, `VaultKeyManagerOptions`.

## Usage

```ts
import { createIdentity } from "@prism/core/identity";
import { createVaultKeyManager } from "@prism/core/encryption";

const identity = await createIdentity();
const keys = createVaultKeyManager({ vaultId: "my-vault" });
await keys.deriveVaultKey(identity.keyHandle.publicKeyBytes);

const snapshot = new Uint8Array([/* loro snapshot bytes */]);
const sealed = await keys.encryptSnapshot(snapshot, undefined, "vault:my-vault");
const plaintext = await keys.decryptSnapshot(sealed);
```
