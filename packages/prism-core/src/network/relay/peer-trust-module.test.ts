import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import { createRelayBuilder } from "./relay.js";
import { peerTrustModule } from "./peer-trust-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { PeerTrustGraph } from "@prism/core/trust";
import type { RelayInstance } from "./relay-types.js";

let relay: RelayInstance;

beforeAll(async () => {
  const id = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: id.did })
    .use(peerTrustModule())
    .build();
  await relay.start();
});

describe("peerTrustModule", () => {
  it("registers the trust capability", () => {
    const graph = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    expect(graph).toBeDefined();
  });

  it("tracks peer reputation", () => {
    const graph = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST) as PeerTrustGraph;
    graph.recordPositive("peer-a");
    graph.recordPositive("peer-a");
    const rep = graph.getPeer("peer-a");
    expect(rep).toBeDefined();
    expect(rep?.positiveInteractions).toBe(2);
  });

  it("bans and unbans peers", () => {
    const graph = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST) as PeerTrustGraph;
    graph.ban("peer-bad", "spamming");
    expect(graph.isBanned("peer-bad")).toBe(true);
    graph.unban("peer-bad");
    expect(graph.isBanned("peer-bad")).toBe(false);
  });

  it("flags content", () => {
    const graph = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST) as PeerTrustGraph;
    graph.flagContent("hash-abc", "malware", "reporter-1");
    expect(graph.isContentFlagged("hash-abc")).toBe(true);
    expect(graph.isContentFlagged("hash-xyz")).toBe(false);
  });
});
