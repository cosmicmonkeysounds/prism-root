/**
 * NSID — Namespaced Identifiers for cross-Node type interoperability.
 *
 * Every ObjectModel type definition can have a globally stable NSID in
 * reverse-DNS format. This enables cross-Node object references: when two
 * Nodes both register `io.prismapp.productivity.task`, they share a type
 * vocabulary and can exchange objects that render richly on both sides.
 *
 * Format: `authority.segment1.segment2...` (AT Protocol lexicon style)
 *
 * Examples:
 *   io.prismapp.productivity.task       — Prism's task type
 *   io.lattice.simulacra.character      — Lattice's character type
 *   com.mystudio.mygame.faction         — Studio's custom type
 *
 * Local types without an NSID (e.g. `type: 'task'`) are Node-local and
 * cannot participate in federation. An NSID can be assigned at any time
 * without changing the local type string.
 */

// ── Types ────────────────────────────────────────────────────────────────────

/**
 * A valid NSID string. At least two dot-separated segments, each segment
 * lowercase alphanumeric + hyphens, authority is reversed domain.
 */
export type NSID = string & { readonly __brand: "NSID" };

/**
 * A Prism object address — globally unique reference to an object on a specific Node.
 *
 * Format: `prism://did:web:node.example.com/objects/object-id`
 */
export type PrismAddress = string & { readonly __brand: "PrismAddress" };

// ── Validation ───────────────────────────────────────────────────────────────

const NSID_RE = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*){2,}$/;
const PRISM_ADDRESS_RE = /^prism:\/\/[^/]+\/objects\/[^/]+$/;

/** Validate an NSID string. Returns true if it matches the format. */
export function isValidNSID(s: string): s is NSID {
  return NSID_RE.test(s);
}

/** Parse a string as an NSID. Returns null if invalid. */
export function parseNSID(s: string): NSID | null {
  return isValidNSID(s) ? s : null;
}

/** Create an NSID from parts: `nsid('io.prismapp', 'productivity', 'task')` */
export function nsid(...parts: string[]): NSID {
  const joined = parts.join(".");
  if (!isValidNSID(joined)) {
    throw new Error(
      `Invalid NSID: '${joined}'. Must be reverse-DNS with 3+ segments, lowercase.`,
    );
  }
  return joined;
}

/** Extract the authority (first two segments) from an NSID. */
export function nsidAuthority(id: NSID): string {
  const parts = id.split(".");
  return parts.slice(0, 2).join(".");
}

/** Extract the name (last segment) from an NSID. */
export function nsidName(id: NSID): string {
  const parts = id.split(".");
  return parts[parts.length - 1] ?? "";
}

// ── Prism addresses ─────────────────────────────────────────────────────────

/** Validate a Prism object address. */
export function isValidPrismAddress(s: string): s is PrismAddress {
  return PRISM_ADDRESS_RE.test(s);
}

/** Build a Prism address from a Node DID and object ID. */
export function prismAddress(
  nodeDid: string,
  objectId: string,
): PrismAddress {
  return `prism://${nodeDid}/objects/${objectId}` as PrismAddress;
}

/** Parse a Prism address into its components. Returns null if invalid. */
export function parsePrismAddress(
  addr: string,
): { nodeDid: string; objectId: string } | null {
  if (!addr.startsWith("prism://")) return null;
  const rest = addr.slice(8); // after 'prism://'
  const slashIdx = rest.indexOf("/objects/");
  if (slashIdx === -1) return null;
  return {
    nodeDid: rest.slice(0, slashIdx),
    objectId: rest.slice(slashIdx + 9), // after '/objects/'
  };
}

// ── NSID Registry ────────────────────────────────────────────────────────────

/**
 * Maps NSIDs to local type strings and vice versa.
 * Maintained alongside the ObjectRegistry.
 */
export class NSIDRegistry {
  private nsidToType = new Map<NSID, string>();
  private typeToNsid = new Map<string, NSID>();

  register(localType: string, nsidStr: string): void {
    const id = parseNSID(nsidStr);
    if (!id) throw new Error(`Invalid NSID: '${nsidStr}'`);
    this.nsidToType.set(id, localType);
    this.typeToNsid.set(localType, id);
  }

  getNSID(localType: string): NSID | undefined {
    return this.typeToNsid.get(localType);
  }

  getLocalType(nsidStr: NSID | string): string | undefined {
    return this.nsidToType.get(nsidStr as NSID);
  }

  hasNSID(nsidStr: NSID | string): boolean {
    return this.nsidToType.has(nsidStr as NSID);
  }

  entries(): ReadonlyMap<NSID, string> {
    return this.nsidToType;
  }

  clear(): void {
    this.nsidToType.clear();
    this.typeToNsid.clear();
  }
}
