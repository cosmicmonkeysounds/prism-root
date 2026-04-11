export {
  createMemoryRosterStore,
  createVaultRoster,
} from "./vault-roster.js";

export type {
  RosterEntry,
  RosterSortField,
  RosterSortDir,
  RosterListOptions,
  RosterChangeType,
  RosterChange,
  RosterChangeHandler,
  RosterStore,
  VaultRoster,
} from "./vault-roster.js";

export {
  createMemoryDiscoveryAdapter,
  createVaultDiscovery,
} from "./vault-discovery.js";

export type {
  DiscoveryAdapter,
  MemoryDiscoveryAdapter,
  DiscoveredVault,
  DiscoveryEventType,
  DiscoveryEvent,
  DiscoveryEventHandler,
  DiscoveryScanOptions,
  VaultDiscovery,
} from "./vault-discovery.js";
