/**
 * Peer Trust module — reputation tracking and auto-banning.
 *
 * Wraps createPeerTrustGraph from the trust layer, exposing it
 * as a relay capability for route handlers and transport to query.
 */

import type { RelayModule, RelayContext } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import { createPeerTrustGraph } from "../trust/trust.js";
import type { PeerTrustGraph } from "../trust/trust-types.js";

export function peerTrustModule(): RelayModule {
  let graph: PeerTrustGraph | undefined;

  return {
    name: "peer-trust",
    description: "Peer reputation tracking with trust levels and banning",
    dependencies: [],

    install(ctx: RelayContext): void {
      graph = createPeerTrustGraph();
      ctx.setCapability(RELAY_CAPABILITIES.TRUST, graph);
    },

    async stop(): Promise<void> {
      graph?.dispose();
      graph = undefined;
    },
  };
}
