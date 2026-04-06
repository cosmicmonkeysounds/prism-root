export {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  createMemoryPingTransport,
} from "./relay.js";

export type { WebhookHttpClient } from "./relay.js";

export {
  RELAY_CAPABILITIES,
} from "./relay-types.js";

export type {
  RelayEnvelope,
  BlindMailbox,
  RelayRouter,
  RouteResult,
  RelayTimestamper,
  TimestampReceipt,
  BlindPinger,
  BlindPing,
  PingTransport,
  CapabilityToken,
  CapabilityTokenManager,
  WebhookConfig,
  WebhookPayload,
  WebhookDelivery,
  WebhookEmitter,
  PortalLevel,
  PortalManifest,
  PortalRegistry,
  RelayModule,
  RelayContext,
  RelayConfig,
  RelayInstance,
  RelayBuilder,
  RelayBuilderOptions,
  CollectionHost,
  HashcashGate,
  HashcashModuleOptions,
  FederationPeer,
  ForwardResult,
  ForwardTransport,
  FederationRegistry,
} from "./relay-types.js";

// ── Phase 2 modules ─────────────────────────────────────────────────────────
export { collectionHostModule } from "./collection-host-module.js";
export { hashcashModule } from "./hashcash-module.js";
export { peerTrustModule } from "./peer-trust-module.js";
export { escrowModule } from "./escrow-module.js";
export { federationModule } from "./federation-module.js";
