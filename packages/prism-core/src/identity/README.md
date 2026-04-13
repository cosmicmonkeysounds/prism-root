# identity

DID-based identity, encryption, trust, and manifest primitives for Prism. Every workspace is an identity envelope (manifest + vault + shell) signed and unlocked by the DIDs and keys defined here.

The category sits in the `language → identity` tier of the `@prism/core` DAG: above `foundation`, below `kernel`, `network`, `interaction`. Loro CRDT remains the source of truth; identity wraps, encrypts, signs, and gates access to it.

## Subsystems

- [`did/`](./did/README.md) — `@prism/core/identity`. W3C DIDs, Ed25519 keypairs, sign/verify, and threshold multi-sig. did:key is fully supported; did:web can be minted.
- [`encryption/`](./encryption/README.md) — `@prism/core/encryption`. HKDF-derived AES-GCM-256 vault and collection keys, snapshot encryption at rest, pluggable `KeyStore` for Tauri keychain integration.
- [`manifest/`](./manifest/README.md) — `@prism/core/manifest`. `PrismManifest` (the on-disk `.prism.json` index of a workspace), collection refs, plus the FileMaker-style `PrivilegeSet` / `PrivilegeEnforcer` access-control layer.
- [`trust/`](./trust/README.md) — `@prism/core/trust`. The "sovereign immune system": Luau capability sandbox, schema poison-pill validator, Hashcash PoW, peer trust graph, Shamir secret sharing, escrow, and password auth.

## Framing

A "workspace" in Prism is the identity envelope, not the data. The Vault is the encrypted directory boundary, Collections are the typed CRDT arrays holding actual data, and a Manifest is a named set of weak references into those Collections. Multiple manifests can share one vault; the DID and keys in `did/` prove ownership, `encryption/` locks the on-disk bytes, `manifest/` describes what's visible, and `trust/` enforces how code and peers interact with it.
