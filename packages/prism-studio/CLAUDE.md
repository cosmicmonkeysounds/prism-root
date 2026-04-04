# @prism/studio

The Universal Host — Vite SPA + Tauri 2.0 desktop shell.

## Build
- `pnpm dev` — Vite dev server on :1420
- `pnpm build` — production build
- `pnpm tauri dev` — Tauri desktop with hot reload
- `pnpm typecheck`

## Architecture
- Vite SPA (NOT Next.js) — local-first philosophy
- Tauri 2.0 shell wraps the SPA for desktop
- All daemon communication via `src/ipc-bridge.ts` using Tauri invoke()
- Never raw HTTP between frontend and daemon
