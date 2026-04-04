# Prism Framework

Distributed Visual Operating System. Turborepo + pnpm monorepo.

## Commands
- `pnpm dev` — start all packages
- `pnpm build` — build in dependency order
- `pnpm test` — Vitest all packages
- `pnpm test:e2e` — Playwright E2E
- `pnpm lint` / `pnpm typecheck` / `pnpm format`
- `cd packages/prism-daemon && cargo test && cargo clippy`

## Style
- ES Modules, TypeScript strict, no `any`, kebab-case files
- Conventional Commits: `type(scope): description`
- Use `@prism/*` path aliases for all imports — never relative paths across packages
- Never deprecate. Rename, move, break, fix. `tsc --noEmit` is your safety net.

## Architecture (read SPEC.md for full details)
- Loro CRDT = source of truth. Editors project Loro state.
- CodeMirror 6 = sole editor. No Monaco.
- Vite SPA for clients. Next.js only on Relays.
- Tauri IPC for frontend<->daemon. Never raw HTTP.
- Ephemeral state (cursors, drags) = RAM only.

## Workflow — IMPORTANT
After every implementation:
1. Write/update tests in `*.test.ts`
2. Run `pnpm test`
3. Update README/CLAUDE.md if public API changed
4. Update `docs/dev/current-plan.md`
Do NOT mark done until all four complete.

## Navigation
- Full spec: `SPEC.md`
- Decisions: `docs/adr/`
- Current task: `docs/dev/current-plan.md`
- Package context: each package's `CLAUDE.md`
