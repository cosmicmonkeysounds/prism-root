/**
 * Vault Host module — hosts complete vaults (manifest + collection snapshots)
 * on a Relay for persistent storage and visitor access.
 *
 * Unlike collection-host (which manages individual CRDT stores for real-time
 * sync), vault-host manages whole vaults as a unit: the manifest plus all
 * referenced collection snapshots. The relay stores opaque binary blobs —
 * whether encrypted or plaintext is decided client-side.
 *
 * This is an opt-in module: `.use(vaultHostModule())` in the builder.
 */

import type { RelayModule, RelayContext, VaultHost, HostedVault } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { PrismManifest } from "../manifest/manifest-types.js";
import type { DID } from "../identity/identity-types.js";

export function vaultHostModule(): RelayModule {
  return {
    name: "vault-host",
    description: "Hosts complete vaults for persistent storage and visitor access",
    dependencies: [],

    install(ctx: RelayContext): void {
      const vaults = new Map<string, HostedVault>();
      const snapshots = new Map<string, Map<string, Uint8Array>>();

      function computeTotalBytes(collections: Record<string, Uint8Array>): number {
        let total = 0;
        for (const data of Object.values(collections)) {
          total += data.byteLength;
        }
        return total;
      }

      const host: VaultHost = {
        publish(params: {
          manifest: PrismManifest;
          ownerDid: DID;
          isPublic?: boolean;
          collections: Record<string, Uint8Array>;
        }): HostedVault {
          const now = new Date().toISOString();
          const vaultId = params.manifest.id;

          const hosted: HostedVault = {
            id: vaultId,
            manifest: params.manifest,
            ownerDid: params.ownerDid,
            isPublic: params.isPublic ?? true,
            hostedAt: now,
            updatedAt: now,
            totalBytes: computeTotalBytes(params.collections),
          };

          vaults.set(vaultId, hosted);

          const collMap = new Map<string, Uint8Array>();
          for (const [id, data] of Object.entries(params.collections)) {
            collMap.set(id, data);
          }
          snapshots.set(vaultId, collMap);

          return hosted;
        },

        get(vaultId: string): HostedVault | undefined {
          return vaults.get(vaultId);
        },

        list(opts?: { publicOnly?: boolean }): HostedVault[] {
          const all = [...vaults.values()];
          if (opts?.publicOnly) return all.filter((v) => v.isPublic);
          return all;
        },

        getSnapshot(vaultId: string, collectionId: string): Uint8Array | undefined {
          return snapshots.get(vaultId)?.get(collectionId);
        },

        getAllSnapshots(vaultId: string): Record<string, Uint8Array> | undefined {
          const collMap = snapshots.get(vaultId);
          if (!collMap) return undefined;
          const result: Record<string, Uint8Array> = {};
          for (const [id, data] of collMap) {
            result[id] = data;
          }
          return result;
        },

        updateCollections(
          vaultId: string,
          ownerDid: DID,
          updates: Record<string, Uint8Array>,
        ): boolean {
          const vault = vaults.get(vaultId);
          if (!vault || vault.ownerDid !== ownerDid) return false;

          const collMap = snapshots.get(vaultId);
          if (!collMap) return false;

          for (const [id, data] of Object.entries(updates)) {
            collMap.set(id, data);
          }

          vault.updatedAt = new Date().toISOString();
          vault.totalBytes = computeTotalBytes(
            Object.fromEntries(collMap.entries()),
          );

          return true;
        },

        remove(vaultId: string, ownerDid: DID): boolean {
          const vault = vaults.get(vaultId);
          if (!vault || vault.ownerDid !== ownerDid) return false;
          vaults.delete(vaultId);
          snapshots.delete(vaultId);
          return true;
        },

        search(query: string): HostedVault[] {
          const lower = query.toLowerCase();
          return [...vaults.values()].filter((v) => {
            const name = v.manifest.name?.toLowerCase() ?? "";
            const desc = v.manifest.description?.toLowerCase() ?? "";
            return name.includes(lower) || desc.includes(lower);
          });
        },
      };

      ctx.setCapability(RELAY_CAPABILITIES.VAULT_HOST, host);
    },
  };
}
