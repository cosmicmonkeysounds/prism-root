# @prism/shared

Shared TypeScript types and YAML schemas used across all Prism packages.

## Build
- `pnpm typecheck` — type-check only (no emit)

## Rules
- Types only — no runtime code, no side effects
- Every type used across packages lives here
- IPC command types define the Tauri invoke() contract
