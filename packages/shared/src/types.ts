/**
 * Core Prism types shared across all packages.
 */

/** Unique identifier for a CRDT node in the object graph. */
export type NodeId = string;

/** Unique identifier for a Vault. */
export type VaultId = string;

/** Unique identifier for a Collection within a Vault. */
export type CollectionId = string;

/** A Prism URI following the format: prism://[Vault]/[Collection]/[ID] */
export type PrismUri = `prism://${string}/${string}/${string}`;

/** Serialized CRDT state as a binary blob. */
export type CrdtSnapshot = Uint8Array;

/** Serialized CRDT incremental update. */
export type CrdtUpdate = Uint8Array;

/** Result of a Lua script execution. */
export type LuaResult = {
  success: boolean;
  value: unknown;
  error?: string;
};

/** Manifest workspace definition (YAML-backed). */
export type Manifest = {
  id: string;
  name: string;
  collections: CollectionId[];
  version: string;
};
