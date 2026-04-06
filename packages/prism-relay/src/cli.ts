/**
 * Prism Relay CLI — start a relay server from the command line.
 *
 * Usage:
 *   npx tsx packages/prism-relay/src/cli.ts [--port 4444] [--host 0.0.0.0]
 */

import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
} from "@prism/core/relay";
import { createRelayServer } from "./server/relay-server.js";

function parseArgs(argv: string[]): { port: number; host: string } {
  let port = 4444;
  let host = "0.0.0.0";
  for (let i = 0; i < argv.length; i++) {
    const next = argv[i + 1];
    if (argv[i] === "--port" && next) {
      port = parseInt(next, 10);
      i++;
    } else if (argv[i] === "--host" && next) {
      host = next;
      i++;
    }
  }
  return { port, host };
}

async function main(): Promise<void> {
  const { port, host } = parseArgs(process.argv.slice(2));

  const identity = await createIdentity({ method: "key" });

  const relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(relayTimestampModule(identity))
    .use(blindPingModule())
    .use(capabilityTokenModule(identity))
    .use(webhookModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: 16 }))
    .use(peerTrustModule())
    .use(escrowModule())
    .use(federationModule())
    .build();

  await relay.start();

  const server = createRelayServer({ relay, port, host });
  const info = await server.start();

  const out = (s: string) => process.stdout.write(s + "\n");
  const err = (s: string) => process.stderr.write(s + "\n");

  out("");
  out("  Prism Relay started");
  out(`  DID:     ${relay.did}`);
  out(`  Listen:  http://${host}:${info.port}`);
  out(`  WS:      ws://${host}:${info.port}/ws/relay`);
  out(`  Modules: ${relay.modules.join(", ")}`);
  out("");

  function shutdown(): void {
    out("\nShutting down...");
    info.close()
      .then(() => relay.stop())
      .then(() => {
        out("Relay stopped.");
        process.exit(0);
      })
      .catch((e) => {
        err(`Shutdown error: ${String(e)}`);
        process.exit(1);
      });
  }

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

main().catch((e: unknown) => {
  process.stderr.write(`Failed to start relay: ${String(e)}\n`);
  process.exit(1);
});
