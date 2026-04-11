/**
 * Escrow module — blind key recovery via encrypted deposits.
 *
 * Wraps createEscrowManager from the trust layer. Users deposit
 * encrypted key material; the relay never sees the raw key.
 */

import type { RelayModule, RelayContext } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import { createEscrowManager } from "@prism/core/trust";

export function escrowModule(): RelayModule {
  return {
    name: "escrow",
    description: "Blind key recovery via encrypted escrow deposits",
    dependencies: [],

    install(ctx: RelayContext): void {
      const manager = createEscrowManager();
      ctx.setCapability(RELAY_CAPABILITIES.ESCROW, manager);
    },
  };
}
