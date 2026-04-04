# Prism

**Distributed Visual Operating System** built on a **Federated Object-Graph** using CRDTs.

Local-first, file-over-app. Applications are lenses -- disposable views over sovereign data. If the entire Prism ecosystem disappeared tomorrow, users retain 100% functional local files.

## Quick Start

### Prerequisites

- **Node.js** >= 20
- **pnpm** >= 9
- **Rust** >= 1.75 (for prism-daemon)
- **Tauri 2.0 CLI** (`cargo install tauri-cli --version "^2"`)

### Setup

```bash
# Install dependencies
pnpm install

# Run TypeScript type checking
pnpm typecheck

# Run tests (Vitest)
pnpm test

# Run Rust tests
cd packages/prism-daemon && cargo test

# Start dev server (Vite SPA)
cd packages/prism-studio && pnpm dev

# Start Tauri desktop app (includes daemon)
cd packages/prism-studio && pnpm tauri dev
```

## Architecture

```
prism/
├── packages/
│   ├── shared/           # Shared TypeScript types and IPC contracts
│   ├── prism-core/       # Layer 1 (agnostic TS) + Layer 2 (React renderers)
│   ├── prism-daemon/     # Rust: Loro CRDT, mlua Lua 5.4, VFS, hardware
│   └── prism-studio/     # Vite SPA + Tauri 2.0 shell (the Universal Host)
├── e2e/                  # Playwright end-to-end tests
├── docs/                 # ADRs, RFCs, dev plans
└── scripts/              # Build helpers, hooks
```

### The Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| CRDT | Loro | Single source of truth for all state |
| State | Zustand | Atomic stores subscribed to Loro nodes |
| Scripting | Lua 5.4 | Same scripts run in browser (wasmoon) and daemon (mlua) |
| Desktop | Tauri 2.0 | Native shell, Rust backend, IPC bridge |
| Frontend | Vite + React | SPA for all client apps |
| Editor | CodeMirror 6 | Sole text/code editor (no Monaco) |

### Data Flow (Phase 1: The Heartbeat)

```
React UI  -->  Zustand Store  -->  Loro Bridge  -->  LoroDoc (browser)
                                                         |
                                                    Tauri IPC
                                                         |
                                                   Rust Daemon  -->  LoroDoc (native)
                                                         |
                                                   mlua (Lua 5.4)
```

Frontend writes go through the Zustand store, which delegates to the Loro bridge. The bridge writes to a local LoroDoc and (when running in Tauri) syncs with the Rust daemon via IPC. The daemon holds the authoritative LoroDoc and can execute Lua scripts against CRDT state.

## Build Phases

| Phase | Name | Description | Status |
|-------|------|-------------|--------|
| 1 | The Heartbeat | Loro CRDT round-trip via Tauri IPC | **In Progress** |
| 2 | The Eyes | CodeMirror 6 + Puck visual editing | Planned |
| 3 | The Graph | @xyflow spatial node graph | Planned |
| 4 | The Network | Relay sync + E2EE | Planned |
| 5+ | Lenses | Flux, Lattice, Cadence, Grip apps | Planned |

## Philosophy

- **Local-First**: User's hard drive is the source of truth
- **Every App Is an IDE**: Consumer and developer share the same app (Glass Flip)
- **Ownership Over Convenience**: MIT-only deps, no vendor lock-in
- **Blockchain-Free**: W3C DIDs + E2EE CRDTs, not ledgers or tokens
- **Loro Is the Hidden Constant**: All state lives in Loro. Editors are projections.

## Development

```bash
# Commands
pnpm dev          # Start all packages
pnpm build        # Build in dependency order
pnpm test         # Run all Vitest tests
pnpm test:e2e     # Playwright end-to-end
pnpm lint         # ESLint
pnpm typecheck    # TypeScript strict check
pnpm format       # Prettier

# Rust
cd packages/prism-daemon
cargo test        # Unit + integration tests
cargo clippy      # Linting
cargo fmt         # Format
```

## License

MIT
