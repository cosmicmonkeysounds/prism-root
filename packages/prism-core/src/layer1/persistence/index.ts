export { createCollectionStore } from "./collection-store.js";

export type {
  CollectionStore,
  CollectionStoreOptions,
  CollectionChangeType,
  CollectionChange,
  CollectionChangeHandler,
  ObjectFilter,
} from "./collection-store.js";

export { createMemoryAdapter, createVaultManager } from "./vault-persistence.js";

export type {
  PersistenceAdapter,
  VaultManager,
  VaultManagerOptions,
} from "./vault-persistence.js";
