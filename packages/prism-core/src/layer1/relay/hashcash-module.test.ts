import { describe, it, expect, beforeAll } from "vitest";
import type { DID } from "../identity/identity-types.js";
import { createIdentity } from "../identity/identity.js";
import { createHashcashMinter } from "../trust/trust.js";
import { createRelayBuilder } from "./relay.js";
import { hashcashModule } from "./hashcash-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { HashcashGate, RelayInstance } from "./relay-types.js";

const ALICE = "did:key:zAlice" as DID;

let relay: RelayInstance;

beforeAll(async () => {
  const id = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: id.did })
    .use(hashcashModule({ bits: 8 })) // low bits for fast tests
    .build();
  await relay.start();
});

describe("hashcashModule", () => {
  it("registers the hashcash capability", () => {
    const gate = relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH);
    expect(gate).toBeDefined();
  });

  it("creates a challenge", () => {
    const gate = relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH) as HashcashGate;
    const challenge = gate.createChallenge("test-resource");
    expect(challenge.resource).toBe("test-resource");
    expect(challenge.bits).toBe(8);
    expect(typeof challenge.salt).toBe("string");
  });

  it("verifies a valid proof", async () => {
    const gate = relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH) as HashcashGate;
    const minter = createHashcashMinter();
    const challenge = gate.createChallenge("test-verify");
    const proof = await minter.mint(challenge);
    const valid = await gate.verifyProof(proof);
    expect(valid).toBe(true);
  });

  it("tracks verified DIDs", () => {
    const gate = relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH) as HashcashGate;
    expect(gate.isVerified(ALICE)).toBe(false);
    gate.markVerified(ALICE);
    expect(gate.isVerified(ALICE)).toBe(true);
  });
});
