/**
 * @prism/core/relay-manager — connection manager for Prism Relays.
 *
 * Apps are client-only. This module handles relay CRUD, WebSocket
 * connect/disconnect via the `@prism/core/relay` RelayClient SDK, portal
 * publish/unpublish/list via HTTP, collection sync, and status polling.
 * HTTP/WS clients are injectable for testing.
 */

export type {
  RelayConnectionStatus,
  RelayEntry,
  RelayStatus,
  PublishPortalOptions,
  DeployedPortal,
  RelayManager,
  RelayHttpClient,
  RelayManagerOptions,
} from "./relay-manager.js";
export { createRelayManager } from "./relay-manager.js";
