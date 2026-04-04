export type {
  LensId,
  TabId,
  LensCategory,
  LensCommand,
  LensKeybinding,
  LensView,
  LensManifest,
} from "./lens-types.js";
export { lensId, tabId } from "./lens-types.js";

export type {
  LensRegistry,
  LensRegistryEvent,
  LensRegistryEventListener,
} from "./lens-registry.js";
export { createLensRegistry } from "./lens-registry.js";

export type {
  TabEntry,
  PanelLayout,
  ShellState,
  ShellActions,
  ShellStore,
} from "./workspace-store.js";
export { createShellStore } from "./workspace-store.js";
