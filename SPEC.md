# The Prism Framework: Complete Technical Specification

> **Version**: 0.1.0-draft
> **Status**: Architectural Blueprint
> **Last Updated**: April 2026

---

## Core Philosophy & Identity

Prism is a **Distributed Visual Operating System** and the architectural blueprint for what we call **Web 4.0**. Where Web 1.0 was the wild-west, Web 2.0 was the corporate take over, Web 3.0 was the finance-bro take over, Web 4.0 is the post-capitalist anarchist-socialist take over. Where Web 3.0 operates on the principle of "no trust", Web 4.0 is diametrically opposed, assuming "maximum trust" (and which does not also mean "security free").

In Prism, Applications are **Lenses** and Automations/Intelligence are **Actors** — all operating on a unified, sovereign **Object-Graph** residing on local hardware. The traditional cloud is reduced to a blind, encrypted routing fabric.

**The defining insight**: Every Prism app is an IDE. There is no wall between "using" and "building." The difference between a consumer app and a developer tool is a single toggle — the **"Glass Flip."**

### The Prime Directives

1. **Local-First, File-Over-App**: User data is sacred. The ultimate source of truth is the local file system — schemas (`.yaml`, `.json`, `.md`, `.loom`), assets (images, audio, video, PDFs, 3D models), and binary files of any type. Applications are merely lenses through which this data is viewed, edited, and federated. If the Prism ecosystem disappeared tomorrow, users retain 100% functional local files.

2. **Blockchain-Free Decentralization**: Data autonomy and federation are achieved through **W3C Decentralized Identifiers (DIDs)** and **End-to-End Encrypted CRDTs** (Conflict-free Replicated Data Types). Zero crypto-tokens, zero ledgers, zero consensus bottlenecks.

3. **Data Transparency & Ownership**: All data is structurally transparent and interoperable. A game dialogue tree (`.loom`), a productivity Kanban board, and a university music assignment share the same underlying Object-Graph architecture.

4. **Web 4.0 Framing**: Prism is explicitly **not** Web 3.0. Web 3.0 carries crypto/token baggage. Prism inherits open protocols from Web 1.0, high-speed UX from Web 2.0, and cryptographic sovereignty from Web 3.0 — but eliminates the blockchain bottleneck by making every device its own sovereign authority.

---

## The 5-Pillar Taxonomy

### 1. Prism Core (The Glass & Logic)

The client-side execution environment, split into two internal layers to protect business logic from UI framework churn:

**Layer 1 — Agnostic Primitives** (Pure TypeScript atoms):
- Zustand state slices (atomic, subscribing to specific Loro node IDs)
- XState interaction machines (tool mode management)
- Loro CRDT subscriptions (reactive state from the Object-Graph)
- Mathematical routing logic
- *Knows what the data is, but not how to draw it*

**Layer 2 — The Renderers** (Visual Implementation):
- **React + TailwindCSS**: The UI framework and utility-first styling
- **Puck** (`@measured/puck`): CSS-based drag-and-drop layout builder for UI composition
- **PixiJS** (`@pixi/react`): GPU-accelerated 2D rendering for spatial canvases, timelines, and performance-critical views
- **@xyflow/react** (React Flow): MIT-licensed node-wire graph editor for logic routing and visual programming
- **CodeMirror 6**: Code/text editing for **all contexts** including Studio — modular, mobile-friendly, à la carte, and LSP-compatible

**Key architectural decisions**:
- **No Monaco Editor anywhere.** CodeMirror 6 is the sole text/code editor across the entire framework. At ~300KB core (vs Monaco's 5-10MB), it runs on mobile, supports LSP bridges via `@codemirror/autocomplete` and `@codemirror/lint`, and is used in production by Sourcegraph, Replit, and Chrome DevTools. Its plugin system allows loading only what each context needs.
- The hidden buffer is the **Loro CRDT**, not any editor instance. CodeMirror displays a projection of Loro state; KBar is King for all command dispatch.
- **Preact Signals** (`@preact/signals-react`) provide SolidJS-like reactivity inside React components, bypassing VDOM diffing for high-frequency updates.
- During active dragging, vanilla JS `onMouseMove` + direct CSS `transform` via `ref.current.style.transform` bypasses React entirely. Only `onMouseUp` writes final coordinates to Zustand → Loro.

### 2. Prism Daemon (The Physics & Metal)

Renamed from "Prism Server" to avoid confusion with Web 2.0 server concepts. It's a silent, relentless background physics engine running directly on sovereign hardware.

- **Stack**: Rust, Loro CRDT, VFS (`object_store` crate), local SQLite indexing
- Handles CRDT merging, VFS routing, Binary Forking, OS file watching (`notify` crate)
- Executes local Actors (Whisper, Python workers, local LLMs) as sidecars
- Talks to hardware protocols (Art-Net, VISCA, DMX, OSC, MIDI) for show control
- Bridges the React/Vite frontend via **Tauri IPC** (`invoke('write_crdt', {...})`)

### 3. Prism Relay (The Network & Bridge)

Open-source routing infrastructure and Web 2.5 translator. **Any server running Relay software is a zero-knowledge router** — not just Nexus.

- **Stack**: Node/Rust, E2EE (libsodium/X25519), WebRTC signaling (LiveKit), Hono (SSR via JSX), Jose (JWT auth)
- **Zero-knowledge store-and-forward** routing with Blind Mailbox queues for offline peers
- Hono JSX SSR for Sovereign Portals (served from **any** Relay, not just Nexus)
- **AutoREST gateway** with Webhook support (outgoing HTTP when CRDT changes — for Zapier/Slack integrations)
- Web 2.0 OAuth *and/or* Web 3.0 Blind Escrow recovery for non-technical users
- **Sovereign Indexer**: XML sitemaps + AutoREST endpoints for SEO — a primitive of the open Relay protocol, not a Nexus-only feature
- **Blind Pings**: Empty "alarm clock" pings via APNs/FCM to wake mobile Capacitor apps for background CRDT syncing without exposing payload data

**E2EE Architecture**: Loro's `export({ mode: "update" })` produces `Uint8Array` blobs. Before transport, the Prism Daemon encrypts these blobs using `libsodium-wrappers` (X25519 key exchange + XChaCha20-Poly1305 symmetric encryption). The Relay receives only opaque ciphertext. The `@loro-extended` framework (by SchoolAI) provides the sync scaffolding — schemas, network adapters (SSE + WebSocket + WebRTC), persistence (PostgreSQL), and reactive subscriptions — built directly on Loro with no Yjs dependency.

### 4. Prism Studio (The Universal Host & IDE)

The "Universal Host" application users actually download. Contains the full dual-engine meta-builder.

- **Stack**: Vite SPA bundled in Tauri 2.0 (Desktop) and Capacitor (Mobile)
- Every Prism app is a Studio instance at its core — the IDE is always present beneath the surface (the "Glass Flip")
- Can deploy and manage self-hosted Relays from within Studio
- Power users (devs, enterprises, tinkerers) live here

### 5. Prism Nexus (The Cloud & SaaS)

Monetized commercial wrapper — Studio + Relay(s) + App Repo as a turnkey subscription.

**Three products in one:**

1. **Managed Relays** — 99.999% uptime, global edge routing, SFU WebRTC fallback, Encrypted Escrow for key recovery
2. **Nexus Cloud Studio** — A full browser-based Prism Studio for Web 2.0 users; builds sites/apps entirely in the cloud like a "new-age Wix/SquareSpace" without ever touching local-first concepts
3. **Central App Repo** — Marketplace for Manifests, Prefabs, full apps, UI components


**The "Eject" Button**: Sites/apps built on Nexus SaaS are architecturally identical to local Prism apps. A user can click "Eject to Local," receive a `.prism` archive, import it into the Universal Host, and point it to a free self-hosted Relay. No other builder on earth can offer this.

Nexus also serves users who want to build websites totally in a vacuum from Prism apps — just using the Hono SSR/Puck infrastructure as a pure SaaS web builder. This is the **Trojan Horse**: they get a powerful modern web builder; Prism secretly gives them a Web 4.0 escape hatch when they're ready.

---

## The Omni-IDE: Every App IS an IDE

Because an "app" is just a YAML Manifest and a Loro CRDT graph interpreted by the Universal Host, there's no compiled binary. The tools to inspect and rebuild are always present beneath the surface.

### The Glass Flip

- **Consumer State**: User sees invoices, charts, dashboards — CodeMirror is hidden, the logic canvas is hidden
- **Creator State**: User hits a hotkey (CMD+K via KBar). Bounding boxes appear around Puck components. A side panel reveals CodeMirror showing the Lua script behind the current view
- **The Agency**: User rewrites a Lua script right there in the app, permanently upgrading their local instance without emailing a developer

**Implications:**
- A Lattice game player can open the IDE toggle and rewrite NPC dialogue trees *while playing*
- A Cadence student can wire their MIDI keyboard to a custom visualization they just coded inline
- This is **Smalltalk for the 2020s** — the system is made of living, editable objects

### The Capability Token Sandbox

When downloading apps from Nexus, the Lua sandbox has **zero network access** by default. Third-party apps need explicit user-signed Capability Tokens to access network, storage, or hardware.

---

## The Dual-Engine Meta Builder

Two distinct builders sharing the same Loro CRDT data source:

### Builder 1: The Layout Engine (The "Glass" / Viewport)

**Purpose**: Building UI, responsive web pages, Sovereign Portals (like Webflow/Squarespace)
**Paradigm**: DOM-based, CSS-driven, responsive flexbox layout
**Analogy**: Webflow, Figma layouts, Xcode Interface Builder

**Core Libraries**:
- `@measured/puck` — Headless drag-and-drop React visual builder; maps 1:1 with Loro JSON tree. Integrated with Prism via: Puck `onChange` → extract diff → push to Loro CRDT; Loro state → feed back into Puck `data` prop (Puck never saves, strictly a visual manipulator). KBar calls Puck API programmatically; Puck's native panels can be hidden. CodeMirror shows YAML serialization of Puck state in 1:1 sync
- `react` / `react-dom` — UI framework
- `tailwindcss` — Utility-first styling
- `@dnd-kit/core` + `@dnd-kit/sortable` — Headless drag-and-drop for the Layers panel (Photoshop/Figma-style AST tree management)
- `react-resizable-panels` — VS Code-style draggable split panes for IDE layout
- `@rjsf/core` (React JSON Schema Form) — Auto-generates complex property panels from YAML schemas; the "Object Properties" sidebar engine
- `radix-ui` — Headless, accessible IDE primitives (dropdowns, context menus, tooltips)
- `esbuild-wasm` — In-browser TS/JSX compiler; user writes component in CodeMirror → compiles to executable JS in milliseconds → dynamically injects into Puck resolver
- `react-moveable` (Daybrush) — Photoshop-style bounding box transform controls (drag, resize, scale, rotate, warp, snap to grid/guides)
- `@scena/react-guides` — Horizontal/vertical rulers at canvas edges (same author as react-moveable)
- Shadow DOM / iframe isolation (native) — Sandboxes untrusted third-party Puck components from Nexus App Registry

### Builder 2: The Spatial Node Graph (The "Wiring" / Object Graph)

**Purpose**: Visual programming, architecture diagrams, storyboarding, spatial document viewing
**Paradigm**: Infinite spatial canvas, absolute X/Y, orthogonal wire routing, bounding-box collision
**Analogy**: MaxMSP, Unreal Engine Blueprints, Miro, Obsidian Graph View, Blender Node Editor

**Core Libraries** (all MIT-licensed):
- `@xyflow/react` (React Flow) — The foundational node-wire graph engine (MIT, used by Stripe, Typeform, Stately). Handles structured node placement, edge routing, viewport math, multi-select, and custom React node/edge rendering. Supports custom edge types for Hard Ref and Weak Ref wire rendering
- `@pixi/react` (PixiJS) — Raw GPU 2D rendering (MIT). Used for the high-performance "Bird's Eye" overview, NLE timeline rendering, and any canvas that exceeds DOM rendering limits. The escape hatch when React Flow's DOM-based nodes can't keep up
- `elkjs` (Eclipse Layout Kernel) — Background Web Worker; orthogonal layout math, 90-degree wire routing, obstacle avoidance, margin alignment; used when nodes are dropped into "UML Frames"
- `graphology` — Background graph math; circular dependency detection, shortest paths, orphaned file analysis without rendering
- `sigma.js` — WebGL renderer for massive network graphs (10,000+ Vault nodes in "Bird's Eye View" mode like Obsidian)
- `paper.js` — Vector math brain; bezier curves, boolean operations (union/subtract like Photoshop Pathfinder), spline calculations
- `react-moveable` — Shared with Builder 1 for transform handles on spatial nodes
- `@use-gesture/react` — Raw pointer event physics (momentum, velocity, pinch, pan). Handles the infinite canvas camera math for PixiJS views
- `react-spring` — Animates camera coordinates with native "Apple-feel" momentum
- `XState` — Finite state machine for tool mode management (Hand vs. Select vs. Edit vs. Pen); prevents click event chaos in multi-layer canvases
- `@floating-ui/react` — Ensures context menus/tooltips render above everything, never clipped by node boundaries

**Node Internals** (what renders inside spatial nodes):
- `codemirror` v6 — Code editing inside nodes
- `react-markdown` + `remark-gfm` — Obsidian-style rich text rendering
- `@glideapps/glide-data-grid` — WebGL canvas grid for CSV/SQL tables inside nodes (millions of rows without lag)

**Wire Types**:
- **Hard Refs** (solid lines) — Structural parent-child ownership; SVG `<path>` with Elk-calculated orthogonal waypoints
- **Weak Refs** (dashed Bezier curves) — Semantic wiki-links like Obsidian; typing `[[` in a Markdown node triggers KBar search, draws dashed curve to referenced file
- SVG `<marker>` tags for decorators (arrowheads, crow's feet, diamonds)
- Reroute nodes: invisible waypoints user can drag to clean up corners
- Configurable: straight, curved, step, animated, styled per relationship type

**Layout Paradigms**:
- **Free-Form Zones** (Miro Mode) — Absolute X/Y saved to Loro
- **Structured Zones** (UML Frames) — Nodes surrender to elkjs auto-routing when dropped into a Frame
- **Programmatic Graphs** — New nodes follow new files in the filesystem (OS Watcher → spatial indexer → elkjs finds open patch of coordinates → animates node appearing); reverse: moving a node on canvas writes metadata to the actual file

### Builder 3: The 3D Viewport (Live Production / CAD)

For the Live Production app (Grip) and any Prism context needing 3D spatial authoring:

- `@react-three/fiber` (R3F) — React renderer for Three.js; 3D objects as React components; maps Loro CRDT JSON to 3D scene exactly like Puck maps to DOM
- `@react-three/drei` — Gizmos (`<TransformControls>`), orbit cameras, environment maps, lighting presets
- `@react-three/rapier` — Rust-based physics engine (WASM) for structural simulation (e.g., "will this speaker array tip over?")
- `opencascade.js` — OpenCASCADE C++ CAD kernel compiled to WASM; true parametric CAD (NURBS, solid geometry, STEP file import); runs in Web Worker managed by Prism Daemon; tessellates math to polygons → R3F draws them
- `three-bvh-csg` — Lighter alternative for CSG (constructive solid geometry — add/subtract basic shapes)
- Three.js Shading Language (TSL) / Node Materials — Builder 2's @xyflow logic canvas wires → TSL Node Objects → compiles to WebGPU/WebGL shaders in real-time (Blender Shader Editor equivalent)

---

## The NLE / Timeline System (Grip "Show Control")

**Decision**: Not using OpenDAW SDK directly as a monolith. Using it with the **"Local Optimistic Flush"** bridge pattern, OR building from deconstructed primitives.

**The Two-Brains Problem**: Audio SDKs assume they own the state. Loro owns the state in Prism.

**Recommended approach (Local Optimistic Flush)**: OpenDAW handles the present frame; Loro handles history. Drag freely in OpenDAW → `onMouseUp` → diff → flush to Loro. Fast, pragmatic, ships fast.

**From-scratch primitive stack** (if not using OpenDAW):
- `tone.js` — Master transport clock; sample-accurate scheduling; reads Loro state; schedules cues with look-ahead math independent of UI thread
- `@elemaudio/core` + `@elemaudio/web-renderer` — Declarative/functional audio DSP (React for sound); no internal state; maps Zustand/Loro state directly to audio graph; WASM C++ backend. Note: web renderer releases have been infrequent — evaluate maintenance status before committing
- `@pixi/react` (PixiJS) — GPU canvas rendering for timeline tracks, audio waveforms, Lua cue blocks (DOM cannot handle 5,000 cues at scale)
- `peaks.js` (BBC R&D) — Fast Canvas/WebGL audio waveform rendering; native "Marker" and "Segment" API for Slate markers and Lua cues
- `d3.js` — Custom timeline math and data visualization if needed
- **WebCodecs API** (native) — Frame-accurate MP4 extraction; paints video frames to canvas exactly when Tone.js clock fires; zero frame drift over 3-hour theatrical performances
- **Web Audio Modules (WAM)** standard — Browser VST standard; allows C++/Rust audio processors (WASM) to load into signal chain; host a Plugin Registry on Nexus

**Show Control Hardware** (via Rust Daemon):
- **Art-Net / sACN** — DMX lighting over UDP
- **VISCA over IP** — Motorized PTZ cameras (pan/tilt/zoom)
- **OSC** (Open Sound Control) — QLab, TouchDesigner, custom apps
- **MIDI** — Soundboards, Ableton, synthesizers
- Lua script outputs generic values → Rust Daemon translates to hardware packets (never through browser sandbox)
- **The principle**: CRDT stores the **Instruction**; Daemon executes the 120 transient UDP packets for a fade (NOT stored in Loro)

---

## The Federated Object-Graph

Prism replaces "Files and Folders" with a semantic mesh of nodes connected by weak references (`prism://[Vault]/[Collection]/[ID]`).

### Topology

- **Identity (DIDs)**: W3C Decentralized Identifiers managed via Veramo. Your "login" is a cryptographic key in your hardware's Secure Enclave, not a password in a database. Organizations use Multi-Sig DIDs.
- **Vaults (The Iron Box)**: The physical security boundary. A local directory natively locked and encrypted at rest via AES-GCM-256. Files appear as unreadable blobs unless the Prism Daemon is unlocked.
- **Collections**: "Hard" typed CRDT arrays (e.g., `Contacts`, `Audio_Busses`, `Tasks`).
- **Manifests (Workspaces)**: YAML files containing weak references to Collections. A "JJM Productions" workspace in Flux is just a Manifest pointing to various data nodes. A professional and personal Manifest can both point to the same Contacts Collection, filtering via tags.
- **Ghost Nodes**: If a shared Manifest links to a node the user lacks the keys to decrypt, the UI renders a locked placeholder.

### The Virtual File System (VFS)

Prism physically decouples the **Lightweight Graph** (metadata/text/CRDTs) from **Heavy Binaries** (4K video, 3D assets, audio) using the `object_store` Rust crate.

**Storage Adapters:**
- **Sovereign Local**: Direct NVMe/SSD storage on the device
- **Self-Hosted NAS**: SMB/NFS mounts for studio LAN environments
- **BYOK Cloud**: Google Drive, S3, or Dropbox acting strictly as "dumb" encrypted blob stores
- **Ghost Assets**: CRDT metadata ensures the UI knows a massive file exists instantly. The VFS "hydrates" on-demand — streaming bytes so a 10GB video doesn't crash a mobile device

**The Cloud is NOT the sync engine.** Devices sync natively peer-to-peer via the Relay. Cloud storage is purely an asynchronous buffer and asset host — decoupled from the transport.

---

## The Lua Scripting Layer

Prism uses **standard Lua 5.4** as its universal scripting language. Same scripts run identically across all surfaces.

| Surface | Lua Engine | Notes |
|---------|-----------|-------|
| Prism Daemon (Rust) | `mlua` with `lua54` + `vendored` feature | Native Rust FFI; async/await; serde; WASM via emscripten target |
| Browser (PWA/Web) | `wasmoon` | Official Lua 5.4 compiled to WASM; JS interop; Promise support |
| Godot 4.5+ | `lua-gdextension` (gilzoide) | Lua 5.4 GDExtension; MIT licensed |
| Unreal Engine | LuaMachine | C++ Lua plugin |
| Unity | NLua/KeraLua (or MoonSharp for 5.2 compat) | C# P/Invoke to native Lua |

**Why Lua 5.4 over Luau**: Luau introduces proprietary extensions (type annotations, modified `collectgarbage`, no `goto`) that fragment the codebase. A Luau script written in the desktop app won't run unmodified in Godot's GDExtension or wasmoon's browser runtime. By targeting strict Lua 5.4, the same script file runs identically everywhere. mlua's optional Luau feature can still be used for enhanced sandboxing on the Daemon when running untrusted code.

---

## The Deployment Paradigm: The Universal Host

Prism uses a **"Russian Doll" distribution model**. Layer 0 provides a standardized runtime (The Host) that injects hardware capabilities, while apps provide the semantic logic.

### Multi-Surface Shells

- **Desktop**: Tauri 2.0 (Rust) — Native binary, direct OS access, GPU acceleration
- **Mobile**: Capacitor (Swift/Kotlin FFI bridges to Rust)
- **Web (Chrome/Edge)**: WASM core uses the **File System Access API** to read/write directly to the host's actual hard drive, bypassing the browser sandbox. Functionally identical to the Tauri desktop app, running in a tab
- **Web (Safari/PWA)**: WASM core uses **OPFS** (Origin Private File System). Runs fast and entirely local, but files are inside the browser's sandbox
- **Web (Guest/Portals)**: Runs entirely in **Ephemeral RAM**, relying on a Relay to route CRDT diffs back to the host. State is discarded when the tab closes
- **Engine Embed**: C++ implementation (Lua 5.4 / Yoga / FlatBuffers) allowing Lattice modules to run natively inside Unreal/Godot game loops

### The Stripped Standalone

A branded binary (e.g., "Flux Desktop") that is the Prism Host hardcoded to a specific app-path, hiding developer modules for a streamlined consumer UX.

---

## Studio as a Self-Replicating Meta-Builder

Prism Studio is not only **the universal host for apps** — it is also **the factory that produces them**. The process of making Flux, Lattice, Cadence, or Grip ends inside Studio: the user composes an **App Profile**, selects build targets, and Studio emits a web build, a Tauri desktop bundle, and Capacitor mobile apps. The same pipeline deploys Relays (via the composable builder pattern) as Dockerized services or static Node bundles.

This is the **Glass Flip at the ecosystem level**: just as any running app can flip into its IDE, the IDE can flip into a factory that produces *other* apps from the same codebase and plugin set.

### The Core Principles

1. **One codebase, many focused apps**: Flux Desktop is not a fork of Studio — it *is* Studio, launched with a pinned App Profile that filters the visible plugin surface, lens registry, and KBar commands. All runtimes remain injectable; the "focus" is purely configuration.
2. **Never compile the framework twice**: Web, Tauri, and Capacitor share the same Vite output. The shell (browser/Tauri/Capacitor) is the only thing that changes.
3. **Option to "Run in Studio" is always preserved**: Even a stripped "Flux Desktop" exposes a hidden toggle to promote itself back to the full universal host, because the underlying binary is identical.
4. **Relay deployment is just another build target**: The same Studio lens that composes Flux can compose a Relay (pick modules, generate `relay.config.json`, emit Docker/PM2/systemd artifact).

### The App Profile

An **App Profile** is a YAML/JSON document (`.prism-app.json`) that pins a slice of Studio to a specific app:

```json
{
  "id": "flux",
  "name": "Flux",
  "version": "0.1.0",
  "plugins": ["work", "finance", "crm"],
  "lenses": ["editor", "canvas", "record-browser", "record-browser", "automation"],
  "defaultLens": "record-browser",
  "theme": { "primary": "#6C5CE7", "brandIcon": "flux.svg" },
  "kbarCommands": ["new-invoice", "start-timer", "open-contact"],
  "manifest": "flux.prism.json"
}
```

When Studio boots with a profile, the Kernel:
- Installs only the listed `PluginBundle`s (not the default six)
- Installs only the listed `LensBundle`s (hiding the Glass Flip's developer lenses unless explicitly re-enabled)
- Runs only the `StudioInitializer`s the profile opts into (seed content, default templates, …)
- Overrides the default theme and brand icon
- Narrows KBar to the profile's `kbarCommands` list at the App depth

### Kernel Composition & Self-Registering Bundles

Studio's Kernel is a pure composition root. Nothing inside a panel, plugin,
or initializer reaches *up* into the host — instead, every subsystem
exposes a **self-registering bundle** that the host hands to the Kernel at
construction time. This is what makes the App Profile filter above a
one-liner: profiles are just subsets of bundle arrays.

There are three bundle kinds, all sharing the same `install(ctx) => uninstall`
contract:

| Bundle | Module | Installs into | Registers |
|--------|--------|---------------|-----------|
| `PluginBundle` | `@prism/core/plugin` | `ObjectRegistry` + `PluginRegistry` | Entity defs, edge defs, plugins with views/commands/keybindings |
| `LensBundle` | `@prism/core/lens` | `LensRegistry` + a component map | A `LensManifest` and its React component for a single lens |
| `StudioInitializer` | `@prism/studio/kernel` | The fully-constructed `StudioKernel` | Templates, seed/demo data, action handlers — any post-boot side effect |

`LensBundle` is generic over its component type (`LensBundle<TComponent>`)
so Layer 1 stays React-free; Studio specializes it to
`LensBundle<ComponentType>` at its own layer.

The Kernel's factory reads like a menu:

```typescript
const kernel = createStudioKernel({
  lensBundles: createBuiltinLensBundles(),
  initializers: createBuiltinInitializers(),
});
```

- `createBuiltinLensBundles()` returns the 40 built-in Studio lens bundles,
  each colocated with its panel (`panels/editor-panel.tsx` exports
  `editorLensBundle`, etc). Adding a new lens is one file + one line in
  `lenses/index.tsx`.
- `createBuiltinInitializers()` returns the post-boot hooks (page
  templates, section templates, demo workspace). Initializers receive the
  live kernel, so they can freely call `registerTemplate`, `createObject`,
  etc. The kernel owns their disposers.
- The host (`App.tsx`) no longer seeds data, registers templates, or
  wires a parallel lens registry — it just constructs the kernel and
  reads `kernel.lensRegistry`, `kernel.lensComponents`, `kernel.shellStore`.

The payoff is that layers flow strictly bottom-up: plugins don't know
about the kernel, panels don't know about `App.tsx`, and an App Profile
filters by passing a subset of the canonical bundle arrays. The same
pattern scales to third-party apps — a plugin package just exports its
own `LensBundle[]` / `PluginBundle[]` / `StudioInitializer[]`, and the
host concatenates them into the `createStudioKernel` call.

An **unprofiled** Studio is the universal host — full IDE, all plugins, no branding.

### Build Targets

| Target | Shell | Output | Use Case |
|--------|-------|--------|----------|
| `web` | Vite SPA | Static `dist/` directory | Hosted on any CDN or Relay Sovereign Portal |
| `tauri` | Tauri 2.0 | `.dmg` / `.msi` / `.AppImage` / `.deb` | Desktop distribution |
| `capacitor-ios` | Capacitor + Swift | `.ipa` (via Xcode) | iOS App Store or TestFlight |
| `capacitor-android` | Capacitor + Kotlin | `.apk` / `.aab` | Google Play or sideload |
| `relay-node` | Node server | `dist/` + `relay.config.json` | Self-hosted Relay (PM2/systemd) |
| `relay-docker` | Multi-stage Dockerfile | OCI image tarball | Docker Hub, private registry |

### The Build Plan

The Studio BuilderManager converts an App Profile + target into a deterministic **BuildPlan**: a list of ordered `BuildStep` entries (each one a command, a file to emit, or a Tauri IPC call into the daemon). BuildPlans are inspectable in the UI before execution and serializable for CI.

```typescript
interface BuildPlan {
  profileId: string;
  profileName: string;
  target: BuildTarget;
  steps: BuildStep[];              // emit-file, run-command, or invoke-ipc
  artifacts: ArtifactDescriptor[]; // expected outputs
  env: Record<string, string>;
  workingDir: string;              // resolved against monorepo root
  dryRun: boolean;                 // true = preview only, no side-effects
}

type BuildStep =
  | { kind: "emit-file"; path: string; contents: string; description: string }
  | { kind: "run-command"; command: string; args: string[]; cwd?: string; description: string }
  | { kind: "invoke-ipc"; name: string; payload: Record<string, unknown>; description: string };
```

### The Execution Model

BuildPlans are executed by the **Prism Daemon** (Rust), not the browser, because they call `cargo`, `pnpm`, `tauri`, and `capacitor` CLIs. Studio dispatches **one step at a time** via `invoke('run_build_step', { step, workingDir, env })`, which maps to `prism_daemon::commands::build::run_build_step`. The daemon resolves relative paths against `workingDir`, executes the step (`emit-file` writes the file + creates parent dirs; `run-command` spawns a child process with the plan's env applied and captures stdout/stderr; `invoke-ipc` is reserved for cross-command chaining), and returns `{ stdout?, stderr? }`. Studio's `BuilderManager.runPlan` walks the step array, threading the execution context through, halting on the first failure.

In the browser-only fallback (pure Vite SPA with no daemon), the BuilderManager swaps in a **dry-run executor** that marks `emit-file` steps successful (buffering their contents into `stdout` for preview) and skips `run-command`/`invoke-ipc` entirely. All vitest coverage runs through either the dry-run executor or a Node-backed invoke fn that mirrors the daemon's contract faithfully (`builder-manager-e2e.test.ts`), so the pipeline is proven end-to-end without spawning real `vite`/`tauri`/`cap` processes in unit tests.

### The App Builder Lens

A new Studio lens (`app-builder`, shortcut `Shift+B`) exposes the pipeline in the UI:

- **Profile editor** — select base profile (Flux, Lattice, Cadence, Grip, Custom, or Relay), toggle plugins, edit metadata
- **Target selector** — checkboxes for web, Tauri, Capacitor iOS/Android, Relay Node, Relay Docker
- **BuildPlan preview** — rendered list of steps and expected artifacts
- **Run / Dry-Run toggle** — execute via daemon or emit plan JSON for inspection
- **Progress log** — streaming output from each BuildStep with status badges

### Why This Matters

- **Ship-ability**: The same monorepo produces the flagship Studio *and* four focused apps without duplicating business logic.
- **Discoverability**: Users adopt a focused app (Flux) without confronting the full Glass IDE; power users flip it open.
- **Consistency**: Every app inherits Studio's data layer, CRDT, sandbox, and trust stack for free.
- **Self-hosting**: Non-technical users can run a Relay without touching a terminal — "Click to Compose Relay → Click to Build Docker → Click to Deploy."
- **Relay symmetry**: The same UI used to compose an app composes a Relay. The builder pattern (`.use(module)`) is exposed as a visual module picker.

---

## Sovereign Portals (The Meta-CMS)

Prism replaces traditional CMS platforms by acting as a sovereign backend. Designed in Studio and routed by **any** Relay, these scale across four levels:

**Level 1 — Read-Only Documents**: SSR HTML snapshots (invoices, reports, syllabi)

**Level 2 — Crafted HTML Views**: Stylized dashboards with real-time CRDT diff-updates. Update a project timeline locally → Relay pushes diff → client's public dashboard animates without refreshing

**Level 3 — Interactive Forms & Inbound Data**: Portals using Ephemeral DIDs. Client submits an intake form or signs a document → E2EE CRDT diff routes through Relay → updates local Vault instantly

**Level 4 — Complex Webapps, IoT & Games**:
- IoT: Home automation dashboards bridging external sensor data into local Lua logic
- Media: Source-agnostic audio mixing (internal Relay feeds or external S3/YouTube/SoundCloud/BandCamp streams)
- Gaming: Multiplayer JS/Three.js games where player state syncs via the P2P mesh and Relay queues
- Live Production: Real-time show control dashboards with hardware integration

### Web 2.0 Feature Parity on Relays

**Deploying Websites**: Tag a CRDT Collection as "Public" → generate Portal Manifest → deploy to Relay. Relay handles Let's Encrypt SSL, custom domain DNS, Hono JSX cache invalidation. Local Vault update → CRDT diff → Relay → portal re-renders from CRDT state → website updates in milliseconds

**AutoREST API Gateway**: Studio generates scoped Capability Token. Relay exposes standard REST or GraphQL. External services see standard HTTP → Relay translates to CRDT operation. **Webhook support**: outgoing HTTP when CRDT changes (for Zapier/Slack)

**Web 2.0 Auth & Recovery (Blind Escrow)**: User authenticates via Google/GitHub OIDC on Relay. Relay derives "Escrow Key" from user password + high-entropy salt from OAuth token. App encrypts master Vault key with Escrow Key → sends encrypted payload to Relay. Relay never sees the raw master key; breach of Relay DB = useless encrypted blobs

**SEO**: Hono JSX on Relay handles all SEO natively. Portal rendering injects `<title>`, `<meta description>`, OpenGraph/Twitter Card tags from Loro state. Auto-generated `sitemap.xml` and `robots.txt` from public portal graph nodes. Each portal page includes `og:title`, `og:description`, `og:type`, `og:url`, and structured data for search engines

---

## The Neural System (Context & Intent)

### KBar is King

A global CMD+K dispatcher (modified KBar) intercepts at the OS level. It interrogates focus depth (**Global → App → Plugin → Cursor**) to surface contextual AutoREST commands and semantic visual previews.

- **Global Depth**: "Open a different Vault," "Switch to Lattice"
- **App Depth**: "New Invoice in Flux," "Start Video Call"
- **Plugin Depth**: "Add Reverb Bus" (when Canto is active)
- **Cursor Depth**: "Refactor Variable" (when cursor is inside CodeMirror in a Lua file)
- **AI Prompt State**: "Ask Prism AI" → Local model processes the current CodeMirror buffer context

### CodeMirror Integration (Subordination)

When the user's cursor is inside a CodeMirror text buffer, KBar dynamically queries the Prism Syntax Engine's LSP and injects code-specific actions ("Refactor", "Format Document", "Go to Definition") into the global KBar list. One unified palette rules the entire ecosystem.

### Prism Syntax Engine

A background Language Server Protocol (LSP) running in a Web Worker. As schemas are edited in YAML, it generates `.d.lua` type definitions in-memory for instant autocomplete. This powers both KBar suggestions and CodeMirror IntelliSense (via `@codemirror/autocomplete` + `@codemirror/lint`) without requiring a build step.

---

## Layer 0B & 0C: The Unified Process System (Actors)

Prism unifies **Automation (0B)** and **Intelligence (0C)** into a single, federated compute pipeline. Both are modular, opt-in "employees" executing against the CRDT graph.

### Shared Execution Targets

Whether running a Python data-scraper or a massive LLM, Prism routes the process based on hardware capability:

1. **Sovereign Local (Prism Server/Daemon)**: Runs directly alongside the Rust daemon. Zero network latency, absolute offline capability, guaranteed privacy.
2. **Federated Delegate (Compute Relay)**: Mobile devices delegate heavy compute to a trusted Relay on the network (e.g., home server with GPU) via E2EE routing.
3. **External Provider (API Bridge)**: Strips data to essential context, wraps in a Capability Token, routes to third-party APIs (Stripe, Claude, Gemini, etc.)

### Layer 0B: Automations (The Process Engine)

- **The Process Queue**: A persistent, CRDT-backed queue defining background tasks as JSON logic-references and payloads
- **Language Runtimes**:
  - **TypeScript (TS)**: High-performance scripting via a hardened Deno/Tauri runtime
  - **Lua 5.4**: Micro-latency guard scripts via mlua (Rust) or wasmoon (browser)
  - **Python**: Integrated via a Minimal Rust Sidecar, unlocking NumPy/SciPy without bloating the core

### Layer 0C: Intelligence (The Sovereign Mind)

- **Native Local AI**: Ships with native support for Ollama. Qwen 3.5 Omni is the default flagship local provider — natively omnimodal (text, images, long-form audio, video)
- **The Provider Interface**: Users can toggle between Sovereign Local (Ollama), Federated Compute (home-server Relay), or External API (Claude, Gemini, ChatGPT). Prism AI is just a provider, ultimately
- **Inline Intelligence**: Integrates into the Viewport to provide Ghost-Text in CodeMirror and contextual reasoning across the OS
- **Zero-Data Leak**: No data is sent to an AI provider unless the specific Manifest is opted-in and the Capability Token is signed by the user

---

## The Communication Fabric (Events-as-Objects)

Live communication is a native data primitive embedded directly into the Object-Graph.

### Session Nodes

A meeting is a managed CRDT Collection containing text (`.md`), annotations (`.yaml`), and binary refs (`.mp4`). When archived, these become searchable graph objects like everything else.

### Self-Dictation (The Diarization Fix)

Using Whisper.cpp (running as an Actor — local Daemon preferred, can delegate to Compute Relay or Nexus GPU), every participant transcribes their own microphone locally on their own GPU. This:
- Guarantees 100% cryptographic attribution (who said what)
- Distributes the AI compute load (50-person meeting = same compute per person as 2-person meeting)
- Gets uncompressed, lossless audio before any WebRTC degradation

### The Listener Fallback

If a guest joins via a low-power PWA, the framework dynamically negotiates a "Compute Host" among connected desktop peers to silently handle that guest's transcription stream.

### Hypermedia Playback

Transcripts and user annotations are synced live via Loro CRDTs. Once archived, clicking any sentence in the text buffer seeks the local video asset (via FFmpeg) to the exact millisecond.

### Ephemeral Presence (The RAM Boundary)

High-frequency state (live cursors, text selections, WebGL player movements) is broadcast in-memory via WebRTC. **Strictly prohibited from writing to the Loro CRDT disk buffer.** The Relay discards ephemeral data if the peer is offline.

### A/V Transport

- **P2P**: LiveKit (WebRTC) data channels for direct device-to-device sync on LAN
- **SFU Fallback**: Nexus provides Selective Forwarding Unit fallback for massive calls

---

## The Integrated Consensus Stack

Consensus in Prism is a **Spectrum of Authority**, not a single mechanism.

| Layer | Type | Mechanism | Purpose |
|-------|------|-----------|---------|
| Mathematical | Optimistic | Loro CRDT | Ensures data mathematically merges without corruption |
| Social | Multi-Sig | Schnorr Signatures | Requires X-of-Y DIDs to sign a change (e.g., merging to "Main" branch) |
| Institutional | Notarized | Relay Timestamping | Any designated Relay provides cryptographic proof of "When" |
| Failure State | Quarantine | CodeMirror Diff UI | Halts automatic merging for human resolution during Semantic Logic conflicts |

**The Quarantine Protocol**: When the CRDT math merges perfectly but the *semantic logic* conflicts (e.g., two users rewrite the same Lua function offline), the system does not guess. It throws the affected nodes into a visual "Quarantine" state in the UI, forcing a Git-style conflict resolution interface.

---

## The Sovereign Immune System (Trust & Safety)

T&S is baked into the **physics** of the protocol, creating a self-healing decentralized network.

### Protocol-Level Defenses

- **Luau Sandbox**: Zero network access; only whitelisted API hooks exposed; never raw `fetch()` without a Capability Token
- **Schema Poison Pill**: Strict YAML schema validation drops malformed CRDT diffs before they touch the graph (stops injection attacks)
- **XSS Prevention**: Third-party Puck components rendered in Shadow DOM / sandboxed iframe with `sandbox="allow-scripts"` but no `allow-same-origin`
- **Relay Spam (Sybil Attacks)**: Hashcash / Proof-of-Work rate limiting — unauthenticated DIDs must solve a 2-second cryptographic puzzle before a Relay accepts large CRDT diffs; invisible to real users, expensive for bots

### Cryptographic Whistleblowing

Granular reporting across **all Relays**, not just Nexus. Users generate a "Whistleblower Packet" containing only the offending CRDT node and a one-time decryption key. The Relay autonomously evaluates the exposed payload against host-defined policies.

### The Gossip Protocol

Relays share verified "Toxic Hashes" (malware/CSAM signatures) discovered via whistleblowing. Creates an emergent, decentralized threat intelligence network. The network organically shuns malicious DIDs without central moderation.

### Web of Trust (Hive-Mind Bans)

Users can cryptographically "Mute" DIDs. If a Relay registers a massive threshold of mutes against a DID, it drops that traffic globally.

### VFS Hash-Checking

Before a binary is publicly projected via a Sovereign Portal, the system checks the hash against global safety databases (NCMEC/VirusTotal) without needing to "see" the file content.

### Nexus Nuclear ToS

Users own their keys, but Nexus retains the right to revoke **routing privileges** for Terms of Service violations. Banned users keep their local Vault intact and can use open-source Relays.

---

## Security & Recovery

### Device in a Lake (Key Recovery)

**Compromise solutions:**
1. **Social Guardians** (Shamir's Secret Sharing) — User designates 3 trusted DIDs. If device is lost, 2-of-3 guardians reconstruct the key
2. **Relay Encrypted Escrow** — Private key encrypted with a memorable password and stored on Relays. Relays never sees the key; user retrieves the blob via standard email/SMS verification

### Binary Forking Protocol

Non-mergeable files (`.wav`, `.png`, `.mp4`) are strictly protected from corruption. Concurrent offline edits result in deterministic forks (e.g., `take_01.variant.[DID].wav`), flagged for visual resolution in the Core UI.

---

## The Ecosystem Apps

### Flux (The Operational Hub)

A hyper-flexible productivity application and the primary entry point for the user. Manages time, finances, goals, gigs, contacts, inventory, and documents.

- Flux users create "workspaces" (Manifests) as their personal blend of all these needs
- "Databases" are really just containers for other typed Collections (Contacts, Financial Accounts, Physical Assets, Documents)
- "Projects," "Gigs," and "Goals" are high-order nodes that contain UIDs of child items (time logs, milestones, focus sessions, tasks, notes, boards)
- Each has a `view_protocol` for custom Craft.js overview dashboards that aggregate data from child nodes
- Deploys Level 1-3 Sovereign Portals (invoices, client dashboards)
- Handles payments via agnostic **Financial Adapters** (Stripe/PayPal/Interac/Lightning bridging directly to CRDT state)

### Lattice (The Game Middleware Suite)

An enterprise-grade, à la carte game middleware platform. Managed from within Flux workspaces.

**The Universal Lua Game Runtime**: Lattice embeds Lua 5.4 into the target game engine. The C++ integration is a "dumb terminal" — it loads the Bank, feeds inputs to the Lua VM, and draws the outputs to the GPU.

**The Lattice Codegen Compiler**: When building a project, generates:
- **The Bank**: A zero-copy binary (FlatBuffers — requires `.fbs` schema compilation) containing layout math, audio assets, and compiled Lua bytecode
- **The Engine API Contract**: Strongly-typed native classes (GDScript `.gd`, Unreal `.h` UCLASS) creating a safe, autocompleting bridge between gameplay programmers and the Lattice Black Box

**Modules (À La Carte):**
- **Iris** — UI Authoring: Uses the Prism Puck canvas to author Flexbox/Tailwind game UI. Compiles to Yoga layout math and native GPU draw calls with SDF text rendering
- **Loom** — Narrative Engine: Node-based editor emitting **LoomLang** script for branching dialogue
- **Canto** — Audio & DSP: Audio middleware managing soundbank generation, mixing hierarchies, and DSP parameters
- **Simulacra** — Game Entity Authoring: Schema builder for character stats, inventory attributes, and RPG behaviors
- **Cue** — Event Orchestration: Timeline editor synchronizing animations, Loom dialogue, Canto audio, and Simulacra logic

### Cadence (The Music Education Platform)

A Prism-native LMS/MOOC for interactive, multi-media music education (K-12 and beyond).

- **Courses as Workspaces**: A course is a shared Prism workspace. Students use DIDs to subscribe to an instructor's CRDT relay. Syllabus and lessons sync directly to local machines
- **Assignments as State Diffs**: Students commit changes to their branch of the course CRDT, not upload dead files to a database
- **Living Lessons**: Because Cadence uses Prism's CodeMirror/Lua core, lessons are interactive. A music theory document can contain a playable MIDI piano roll; homework can require authoring live DSP chains (via Canto), which the instructor evaluates directly inside the document
- **Institutional Bridge**: Uses Prism codegen to emit **IMS Common Cartridge (.imscc)** and **SCORM** packages for legacy LMS compatibility. Acts as an **LTI** provider for embedding inside Brightspace/Canvas
- **Killer App for Layer 0C**: Utilizes local Qwen 3.5 Omni to ingest 10-hour audio sessions and transcribe them into searchable hypermedia Session Nodes

### Grip (Live Production Management)

A Layer 2 extension on Flux, bootstrapping its ecosystem for live production (film, theatre, concerts, events).

- **Loom** abstracted as a strictly typed document-generating script for screenplays/run-of-shows
- Obsidian weak refs connect every script line to lighting cues, sound triggers, wardrobe changes, stagehand tasks
- **Builder 1**: Generates itineraries, rider forms, inventory manifests, cut sheets
- **Builder 2**: Technical Directors author stage plots with PixiJS/@xyflow; every piece of gear is a Loro object — dropping gear on stage plot updates Flux inventory instantly
- **Builder 3 (3D)**: R3F + OpenCASCADE for full 3D stage/venue visualization; CAD import (.STEP, .gltf)
- **Node-to-shader pipeline**: Wire DMX parameters visually like Blender Shader Editor → TSL compiles to real-time lighting shaders
- **NLE Timeline**: Tone.js (clock) + PixiJS (rendering) + optional OpenDAW SDK (audio DSP, optimistic flush pattern) + peaks.js (waveforms)
- **Hardware control**: Art-Net (lights), VISCA (cameras), OSC/MIDI (audio), all via Rust Daemon
- **Live**: iPad slate marking, Whisper live transcription as Actor, IoT sensor ingestion

---

## React Performance Strategy

Because the entire app ecosystem is React-based, performance requires strict architectural discipline:

1. **Atomic State (Zustand)**: Abandon React Context entirely. Subscribe to specific Loro node IDs. If `AudioNode-12` updates from Relay, only that component re-renders
2. **Transient Updates**: During active dragging, use vanilla JS `onMouseMove` + direct CSS transform via `ref.current.style.transform`. React has no idea the element is moving. Only `onMouseUp` writes final coordinates to Zustand → Loro
3. **Preact Signals** (`@preact/signals-react`): SolidJS-like reactivity inside React components; bypasses VDOM diffing for text/simple DOM updates; works alongside `@xyflow/react` and Puck
4. **`React.memo` + `useMemo` + `useCallback`**: Aggressive memoization on canvas components
5. **Why NOT Million.js**: Million's Block DOM only compiles *your* source code, not `node_modules`. It won't speed up React Flow or Puck — the heaviest parts of the IDE. Also causes "bailouts" on complex patterns and timing desyncs in DnD physics

---

## Complete Library Manifest

### Agnostic Core & IDE Glue (Shared)

| Library | Purpose |
|---------|---------|
| `loro-crdt` | Mathematical source of truth (Rust + WASM) |
| `@loro-extended` | Sync scaffolding for Loro: schemas, network adapters (SSE/WS/WebRTC), persistence, reactivity |
| `zustand` | Atomic state slices; subscribes to specific Loro nodes |
| `@preact/signals-react` | Surgical VDOM bypass for high-frequency updates |
| `codemirror` v6 | Modular, mobile-friendly code editor — **sole editor for all contexts** |
| `esbuild-wasm` | In-browser TS/JSX compiler for user-authored components |
| `kbar` | Global CMD+K dispatcher |
| `tweakpane` | Auto-generated Inspector panels from JSON schema |
| `@floating-ui/react` | Popovers/tooltips that escape `overflow: hidden` |
| `xstate` | Finite state machines for tool mode management |
| `@use-gesture/react` | Raw pointer event physics (momentum, velocity, pinch) |
| `react-spring` | Camera animation with native momentum feel |
| `react-virtuoso` | Virtual list rendering (50k+ items) |
| `wa-sqlite` | In-browser SQLite WASM — local query index derived from Loro state (not a CRDT DB) |
| `libsodium-wrappers` | X25519 key exchange, XChaCha20-Poly1305 symmetric encryption for E2EE |
| `jose` | JWT for AutoREST API access control |
| `@huggingface/transformers` | Browser-local AI (Whisper, embeddings) via WASM/WebGPU — v4 bundles ONNX Runtime internally |
| `livekit-client` | WebRTC: SFU for large calls, data channels for P2P CRDT sync on LAN |
| `peaks.js` | Audio waveform rendering (BBC R&D) — Canvas/WebGL with native Marker/Segment API |

### Infrastructure & Build

| Library | Purpose |
|---------|---------|
| `vite` | SPA bundler for all local apps |
| `tauri` v2 | Desktop shell, Rust daemon, native FS |
| `@capacitor/core` | Mobile shell, Swift/Kotlin FFI |
| `hono` | Lightweight HTTP framework + JSX SSR on Relays for Sovereign Portals |
| `notify` (Rust) | OS file watcher |
| `object_store` (Rust) | VFS adapter layer (S3/GCS/local/NAS) |
| `mlua` (Rust) | Lua 5.4 bindings with WASM support |
| `wasmoon` | Lua 5.4 in browser via WASM |
| `git2-rs` | Manifest versioning in Studio |
| `flatbuffers` | Zero-copy C++ serialization for game engine banks |
| `whisper.cpp` | GPU-accelerated local transcription as Actor |
| `ffmpeg` | Video seeking, compression, archival |

---

## Terminology Clarity

| Term | Meaning | Audience |
|------|---------|----------|
| Prism Daemon | Local Rust physics engine | Engineers |
| Prism Engine | Same thing | End users |
| Prism Core | Client-side glass + logic (Layer 1 + 2) | All |
| Prism Relay | Network routing infrastructure | All |
| Prism Studio | Universal Host + IDE app | Power users |
| Prism Nexus | Managed SaaS (Studio + Relay + App Repo) | General market |
| Actor | 0B/0C process (Whisper, Python, LLM) | Engineers |
| Vault | Encrypted local directory | All |
| Manifest | YAML workspace/app definition | All |
| Collection | Typed CRDT data array | Engineers |

---

## The Web 4.0 Assessment

Prism qualifies as the most architecturally complete vision of Web 4.0 seen to date:

- **Web 1.0 contribution**: Open protocols, hyperlinking, decentralization → Prism's open Relay protocol + Sovereign Indexer
- **Web 2.0 contribution**: High-speed UX, real-time collaboration, rich applications → Puck/React/@xyflow builders + CRDT multiplayer as a primitive
- **Web 3.0 contribution**: Cryptographic sovereignty, no central databases → DIDs, Vaults, E2EE Relays (without the blockchain bottleneck)
- **Web 4.0 original**: Every app is an IDE, intelligence is local and owned, physical hardware is controlled by the same OS that manages your documents, the "cloud" is just your relay choosing to be always-on

**The remaining gap being actively solved**: Physical world interface — the Live Production app (Grip) proves Prism can control lights, cameras, and motorized hardware through the same Loro graph that stores your invoices and game scripts.

---

## Known Caveats & Maturity Notes

1. **Loro CRDT**: High-performance and actively maintained (v1.8.x), but independent sources note it requires "substantial development work" for production collaborative apps. The `@loro-extended` framework significantly reduces this burden by providing schemas, sync adapters, and persistence out of the box, but evaluate its maturity for your specific scale requirements.

2. **E2EE Layer**: The E2EE approach (libsodium encryption of Loro export blobs) is architecturally straightforward but requires careful key management. The Blind Escrow recovery system must be rigorously tested — lost keys mean permanently inaccessible Vaults.

3. **Spatial Canvas (Builder 2)**: By choosing MIT-licensed composable primitives (`@xyflow/react` + `PixiJS` + `XState`) over a monolithic canvas SDK, you gain licensing freedom but accept more integration work for features like freeform drawing, pressure-sensitive strokes, and multi-touch gesture handling. These must be built from `@use-gesture/react` + `PixiJS` primitives.

4. **Lua 5.4 in browser**: wasmoon works but has a larger bundle than Fengari (~pure JS Lua). For ultra-lightweight browser contexts (Level 1-2 Portals), consider whether Lua execution is even needed client-side, or defer scripting to the Relay/Daemon.

5. **FlatBuffers**: Requires schema compilation (`.fbs` files → generated code). Not a drop-in replacement for JSON. The Lattice build pipeline must account for this codegen step.

6. **wa-sqlite as Query Index**: This is explicitly NOT a CRDT database. Loro is the source of truth; the Daemon asynchronously mirrors Loro state into an in-memory SQLite database for fast complex queries. If the SQLite index is corrupted or lost, it can always be rebuilt from the Loro CRDT history.
