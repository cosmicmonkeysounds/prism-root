# @prism/studio

The Universal Host — every Prism app is a Studio instance. Vite SPA + Tauri 2.0 desktop shell.

## Quick Start

```bash
pnpm dev              # Vite dev server on http://localhost:1420
pnpm tauri dev        # Tauri desktop with hot reload
pnpm build            # Production build
pnpm typecheck        # TypeScript strict
pnpm test:e2e         # Playwright E2E tests (requires dev server on :1420)
```

## Architecture

Studio is a **local-first Vite SPA** (not Next.js). It wraps in Tauri for desktop. All daemon communication goes through `src/ipc-bridge.ts` using Tauri `invoke()` — never raw HTTP.

### Kernel (`src/kernel/`)

The kernel wires all Layer 1 systems together at app startup:

- **`studio-kernel.ts`** — singleton that creates and connects ObjectRegistry, CollectionStore, PrismBus, AtomStore, UndoRedoManager, NotificationStore, SearchEngine, ActivityStore, RelayManager, AutomationEngine, PluginRegistry, InputRouter, Identity, VfsManager, Trust & Safety, Facet System, and more.
- **`relay-manager.ts`** — manages WebSocket connections to Prism Relay servers. Studio connects as a client only (no server code).
- **`entities.ts`** — page-builder entity types: folder, page, section, heading, text-block, image, button, card.
- **`kernel-context.tsx`** — React context providing `useKernel`, `useSelection`, `useObjects`, `useUndo`, `useRelay`, `useIdentity`, `useVfs`, etc.

### Components (`src/components/`)

- **`studio-shell.tsx`** — custom shell layout with sidebar/inspector
- **`object-explorer.tsx`** — tree view with drag-drop reorder/reparent, search, templates, activity feed
- **`component-palette.tsx`** — available block types from registry, click-to-add, draggable
- **`inspector-panel.tsx`** — schema-driven property editor from EntityDef fields
- **`notification-toast.tsx`** — floating toasts from NotificationStore
- **`presence-indicator.tsx`** — colored avatar dots for connected peers

### Panels (`src/panels/`)

22 lens panels covering the full IDE surface:

| Panel | Key | Purpose |
|-------|-----|---------|
| Canvas | `v` | WYSIWYG page preview with block toolbar + quick-create |
| Editor | `e` | CodeMirror 6 editing selected object content |
| Graph | `g` | Live-reactive @xyflow/react node graph |
| Layout | `l` | Puck visual builder wired to kernel |
| CRDT | `c` | State inspector (objects/edges/JSON) |
| Relay | `r` | Relay connection manager + portal publishing |
| Settings | `,` | Category-grouped settings from ConfigRegistry |
| Automation | `a` | Create/edit/toggle automation rules |
| Analysis | `n` | Critical path, cycle detection, impact analysis |
| Plugins | `p` | Plugin registry browser |
| Shortcuts | `k` | Keyboard binding manager |
| Vaults | `w` | Vault roster browser |
| Identity | `i` | DID generation, sign/verify, import/export |
| Assets | `f` | VFS blob browser with import/lock/unlock |
| Trust | `t` | Peers, validation, content flags, escrow |
| Form | `d` | Schema-driven form renderer |
| Table | `b` | Data grid with sort/filter/inline edit |
| Sequencer | `q` | Visual automation/script builder |
| Report | `o` | Grouped/summarized data with aggregates |
| Lua Facet | `u` | Lua `ui.*` call parser → React renderer |
| Facet Designer | `x` | Visual FacetDefinition layout builder |
| Record Browser | `z` | Unified data browser (form/list/table/card modes) |

### Data Flow

```
User action → kernel.createObject/updateObject/deleteObject
  → CollectionStore.putObject (Loro CRDT)
  → PrismBus.emit (event)
  → ObjectAtomStore (Zustand cache)
  → React re-render
  → UndoRedoManager.push (snapshot)
  → SearchEngine auto-reindex
```

## Key Design Decisions

- Kernel is a **singleton** created at app startup, not per-component.
- All mutations go through **kernel CRUD methods** — never raw CollectionStore.
- Studio has **no server code**. Relay servers are managed via CLI; Studio connects as a client.
- Canvas resolves the selected page (walking up parentId) and renders live.
- Editor creates per-object LoroText buffers, debounce-syncs back to kernel.
