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
} from "./shell-store.js";
export { createShellStore } from "./shell-store.js";

export type {
  ViewportState,
  ViewportCacheState,
  ViewportCacheActions,
  ViewportCache,
} from "./viewport-cache.js";
export { createViewportCache } from "./viewport-cache.js";

export type {
  LensBundle,
  LensInstallContext,
  ShellWidgetBundle,
  ShellWidgetInstallContext,
} from "./lens-install.js";
export {
  installLensBundles,
  defineLensBundle,
  withShellModes,
  filterLensBundlesByShellMode,
  installShellWidgetBundles,
  defineShellWidgetBundle,
} from "./lens-install.js";

export type {
  ShellMode,
  Permission,
  ShellModeConstraints,
  BootConfig,
  ResolvedBootConfig,
} from "./shell-mode.js";
export {
  SHELL_MODES,
  PERMISSIONS,
  PERMISSION_RANK,
  DEFAULT_AVAILABLE_IN_MODES,
  DEFAULT_MIN_PERMISSION,
  DEFAULT_BOOT_CONFIG,
  permissionAtLeast,
  resolveBootConfig,
  isShellMode,
  isPermission,
  lensBundleMatchesShellContext,
} from "./shell-mode.js";
