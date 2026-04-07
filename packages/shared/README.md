# @prism/shared

Shared TypeScript types and IPC contracts used by all Prism packages. This package contains **no runtime code** — only type definitions and interface contracts.

## What's Here

| File | Purpose |
|------|---------|
| `types.ts` | Core branded IDs (`NodeId`, `VaultId`), CRDT types (`CrdtSnapshot`, `CrdtUpdate`), `LuaResult`, `Manifest` |
| `ipc-types.ts` | Tauri IPC command contracts: `CrdtWriteRequest`, `CrdtReadRequest`, `LuaExecRequest`, `IpcCommands` |

## Rules

- **Types only** — no runtime code, no side effects, no imports with side effects.
- Every type used across two or more packages lives here.
- IPC command types define the contract between `@prism/studio` (frontend) and `prism-daemon` (Rust backend) via Tauri `invoke()`.

## Usage

```typescript
import type { NodeId, CrdtSnapshot } from "@prism/shared";
import type { CrdtWriteRequest } from "@prism/shared/ipc";
```

## Build

```bash
pnpm typecheck   # Type-check only (no emit — consumed as source by other packages)
```
