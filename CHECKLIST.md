# Prism Ship-Readiness Checklist

> Slint migration parity target: React version at commit `8426588`.
> Relay is all-Rust (Axum), no Tauri/webview anywhere.
>
> **Last updated:** 2026-04-18

---

## Core Infrastructure

- [x] Workspace compiles — `cargo check --workspace` clean
- [x] prism-core foundations — 1,190 tests, all leaf modules ported
- [x] prism-daemon — 13 modules, 6 transports, 95 tests
- [x] prism-cli — all 5 subcommands (test, build, dev, lint, fmt)
- [x] Hot-reload — `.slint` live-preview + `.rs` respawn loop
- [x] WASM web target — `wasm32-unknown-unknown` + `wasm-bindgen` pipeline
- [x] Store / state management — `Store<S>` replaces Zustand
- [x] Kernel orchestration — `PrismKernel` + plugin/automation/builder/actor
- [x] Licensing — GPL-3.0-or-later workspace-wide

---

## UI Shell — Panels

React shipped 43 panels. Slint has 4.

### Shipped

- [x] Identity panel (static/read-only)
- [x] Builder panel (Slint DSL preview)
- [x] Inspector panel (tree dump)
- [x] Properties panel (read-only field rows)

### Must Ship (MVP)

- [ ] Editor panel — text/code editing (CodeMirror 6 equivalent with LoroText sync)
- [ ] Work panel — task/project management (Flux domain surface)
- [ ] Admin panel — live kernel introspection, health, metrics
- [ ] Settings panel — user preferences and app configuration
- [ ] Assets panel — VFS media browser and uploader
- [ ] Import panel — CSV/JSON import with field mapping
- [ ] Schema Designer panel — entity type creation/modification
- [ ] Form Builder panel — drag-drop form assembly

### Should Ship

- [ ] Graph panel — xyflow-equivalent spatial node-graph editor
- [ ] Canvas panel — free-form infinite canvas
- [ ] Layout panel — Puck-equivalent visual page builder with drag-drop
- [ ] Automation panel — trigger/condition/action rule designer
- [ ] Vault panel — encryption/vault management
- [ ] Plugin panel — plugin registry browser and loader
- [ ] Relay panel — relay server configuration/monitoring
- [ ] Trust panel — peer trust, Shamir escrow, hashcash
- [ ] CRM panel — contact/organization management
- [ ] Finance panel — transactions, accounts, invoices
- [ ] Analysis panel — dependency analysis, critical path, graph queries
- [ ] Help/Docs panel — built-in documentation with search

### Can Wait

- [ ] App Builder panel — self-replicating app factory
- [ ] Facet Designer panel — FileMaker-style layout builder
- [ ] Relationship Builder panel — visual relationship editor
- [ ] Shortcuts panel — keyboard shortcut customization
- [ ] Design Tokens panel — theme/token editor
- [ ] Privilege Set panel — permission/capability token editor
- [ ] Publish panel — publishing and deployment
- [ ] Luau panel — script editor with debugger
- [ ] Visual Script panel — step sequencer
- [ ] Sequencer panel — timeline NLE-style
- [ ] Table Facet panel — table/SQL surface designer
- [ ] Report Facet panel — report template designer
- [ ] Form Facet panel — form layout builder
- [ ] Saved View panel — filtered/sorted view management
- [ ] Sitemap panel — navigation map
- [ ] Site-Nav panel — navigation tree editor
- [ ] Spatial Canvas panel — 3D spatial rendering

---

## Shell Framework Features

### Must Ship (MVP)

- [x] Interactive property editing — Store-backed field mutation via editable FieldRowView + MutateNodeProp
- [x] Tabbed MDI — ActivityBar (48px icon strip) + TabBar multi-document interface
- [x] Command palette — Ctrl+Shift+P overlay with fuzzy command filtering
- [x] Keyboard binding system — KeyCombo / KeyBinding / KeyboardModel with context-aware resolution
- [x] Notification toasts — visual toast overlay (bottom-right) with kind-colored indicators
- [x] Selection model — multi-select with focus depth (SelectionModel replaces selected_node)

### Should Ship

- [x] Drag-and-drop — node move-up/move-down reordering in inspector panel
- [x] Undo/redo UI — StatusBar with undo/redo buttons, labels, and document snapshot stack
- [x] Search — TF-IDF cross-collection search with sidebar results

### Modernization (Builder Unification)

- [x] B1: std-widgets — `app.slint` rewritten with `Button`, `LineEdit`, `Switch`, `GroupBox`, `Palette` theming (2026-04-18)
- [x] B2: Reactivity cleanup — vestigial color props removed, Palette theming only (2026-04-18)
- [x] B3: Builder/Shell merge — `BuilderNode` model drives native Slint rendering, no DSL string compilation (2026-04-18)
- [x] B4: HTML SSR separation — `HtmlBlock` trait + `HtmlRegistry` decouple HTML from `Component`; relay stays Slint-free (2026-04-18)
- [x] B5: Interactive builder — component palette, add/delete nodes, inline text editing (2026-04-18)

### Can Wait

- [ ] Print rendering — `renderForPrint()` equivalent
- [ ] Help system — HelpRegistry, tooltips, contextual help sheet

---

## Builder / Editor

### Shipped

- [x] Component trait with dual render targets (Slint DSL + HTML SSR)
- [x] ComponentRegistry DI
- [x] 17 starter components (heading, text, link, image, container, form, input, button, card, code, divider, spacer, columns, list, table, tabs, accordion)
- [x] SlintEmitter + document render pipeline
- [x] `slint-interpreter` compile + instantiate
- [x] Component palette — browsable/searchable 17-type picker in sidebar
- [x] Interactive builder surface — add/delete/reorder nodes, inline text editing

### Must Ship (MVP)

- [ ] Drag-drop resize — spatial drag-drop and resize within Slint (currently button-based reorder)
- [ ] Puck-Loro bridge equivalent — real-time CRDT sync of builder state

### Should Ship

- [ ] Layer/hierarchy panel — depth traversal with collapse
- [ ] Responsive layout — breakpoint-aware layout editing
- [ ] Remaining block types — charts, kanban, calendar, map, data display, etc.

---

## Data Layer

### Shipped

- [x] Loro CRDT behind `crdt` feature
- [x] GraphObject / ObjectRegistry / TreeModel / EdgeModel
- [x] VFS with SHA-256 dedup
- [x] CollectionStore (Loro-backed, feature-gated)
- [x] Undo/redo with batch merging
- [x] Clipboard with deep clone + ID remap
- [x] Atom<T> — fine-grained reactive cell with PartialEq-gated subscribers (20 tests)
- [x] CrdtSync — bidirectional CollectionStore-to-Atom bridge (19 tests)

### Must Ship (MVP)

- [x] Live Loro document sync in shell — bidirectional CRDT-to-UI binding
- [x] Reactive atom subscriptions — high-frequency UI update path

---

## Text & Code Editing

- [ ] Text editor — CodeMirror 6 equivalent in Slint (LoroText sync, language support, diagnostics, completions, hover)
- [ ] Markdown live preview
- [ ] Luau syntax highlighting
- [ ] Spell-check integration

---

## Relay / Server

React had 15+ Hono modules. Rust relay has 18 modules, 100 routes, full WebSocket protocol.

### Shipped

- [x] `GET /healthz` liveness probe
- [x] `GET /` + `GET /portals` landing/listing
- [x] `GET /portals/:id` SSR portal render
- [x] `GET /sitemap.xml` + `GET /robots.txt`
- [x] PortalStore with L1 static HTML
- [x] Auth module — password auth (PBKDF2-SHA256 register/login/change) + OAuth provider stub
- [x] Collection host — CRDT collection hosting, snapshot import/export
- [x] WebSocket real-time sync — full relay protocol (auth, envelope, collect, sync, presence, hashcash)
- [x] Rate limiting / DoS protection — token-bucket per IP (100 burst, 20/s, 10k LRU)
- [x] Admin routes — HTML dashboard, metrics snapshot, Prometheus `/metrics`
- [x] Capability tokens — issue/verify/revoke/list
- [x] Blind mailbox — offline peer message queuing (via relay router)
- [x] Router hub — relay routing + federation forwarding
- [x] Presence tracking — WebSocket presence protocol + HTTP snapshot
- [x] Webhook module — CRUD, delivery history, test fire
- [x] Vault hosting — publish/list/get/download + per-vault collection management
- [x] Signaling — WebRTC room management (join/leave/signal/peer listing)
- [x] Metrics — Prometheus text format + request counter + status histogram
- [x] Backup/restore — full state export/import
- [x] Directory — relay discovery feed (DID, modules, uptime, portals, vaults)
- [x] Portal templates — template CRUD (CSS, header, footer, card HTML)
- [x] Trust & Safety — peer trust graph, ban/unban, content reporting, flagged hash gossip
- [x] Escrow — deposit/claim lifecycle
- [x] Hashcash — proof-of-work challenge/verify
- [x] AutoREST — dynamic CRUD gateway for any collection
- [x] Push pings — device registration, send, wake
- [x] CSRF middleware — `X-Prism-CSRF: 1` on mutating `/api/*` routes
- [x] Body limit middleware — 1MB Content-Length cap
- [x] ACME/TLS — certificate + challenge management routes
- [x] Federation — announce, list peers, forward envelope, sync
- [x] Logs — ring buffer query/clear (stub, follow-on)
- [x] Email — transport status + send (stub, follow-on)

### Remaining

- [x] OAuth/OIDC — Google/GitHub redirect + callback (18th module: session state, identity linking, escrow derive/recover)
- [x] Hydration — L4 interactive portals (WebSocket hydration script injection)
- [x] Form submit — L3 portal forms (form/input/button components + POST handler)

---

## Network (prism-core)

### Shipped

- [x] `presence` — PresenceManager + TTL expiry + subscription bus (39 tests)
- [x] `relay` — RelayConnection state machine + auto-reconnect + 17-module system (29 tests)
- [x] `relay_manager` — pool + health tracking + selection strategies (23 tests)
- [x] `discovery` — VaultRoster + DiscoveryService + TTL sweep (27 tests)
- [x] `session` — SessionManager + TranscriptTimeline + PlaybackController (39 tests)
- [x] `server` — RouteSpec generator + OpenAPI 3.0 from ObjectRegistry (22 tests)

---

## Domain Models

- [x] Flux — 11 entity types, 7 relationships, 8 automation presets (38 tests)
- [x] Timeline — NLE engine with PPQN tempo map (67 tests)
- [x] Graph Analysis — dependency, topo sort, cycle detection, CPM (30 tests)

---

## Language / Scripting

### Shipped

- [x] Expression engine — tokenizer, Pratt parser, evaluator
- [x] Syntax scanner + AST
- [x] Codegen pipeline (TypeScript, C#, EmmyDoc, GDScript)
- [x] Markdown dialect (Prism-flavored, wikilink support)
- [x] Forms / field schemas
- [x] Language registry (`LanguageContribution<R, E>`)

### Shipped (Phase 4)

- [x] Luau parser — full-moon 2.2.0 Rust parser in prism-core (14 tests)
- [x] LuauSyntaxProvider — real diagnostics, completions, hover via full_moon (11 tests)
- [x] Luau browser runtime — daemon compiles to wasm32-unknown-emscripten with mlua vendored Luau
- [x] Debugger — `luau.debug.*` commands in prism-daemon: launch, breakpoints, stepping, inspect, eval, terminate (6 tests)
- [x] Visual script bridge — generalized `VisualLanguage` trait + `ScriptGraph` IR for bidirectional code↔graph editing (19 tests)
- [x] LuauVisualLanguage — decompile (source→graph) + compile (graph→source) + palette + validation (11 tests)
- [x] `SurfaceMode::VisualScript` — new surface mode for visual node-graph editors

### Remaining

- [ ] Visual script editor UI — Slint node-graph panel (scaffolding in place, needs Slint surface)

---

## Identity & Trust

- [x] W3C DID (Ed25519 + multisig)
- [x] AES-GCM-256 vault encryption + HKDF derivation
- [x] Privilege sets / manifest
- [x] Luau sandbox, hashcash PoW, peer trust graph, Shamir secret sharing, escrow, PBKDF2

---

## Admin & Monitoring

- [ ] Admin kit — HealthBadge, MetricCard, MetricChart, ServiceList, ActivityTail, UptimeCard widgets (Slint equivalents)
- [ ] Admin HTML renderer — `renderAdminHtml()` for server embedding
- [ ] Live metrics dashboard — kernel/relay/daemon telemetry

---

## Packaging & Distribution (Phase 5)

- [ ] Desktop packaging — `cargo-packager` + code signing
- [ ] Auto-updates — `self_update` crate
- [ ] Tray icon — `tray-icon` crate
- [ ] System notifications — `notify-rust`
- [ ] File dialogs — `rfd`
- [ ] Clipboard integration — `arboard`
- [ ] Keyring — `keyring` crate
- [ ] Daemon bundling — embed `prism-daemond` binary in installer
- [ ] Mobile targets — iOS (UIKit) + Android (android-activity) via `cargo-mobile2`

---

## Testing

### Shipped

- [x] 1,547 unit + integration tests passing
- [x] Integration tests — daemon (kernel, stdio, IPC)
- [x] Integration tests — relay (HTTP via tower)

### Remaining

- [ ] E2E test suite — React had 32 Playwright specs; need Slint equivalent
- [ ] UI regression testing — screenshot / visual diff

---

## Apps (built on the platform)

- [ ] Flux — operational hub (productivity, finance, CRM, goals, inventory)
- [ ] Lattice — game middleware (narrative, audio, entities, orchestration)
- [ ] Cadence — music production + education
- [ ] Grip — live production (stage plots, cue sheets, MIDI/DMX/OSC)

---

## Summary

### What's Done

The **engine and relay are built**. Daemon (13 modules, 6 transports),
core (1,190 tests, all 6 network modules shipped), relay (18 modules,
100 routes, full WebSocket protocol), CLI, build pipeline, CRDT, identity,
trust, domain models, language tooling (Luau parser, debugger, visual
script bridge), and the component registry with dual-target rendering
are all shipping. The Slint desktop + WASM pipeline works end-to-end with
hot-reload.

### What's Left for MVP

The gap is almost entirely in the **UI shell**:

1. **Interactive property editing** — unlock the builder feedback loop
2. **Text/code editor panel** — critical for any real work in the app
3. **Tabbed interface + command palette** — basic IDE ergonomics
4. ~~**More block types** — 5 starter components is not enough; need ~15-20~~ (**done** — 17 types)
5. **Drag-and-drop in builder** — spatial drag-drop and resize
6. **At least one domain app surface** — Work panel for Flux
7. **Desktop packaging** — users need an installable binary
8. **Live Loro sync in shell** — CRDT-to-UI binding for real-time collaboration
9. ~~**OAuth/OIDC** — relay password auth ships, but OAuth stubs need implementation~~ (**done**)

### What Can Ship After v1

Full 43-panel parity, 3D viewport, admin kit, mobile, visual script
editor UI (bridge layer shipped), all four apps, E2E test suite.
