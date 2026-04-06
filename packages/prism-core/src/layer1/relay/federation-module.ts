/**
 * Federation module — relay-to-relay mesh networking.
 *
 * Maintains a registry of known relay peers. Envelope forwarding
 * is handled by a pluggable ForwardTransport set by the runtime.
 */

import type { DID } from "../identity/identity-types.js";
import type {
  RelayModule,
  RelayContext,
  RelayEnvelope,
  FederationPeer,
  FederationRegistry,
  ForwardResult,
  ForwardTransport,
} from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";

export function federationModule(): RelayModule {
  return {
    name: "federation",
    description: "Relay-to-relay mesh networking and envelope forwarding",
    dependencies: [],

    install(ctx: RelayContext): void {
      const peers = new Map<DID, FederationPeer>();
      let transport: ForwardTransport | undefined;

      const registry: FederationRegistry = {
        announce(relayDid: DID, url: string): void {
          const now = new Date().toISOString();
          const existing = peers.get(relayDid);
          if (existing) {
            existing.url = url;
            existing.lastSeenAt = now;
          } else {
            peers.set(relayDid, {
              relayDid,
              url,
              announcedAt: now,
              lastSeenAt: now,
            });
          }
        },

        getPeers(): FederationPeer[] {
          return [...peers.values()];
        },

        getPeer(relayDid: DID): FederationPeer | undefined {
          return peers.get(relayDid);
        },

        removePeer(relayDid: DID): boolean {
          return peers.delete(relayDid);
        },

        async forwardEnvelope(
          envelope: RelayEnvelope,
          targetRelay: DID,
        ): Promise<ForwardResult> {
          if (!transport) {
            return { status: "no-transport" };
          }
          const peer = peers.get(targetRelay);
          if (!peer) {
            return { status: "unknown-relay", targetRelay };
          }
          return transport(envelope, peer.url);
        },

        setTransport(t: ForwardTransport): void {
          transport = t;
        },
      };

      ctx.setCapability(RELAY_CAPABILITIES.FEDERATION, registry);
    },
  };
}
