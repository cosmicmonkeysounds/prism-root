/**
 * Tauri IPC command types for frontend <-> daemon communication.
 * Every Tauri invoke() call uses these typed payloads.
 */

import type { CrdtSnapshot, CrdtUpdate, LuauResult, NodeId } from "./types.js";

/** Write a value to a CRDT map node. */
export type CrdtWriteRequest = {
  docId: string;
  key: string;
  value: string;
};

/** Response after a CRDT write operation. */
export type CrdtWriteResponse = {
  success: boolean;
  update: CrdtUpdate;
};

/** Read a value from a CRDT map node. */
export type CrdtReadRequest = {
  docId: string;
  key: string;
};

/** Response from a CRDT read operation. */
export type CrdtReadResponse = {
  value: string | null;
};

/** Export the full CRDT document state. */
export type CrdtExportRequest = {
  docId: string;
};

/** Import a CRDT snapshot into the daemon. */
export type CrdtImportRequest = {
  docId: string;
  snapshot: CrdtSnapshot;
};

/** Execute a Luau script on the daemon. */
export type LuauExecRequest = {
  script: string;
  args?: Record<string, unknown>;
};

/** Subscribe to changes on a specific CRDT node. */
export type CrdtSubscribeRequest = {
  docId: string;
  nodeId: NodeId;
};

/** CRDT change event emitted from daemon to frontend. */
export type CrdtChangeEvent = {
  docId: string;
  key: string;
  value: string;
  update: CrdtUpdate;
};

/**
 * Map of all Tauri IPC commands to their request/response types.
 */
export type IpcCommands = {
  crdt_write: { request: CrdtWriteRequest; response: CrdtWriteResponse };
  crdt_read: { request: CrdtReadRequest; response: CrdtReadResponse };
  crdt_export: { request: CrdtExportRequest; response: CrdtSnapshot };
  crdt_import: { request: CrdtImportRequest; response: { success: boolean } };
  luau_exec: { request: LuauExecRequest; response: LuauResult };
};
