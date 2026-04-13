# identity/did

W3C DID identity layer for Prism. Generates Ed25519 keypairs, mints `did:key` / `did:web` DIDs, signs and verifies payloads, and assembles threshold multi-signatures for shared vault ownership. All crypto is Web Crypto (`SubtleCrypto`) so the same code runs in Node 20+, browsers, and the Tauri WebView.

```ts
import { createIdentity, signPayload, verifySignature } from "@prism/core/identity";
```

## Key exports

- `createIdentity(options?)` — generate a fresh `PrismIdentity` (Ed25519 keypair + DID document); `method` defaults to `"key"`, pass `"web"` with a `domain` for did:web.
- `resolveIdentity(did, options?)` — resolve a did:key to its public key and a verifier function.
- `signPayload(identity, data)` / `verifySignature(did, data, signature)` — convenience sign/verify over `Uint8Array` payloads.
- `exportIdentity(identity)` / `importIdentity(exported, options?)` — JWK-based round-trip for file persistence.
- `createMultiSigConfig(threshold, signers)`, `createPartialSignature(identity, data)`, `assembleMultiSignature(config, partials)`, `verifyMultiSignature(config, multiSig, data, options?)` — threshold multi-sig primitives.
- `encodeBase58` / `decodeBase58` / `publicKeyToDidKey` / `didKeyToPublicKey` / `base64urlEncode` — low-level multibase/multicodec helpers.
- Types: `DID`, `DIDDocument`, `PrismIdentity`, `ResolvedIdentity`, `KeyHandle`, `Ed25519KeyPair`, `MultiSigConfig`, `MultiSignature`, `PartialSignature`, `ExportedIdentity`, `CreateIdentityOptions`, `ResolveIdentityOptions`, `ImportIdentityOptions`, `VerificationMethod`.

## Usage

```ts
import { createIdentity, verifySignature } from "@prism/core/identity";

const identity = await createIdentity(); // did:key by default
const payload = new TextEncoder().encode("hello prism");

const signature = await identity.signPayload(payload);
const ok = await verifySignature(identity.did, payload, signature);

console.log(identity.did, ok); // did:key:z6Mk... true
```
