import { describe, it, expect, beforeAll } from "vitest";
import type { DID } from "../identity/identity-types.js";
import { createIdentity } from "../identity/identity.js";
import { createRelayBuilder } from "./relay.js";
import { federationModule } from "./federation-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { FederationRegistry, RelayInstance, ForwardResult } from "./relay-types.js";

const PEER_A = "did:key:zPeerA" as DID;
const PEER_B = "did:key:zPeerB" as DID;

let relay: RelayInstance;

beforeAll(async () => {
  const id = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: id.did })
    .use(federationModule())
    .build();
  await relay.start();
});

describe("federationModule", () => {
  it("registers the federation capability", () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    expect(reg).toBeDefined();
  });

  it("announces and lists peers", () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    reg.announce(PEER_A, "http://peer-a:4444");
    reg.announce(PEER_B, "http://peer-b:4444");

    const peers = reg.getPeers();
    expect(peers).toHaveLength(2);
    expect(peers.some((p) => p.relayDid === PEER_A)).toBe(true);
  });

  it("gets a specific peer", () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    const peer = reg.getPeer(PEER_A);
    expect(peer).toBeDefined();
    expect(peer?.url).toBe("http://peer-a:4444");
  });

  it("updates URL on re-announce", () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    reg.announce(PEER_A, "http://peer-a:5555");
    expect(reg.getPeer(PEER_A)?.url).toBe("http://peer-a:5555");
  });

  it("removes a peer", () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    expect(reg.removePeer(PEER_B)).toBe(true);
    expect(reg.getPeer(PEER_B)).toBeUndefined();
    expect(reg.removePeer(PEER_B)).toBe(false);
  });

  it("returns no-transport when no transport set", async () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    const result = await reg.forwardEnvelope(
      {
        id: "env-1",
        from: "did:key:zSender" as DID,
        to: "did:key:zRecipient" as DID,
        ciphertext: new Uint8Array([1, 2, 3]),
        submittedAt: new Date().toISOString(),
        ttlMs: 60_000,
      },
      PEER_A,
    );
    expect(result.status).toBe("no-transport");
  });

  it("forwards via transport when set", async () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    reg.setTransport(async (_env, _url): Promise<ForwardResult> => {
      return { status: "forwarded", targetRelay: PEER_A };
    });

    const result = await reg.forwardEnvelope(
      {
        id: "env-2",
        from: "did:key:zSender" as DID,
        to: "did:key:zRecipient" as DID,
        ciphertext: new Uint8Array([4, 5, 6]),
        submittedAt: new Date().toISOString(),
        ttlMs: 60_000,
      },
      PEER_A,
    );
    expect(result.status).toBe("forwarded");
  });

  it("returns unknown-relay for unregistered peer", async () => {
    const reg = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
    const unknownDid = "did:key:zUnknown" as DID;
    const result = await reg.forwardEnvelope(
      {
        id: "env-3",
        from: "did:key:zSender" as DID,
        to: "did:key:zRecipient" as DID,
        ciphertext: new Uint8Array([7]),
        submittedAt: new Date().toISOString(),
        ttlMs: 60_000,
      },
      unknownDid,
    );
    expect(result.status).toBe("unknown-relay");
  });
});
