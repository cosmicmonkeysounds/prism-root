# ADR-001: Loro CRDT over Yjs

**Status**: Accepted
**Date**: 2026-04-03

## Context

Prism needs a CRDT library as the single source of truth for all state in the object graph. The two leading options are:

- **Yjs** — The most widely adopted JavaScript CRDT, used by Notion, Liveblocks, and many collaborative editors. Mature ecosystem with y-websocket, y-indexeddb, etc.
- **Loro** — A newer Rust+WASM CRDT with first-class support for rich data structures (Map, List, Text, Tree, MovableList, Counter). Actively maintained (v1.x).

## Decision

We chose **Loro** as the CRDT engine for Prism.

## Rationale

### 1. Rust-native with WASM bridge

Prism's daemon is written in Rust (Tauri 2.0). Loro is a Rust crate (`loro`) that also compiles to WASM (`loro-crdt` npm package). This means:

- **Same library, both sides**: The daemon uses native Loro; the browser uses Loro-WASM. No impedance mismatch.
- **Performance**: Rust-native CRDT operations in the daemon avoid the overhead of running a JS runtime for state management.
- Yjs is JavaScript-only. Using it in the Rust daemon would require embedding a JS runtime or maintaining a separate Rust CRDT implementation.

### 2. Richer data model

Loro natively supports `LoroMap`, `LoroList`, `LoroText`, `LoroTree`, `LoroMovableList`, and `LoroCounter` — all as first-class CRDT types. The `LoroTree` type is particularly relevant for Prism's object graph (hierarchical node structures).

Yjs provides `Y.Map`, `Y.Array`, `Y.Text`, and `Y.XmlFragment`, but lacks native tree and movable list types.

### 3. Export/import model matches E2EE requirements

Loro's `doc.export({ mode: "snapshot" })` and `doc.export({ mode: "update" })` produce `Uint8Array` blobs that are perfect for E2EE: encrypt the blob with libsodium before sending to a Relay. The Relay never sees document structure.

Yjs has a similar model with `Y.encodeStateAsUpdate()`, but Loro's export modes are more explicit and align better with Prism's sync architecture.

### 4. @loro-extended ecosystem

The `@loro-extended` framework (by SchoolAI) provides sync scaffolding built directly on Loro: schemas, network adapters (SSE + WebSocket + WebRTC), persistence (PostgreSQL), and reactive subscriptions. This reduces the integration burden for Phase 4 (The Network).

### 5. Performance characteristics

Loro uses a Rust-optimized internal representation with efficient memory layout. Benchmarks show competitive or superior performance to Yjs for large documents, particularly for tree operations and rich text.

## Consequences

### Positive

- Single CRDT library across Rust daemon and browser
- Native tree type supports the object graph without workarounds
- Clean export/import model for E2EE
- Growing ecosystem with @loro-extended for sync

### Negative

- Smaller community than Yjs — fewer tutorials, Stack Overflow answers
- Less battle-tested in production at scale (Yjs is used by Notion, etc.)
- The library requires "substantial development work" for production collaborative apps (per independent assessment)
- WASM bundle adds ~200KB to the browser payload

### Mitigations

- Pin to stable Loro releases (1.x) and track the changelog
- Write comprehensive CRDT merge/sync tests (already in Phase 1)
- The @loro-extended framework reduces the "substantial development work" gap
- WASM size is acceptable for a desktop-class application (Tauri/Capacitor)
