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

## Kernel (`src/kernel/`)
The Studio kernel wires all Layer 1 systems together:
- `studio-kernel.ts` — creates and connects ObjectRegistry, CollectionStore, PrismBus, AtomStore, ObjectAtomStore, UndoRedoManager, NotificationStore, SearchEngine, ActivityStore, ActivityTracker. Also wires clipboard (copy/cut/paste), batch ops, templates, and LiveView.
- `entities.ts` — page-builder entity types (folder, page, section, heading, text-block, image, button, card) with category containment rules and edge types (references, links-to)
- `kernel-context.tsx` — React context + hooks: useKernel, useSelection, useObjects, useObject, useUndo, useNotifications

## Components (`src/components/`)
- `studio-shell.tsx` — custom shell layout replacing core ShellLayout with real sidebar/inspector content
- `object-explorer.tsx` — tree view of objects from CollectionStore, click to select, "New Page" button, search via SearchEngine. Includes TemplateGallery overlay, ActivityFeed sidebar, and reorder buttons (↑/↓) on selected nodes.
- `inspector-panel.tsx` — schema-driven property editor reading EntityDef fields from ObjectRegistry. Includes clipboard buttons (Copy/Cut/Paste) and Add Child menu.
- `notification-toast.tsx` — floating toast overlay subscribing to NotificationStore
- `undo-status-bar.tsx` — undo/redo buttons in the header bar

## Panels (`src/panels/`)
- `canvas-panel.tsx` — WYSIWYG page preview: renders page→section→component tree as visual React components, click-to-select blocks
- `editor-panel.tsx` — CodeMirror 6 editing selected text-block/heading content (or scratch buffer when nothing editable selected)
- `graph-panel.tsx` — @xyflow/react node graph of kernel objects and edges
- `layout-panel.tsx` — Puck visual builder (Loro CRDT bridge)
- `crdt-panel.tsx` — CRDT state inspector (objects/edges/JSON tabs)

## Lenses
5 lenses: Editor (e), Graph (g), Layout (l), Canvas (v), CRDT (c)

## Data Flow
```
User action → kernel.createObject/updateObject/deleteObject
  → CollectionStore.putObject (Loro CRDT)
  → PrismBus.emit (ObjectCreated/Updated/Deleted)
  → connectBusToObjectAtoms → ObjectAtomStore (Zustand cache)
  → React components re-render via useSyncExternalStore
  → UndoRedoManager.push (snapshot for undo)
  → SearchEngine auto-reindexes via CollectionStore.onChange
```

## Key Design Decisions
- Kernel is a singleton created at app startup, not per-component
- All mutations go through kernel CRUD methods (never raw CollectionStore)
- Bus payloads follow connectBusToObjectAtoms format: `{ object }`, `{ edge }`, `{ id }`
- StudioShell replaces core ShellLayout to inject real sidebar/inspector content
- Panels access data via kernel context (useKernel), not prop drilling
- Editor creates per-object LoroText buffers keyed as `obj_content_{id}`, debounce-syncs back to kernel
- Canvas resolves the selected page (walking up parentId if a child is selected) and renders live
