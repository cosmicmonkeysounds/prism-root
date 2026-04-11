import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import { createRelayBuilder } from "./relay.js";
import { escrowModule } from "./escrow-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { EscrowManager } from "@prism/core/trust";
import type { RelayInstance } from "./relay-types.js";

let relay: RelayInstance;

beforeAll(async () => {
  const id = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: id.did })
    .use(escrowModule())
    .build();
  await relay.start();
});

describe("escrowModule", () => {
  it("registers the escrow capability", () => {
    const mgr = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
    expect(mgr).toBeDefined();
  });

  it("deposit and claim lifecycle", () => {
    const mgr = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW) as EscrowManager;
    const deposit = mgr.deposit("user-1", "encrypted-key-material-abc");
    expect(deposit.depositorId).toBe("user-1");
    expect(deposit.encryptedPayload).toBe("encrypted-key-material-abc");
    expect(deposit.claimed).toBe(false);

    const claimed = mgr.claim(deposit.id);
    expect(claimed).toBeDefined();
    expect(claimed?.claimed).toBe(true);

    // Second claim returns null (already claimed)
    const again = mgr.claim(deposit.id);
    expect(again).toBeNull();
  });

  it("lists deposits for a depositor", () => {
    const mgr = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW) as EscrowManager;
    mgr.deposit("user-2", "payload-a");
    mgr.deposit("user-2", "payload-b");
    const deposits = mgr.listDeposits("user-2");
    expect(deposits.length).toBeGreaterThanOrEqual(2);
  });
});
