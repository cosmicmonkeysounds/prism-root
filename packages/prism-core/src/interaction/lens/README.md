# lens

The Lens system — Prism's universal extension unit. A Lens is a typed manifest (identity, icon, category, view slots, commands, keybindings) plus a component supplied by the host. `LensRegistry` stores manifests; `ShellStore` (Zustand) manages tabs and panel layout; `LensBundle` is the self-installing pairing of a manifest with its component. Generic over component type so this module stays React-free — Studio specializes it with React's `ComponentType`.

## Import

```ts
import {
  createLensRegistry,
  createShellStore,
  defineLensBundle,
  installLensBundles,
  lensId,
  tabId,
} from "@prism/core/lens";
```

## Key exports

- `createLensRegistry()` — `register`/`unregister`/`get`/`has`/`allLenses`/`getByCategory`/`subscribe`.
- `createShellStore()` — Zustand store with `tabs`/`activeTabId`/`panelLayout` and actions `openTab`/`closeTab`/`pinTab`/`unpinTab`/`reorderTab`/`setActiveTab`/`toggleSidebar`/`toggleInspector`/`setSidebarWidth`/`setInspectorWidth`.
- `defineLensBundle<TComponent>(manifest, component)` — builds a `LensBundle` that registers manifest + component together.
- `installLensBundles(bundles, ctx)` — installs many bundles; returns a single uninstall that tears down in reverse order.
- `lensId(s)` / `tabId(s)` — branded id constructors.
- Types: `LensId`, `TabId`, `LensCategory`, `LensManifest`, `LensView`, `LensCommand`, `LensKeybinding`, `LensBundle`, `LensInstallContext`, `TabEntry`, `PanelLayout`, `ShellState`, `ShellStore`.

## Usage

```ts
import {
  createLensRegistry,
  defineLensBundle,
  installLensBundles,
  lensId,
} from "@prism/core/lens";

const lensRegistry = createLensRegistry();
const componentMap = new Map<string, () => unknown>();

const bundle = defineLensBundle(
  {
    id: lensId("editor"),
    name: "Editor",
    icon: "pencil",
    category: "editor",
    contributes: { views: [{ slot: "main" }], commands: [] },
  },
  () => "<EditorPanel />",
);

const uninstall = installLensBundles([bundle], { lensRegistry, componentMap });
```
