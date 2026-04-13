# identity/trust

Trust and safety primitives — Prism's "sovereign immune system". Pure in-memory implementations with no crypto dependencies; Web Crypto is used only for SHA-256 (Hashcash) and PBKDF2 (password auth).

```ts
import { createLuauSandbox, createPeerTrustGraph } from "@prism/core/trust";
```

## Key exports

- `createLuauSandbox(policy)` — capability-based API restriction with glob URL/path filtering and violation recording (`hasCapability`, `isUrlAllowed`, `isPathAllowed`, `recordViolation`).
- `createSchemaValidator(options?)` — import-safety rules against untrusted JSON: max depth, max string/array length, max total keys, disallowed keys (prototype-pollution guard). Extensible via `addRule`.
- `createHashcashMinter()` / `createHashcashVerifier(defaultBits?)` — SHA-256 proof-of-work spam protection (mint/verify/`createChallenge`).
- `createPeerTrustGraph(options?)` — peer reputation ledger with trust/distrust/ban, positive/negative interactions, content-hash flagging, and `onChange` subscriptions.
- `createShamirSplitter()` — GF(256) Shamir secret sharing (`split`/`combine`) for vault recovery seeds.
- `createEscrowManager()` — deposit/claim/evict lifecycle for encrypted payloads with optional TTL.
- `createPasswordAuthManager(options?)` — PBKDF2-SHA-256 (600k iters default) password records with `register`/`verify`/`changePassword`/`restore`.
- Types: `SandboxCapability`, `SandboxPolicy`, `SandboxViolation`, `LuauSandbox`, `SchemaValidationRule`, `SchemaValidator`, `SchemaValidationResult`, `HashcashChallenge`, `HashcashProof`, `HashcashMinter`, `HashcashVerifier`, `TrustLevel`, `PeerReputation`, `PeerTrustGraph`, `ContentHash`, `TrustGraphEvent`, `ShamirShare`, `ShamirConfig`, `ShamirSplitter`, `EscrowDeposit`, `EscrowManager`, `PasswordAuthRecord`, `PasswordAuthResult`, `PasswordAuthManager`.

## Usage

```ts
import { createLuauSandbox } from "@prism/core/trust";

const sandbox = createLuauSandbox({
  capabilities: ["net:fetch", "fs:read"],
  allowedUrls: ["https://api.example.com/*"],
  allowedPaths: ["/vault/readonly/*"],
});

sandbox.hasCapability("net:fetch");                 // true
sandbox.isUrlAllowed("https://api.example.com/v1"); // true
sandbox.isPathAllowed("/vault/secret/keys");        // false
```
