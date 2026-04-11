/**
 * Hashcash module — spam protection via proof-of-work challenges.
 *
 * Wraps createHashcashVerifier from the trust layer. Unknown DIDs must
 * solve a challenge before the relay accepts their envelopes.
 */

import type { DID } from "@prism/core/identity";
import type { RelayModule, RelayContext, HashcashGate, HashcashModuleOptions } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import { createHashcashVerifier } from "@prism/core/trust";

export function hashcashModule(options?: HashcashModuleOptions): RelayModule {
  const bits = options?.bits ?? 16;

  return {
    name: "hashcash",
    description: "Proof-of-work spam protection for relay traffic",
    dependencies: [],

    install(ctx: RelayContext): void {
      const verifier = createHashcashVerifier();
      const verified = new Set<DID>();

      const gate: HashcashGate = {
        createChallenge(resource: string) {
          return verifier.createChallenge(resource, bits);
        },

        async verifyProof(proof) {
          return verifier.verify(proof);
        },

        isVerified(did: DID): boolean {
          return verified.has(did);
        },

        markVerified(did: DID): void {
          verified.add(did);
        },
      };

      ctx.setCapability(RELAY_CAPABILITIES.HASHCASH, gate);
    },
  };
}
