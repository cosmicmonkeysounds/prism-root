/**
 * @prism/admin-kit — Puck-native admin panel components for Prism runtimes.
 *
 * Consumers:
 *   1. Create an `AdminDataSource` for the runtime they want to inspect
 *      (kernel-backed, relay-backed, or custom).
 *   2. Wrap a Puck tree in `<AdminProvider source={…}>`.
 *   3. Pass `createAdminPuckConfig()` as the Puck `config`, and optionally
 *      seed with `createDefaultAdminLayout()`.
 */

export type {
  AdminDataSource,
  AdminSnapshot,
  ActivityItem,
  HealthLevel,
  HealthStatus,
  Metric,
  Service,
} from "./types.js";
export { emptySnapshot } from "./types.js";

export {
  AdminProvider,
  useAdminContext,
  useAdminSnapshot,
} from "./admin-context.js";
export type { AdminProviderProps } from "./admin-context.js";

export {
  createKernelDataSource,
  createRelayDataSource,
  createDaemonDataSource,
  parsePrometheus,
  findSample,
} from "./data-sources/index.js";
export type {
  KernelAdminTarget,
  KernelDataSourceOptions,
  RelayDataSourceOptions,
  DaemonDataSourceOptions,
  PromSample,
} from "./data-sources/index.js";

export * from "./widgets/index.js";

export { createAdminPuckConfig } from "./puck-config.js";
export { createDefaultAdminLayout } from "./default-layout.js";

export {
  formatUptime,
  formatBytes,
  formatMetricValue,
  formatRelativeTime,
  HEALTH_COLORS,
  rollupHealth,
} from "./admin-helpers.js";
