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
  acmeCertificateModule,
  portalTemplateModule,
  webrtcSignalingModule,
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
  AcmeChallenge,
  SslCertificate,
  AcmeCertificateManager,
  PortalTemplate,
  PortalTemplateRegistry,
  SignalType,
  SignalMessage,
  SignalingPeer,
  SignalingRoom,
  SignalDelivery,
  SignalingHub,
} from "./relay-types.js";

// ── Client SDK ──────────────────────────────────────────────────────────────
export { createRelayClient } from "./relay-client.js";
export type {
  RelayClientOptions,
  SendEnvelopeOptions,
  RelayClientState,
  RelayClientEvents,
  RelayClient,
} from "./relay-client.js";

// ── Portal Renderer ────────────────────────────────────────────────────────
export {
  extractPortalSnapshot,
  escapeHtml,
  renderPortalHtml,
} from "./portal-renderer.js";
export type {
  PortalObject,
  PortalEdge,
  PortalSnapshot,
} from "./portal-renderer.js";

// ── Phase 2 modules ─────────────────────────────────────────────────────────
export { collectionHostModule } from "./collection-host-module.js";
export { hashcashModule } from "./hashcash-module.js";
export { peerTrustModule } from "./peer-trust-module.js";
export { escrowModule } from "./escrow-module.js";
export { federationModule } from "./federation-module.js";
