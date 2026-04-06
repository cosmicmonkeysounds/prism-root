/**
 * @prism/core — PrismIdentity
 *
 * W3C DID identity backed by Ed25519 keypairs. Supports did:key and did:web
 * methods, payload signing/verification, and threshold multi-sig for shared
 * vault ownership.
 *
 * All crypto uses Web Crypto API (SubtleCrypto) for portability across
 * Node.js 20+, browsers, and Tauri WebView.
 */

import type {
  DID,
  DIDDocument,
  VerificationMethod,
  KeyHandle,
  PrismIdentity,
  ResolvedIdentity,
  CreateIdentityOptions,
  ResolveIdentityOptions,
  MultiSigConfig,
  MultiSignature,
  PartialSignature,
} from "./identity-types.js";

// ── Base58btc ───────────────────────────────────────────────────────────────

const BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function encodeBase58(bytes: Uint8Array): string {
  // Count leading zeros
  let zeroes = 0;
  for (const b of bytes) {
    if (b !== 0) break;
    zeroes++;
  }

  // Convert to base58
  const digits: number[] = [];
  for (const byte of bytes) {
    let carry = byte;
    for (let j = 0; j < digits.length; j++) {
      carry += (digits[j] as number) << 8;
      digits[j] = carry % 58;
      carry = (carry / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }

  let result = "";
  for (let i = 0; i < zeroes; i++) result += "1";
  for (let i = digits.length - 1; i >= 0; i--) result += BASE58_ALPHABET[digits[i] as number];
  return result;
}

function decodeBase58(str: string): Uint8Array {
  // Count leading '1's
  let zeroes = 0;
  for (const ch of str) {
    if (ch !== "1") break;
    zeroes++;
  }

  const digits: number[] = [];
  for (const ch of str) {
    const idx = BASE58_ALPHABET.indexOf(ch);
    if (idx === -1) throw new Error(`Invalid base58 character: ${ch}`);

    let carry = idx;
    for (let j = 0; j < digits.length; j++) {
      carry += (digits[j] as number) * 58;
      digits[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      digits.push(carry & 0xff);
      carry >>= 8;
    }
  }

  const result = new Uint8Array(zeroes + digits.length);
  for (let i = digits.length - 1; i >= 0; i--) {
    result[zeroes + (digits.length - 1 - i)] = digits[i] as number;
  }
  return result;
}

// ── Multicodec ──────────────────────────────────────────────────────────────

/** Ed25519 public key multicodec prefix: 0xed 0x01 */
const ED25519_MULTICODEC_PREFIX = new Uint8Array([0xed, 0x01]);

function encodeMulticodecEd25519(publicKey: Uint8Array): Uint8Array {
  const result = new Uint8Array(ED25519_MULTICODEC_PREFIX.length + publicKey.length);
  result.set(ED25519_MULTICODEC_PREFIX, 0);
  result.set(publicKey, ED25519_MULTICODEC_PREFIX.length);
  return result;
}

function decodeMulticodecEd25519(bytes: Uint8Array): Uint8Array {
  if (
    bytes.length < 2 ||
    bytes[0] !== 0xed ||
    bytes[1] !== 0x01
  ) {
    throw new Error("Not an Ed25519 multicodec key");
  }
  return bytes.slice(2);
}

// ── DID:key encoding ────────────────────────────────────────────────────────

function publicKeyToDidKey(publicKey: Uint8Array): DID {
  const multicodec = encodeMulticodecEd25519(publicKey);
  const multibase = "z" + encodeBase58(multicodec);
  return `did:key:${multibase}` as DID;
}

function didKeyToPublicKey(did: DID): Uint8Array {
  const parts = did.split(":");
  if (parts.length < 3 || parts[0] !== "did" || parts[1] !== "key") {
    throw new Error(`Invalid did:key format: ${did}`);
  }
  const multibase = parts.slice(2).join(":");
  if (!multibase.startsWith("z")) {
    throw new Error(`Unsupported multibase encoding (expected 'z' prefix): ${multibase}`);
  }
  const decoded = decodeBase58(multibase.slice(1));
  return decodeMulticodecEd25519(decoded);
}

// ── DID:web encoding ────────────────────────────────────────────────────────

function buildDidWeb(domain: string, path?: string): DID {
  const encodedDomain = domain.replace(/:/g, "%3A");
  if (path) {
    const encodedPath = path.split("/").map(s => encodeURIComponent(s)).join(":");
    return `did:web:${encodedDomain}:${encodedPath}` as DID;
  }
  return `did:web:${encodedDomain}` as DID;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function getSubtle(options?: { subtle?: SubtleCrypto }): SubtleCrypto {
  if (options?.subtle) return options.subtle;
  if (typeof globalThis.crypto !== "undefined" && globalThis.crypto.subtle) {
    return globalThis.crypto.subtle;
  }
  throw new Error("SubtleCrypto not available — pass `subtle` option or run in a secure context");
}

/** Cast Uint8Array to BufferSource for Web Crypto API compatibility with strict TS. */
function buf(bytes: Uint8Array): BufferSource {
  return bytes as unknown as BufferSource;
}

function base64urlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function buildDIDDocument(did: DID, publicKey: Uint8Array, created: string): DIDDocument {
  const vm: VerificationMethod = {
    id: `${did}#key-1`,
    type: "Ed25519VerificationKey2020",
    controller: did,
    publicKeyMultibase: "z" + encodeBase58(encodeMulticodecEd25519(publicKey)),
  };

  return {
    "@context": [
      "https://www.w3.org/ns/did/v1",
      "https://w3id.org/security/suites/ed2519-2020/v1",
    ],
    id: did,
    verificationMethod: [vm],
    authentication: [vm.id],
    assertionMethod: [vm.id],
    created,
  };
}

// ── Key generation ──────────────────────────────────────────────────────────

async function generateEd25519KeyHandle(subtle: SubtleCrypto): Promise<KeyHandle> {
  const keyPair = await subtle.generateKey("Ed25519", true, [
    "sign",
    "verify",
  ]) as CryptoKeyPair;

  const rawPublic = await subtle.exportKey("raw", keyPair.publicKey);
  const publicKeyBytes = new Uint8Array(rawPublic);

  return {
    signingKey: keyPair.privateKey,
    verifyKey: keyPair.publicKey,
    publicKeyBytes,
  };
}

async function importPublicKey(
  subtle: SubtleCrypto,
  publicKeyBytes: Uint8Array,
): Promise<CryptoKey> {
  return subtle.importKey("raw", buf(publicKeyBytes), "Ed25519", true, ["verify"]);
}

// ── Sign / Verify ───────────────────────────────────────────────────────────

async function signWithKey(subtle: SubtleCrypto, signingKey: CryptoKey, data: Uint8Array): Promise<Uint8Array> {
  const sig = await subtle.sign("Ed25519", signingKey, buf(data));
  return new Uint8Array(sig);
}

async function verifyWithKey(
  subtle: SubtleCrypto,
  verifyKey: CryptoKey,
  data: Uint8Array,
  signature: Uint8Array,
): Promise<boolean> {
  return subtle.verify("Ed25519", verifyKey, buf(signature), buf(data));
}

// ── Public API ──────────────────────────────────────────────────────────────

/**
 * Generate a new Ed25519 identity with a DID document.
 *
 * @param options.method - "key" (default) or "web"
 * @param options.domain - Required for did:web
 * @param options.path   - Optional path for did:web
 */
export async function createIdentity(
  options: CreateIdentityOptions = {},
): Promise<PrismIdentity> {
  const { method = "key", domain, path } = options;
  const subtle = getSubtle(options);

  const keyHandle = await generateEd25519KeyHandle(subtle);

  let did: DID;
  if (method === "key") {
    did = publicKeyToDidKey(keyHandle.publicKeyBytes);
  } else if (method === "web") {
    if (!domain) throw new Error("did:web requires a domain");
    did = buildDidWeb(domain, path);
  } else {
    throw new Error(`Unsupported DID method: ${method}`);
  }

  const document = buildDIDDocument(did, keyHandle.publicKeyBytes, new Date().toISOString());

  return {
    did,
    document,
    keyHandle,
    signPayload: (data: Uint8Array) => signWithKey(subtle, keyHandle.signingKey, data),
    verifySignature: (data: Uint8Array, signature: Uint8Array) =>
      verifyWithKey(subtle, keyHandle.verifyKey, data, signature),
  };
}

/**
 * Resolve a DID to its public key and verification method.
 * For did:key, the public key is extracted directly from the DID string.
 * For did:web, a DIDDocument resolver would be needed (not yet implemented).
 */
export async function resolveIdentity(
  did: DID,
  options: ResolveIdentityOptions = {},
): Promise<ResolvedIdentity> {
  const subtle = getSubtle(options);
  const parts = did.split(":");
  const method = parts[1];

  if (method === "key") {
    const publicKeyBytes = didKeyToPublicKey(did);
    const verifyKey = await importPublicKey(subtle, publicKeyBytes);
    const document = buildDIDDocument(did, publicKeyBytes, new Date().toISOString());

    return {
      did,
      document,
      publicKey: publicKeyBytes,
      verifySignature: (data: Uint8Array, signature: Uint8Array) =>
        verifyWithKey(subtle, verifyKey, data, signature),
    };
  }

  if (method === "web") {
    throw new Error("did:web resolution requires a network resolver (not yet implemented)");
  }

  throw new Error(`Unsupported DID method: ${method}`);
}

/**
 * Sign a payload with a PrismIdentity. Convenience wrapper.
 */
export async function signPayload(
  identity: PrismIdentity,
  data: Uint8Array,
): Promise<Uint8Array> {
  return identity.signPayload(data);
}

/**
 * Verify a signature against a DID. Resolves the DID to get the public key.
 */
export async function verifySignature(
  did: DID,
  data: Uint8Array,
  signature: Uint8Array,
  options: ResolveIdentityOptions = {},
): Promise<boolean> {
  const resolved = await resolveIdentity(did, options);
  return resolved.verifySignature(data, signature);
}

// ── Multi-sig ───────────────────────────────────────────────────────────────

/**
 * Create a multi-sig configuration for shared vault ownership.
 */
export function createMultiSigConfig(
  threshold: number,
  signers: DID[],
): MultiSigConfig {
  if (threshold < 1) throw new Error("Threshold must be at least 1");
  if (threshold > signers.length) {
    throw new Error(`Threshold (${threshold}) exceeds number of signers (${signers.length})`);
  }
  if (new Set(signers).size !== signers.length) {
    throw new Error("Duplicate signers not allowed");
  }
  return { threshold, signers: [...signers] };
}

/**
 * Collect a partial signature from one signer.
 */
export async function createPartialSignature(
  identity: PrismIdentity,
  data: Uint8Array,
): Promise<PartialSignature> {
  const signature = await identity.signPayload(data);
  return { signerDid: identity.did, signature };
}

/**
 * Assemble partial signatures into a MultiSignature.
 */
export function assembleMultiSignature(
  config: MultiSigConfig,
  partials: PartialSignature[],
): MultiSignature {
  // Verify all signers are in the config
  for (const partial of partials) {
    if (!config.signers.includes(partial.signerDid)) {
      throw new Error(`Signer ${partial.signerDid} is not in the multi-sig config`);
    }
  }

  // Check for duplicate signers
  const signerDids = partials.map(p => p.signerDid);
  if (new Set(signerDids).size !== signerDids.length) {
    throw new Error("Duplicate partial signatures from the same signer");
  }

  return {
    threshold: config.threshold,
    signatures: [...partials],
  };
}

/**
 * Verify a multi-signature meets the threshold requirement.
 * Each partial signature is individually verified against the signer's DID.
 */
export async function verifyMultiSignature(
  config: MultiSigConfig,
  multiSig: MultiSignature,
  data: Uint8Array,
  options: ResolveIdentityOptions = {},
): Promise<boolean> {
  if (multiSig.signatures.length < config.threshold) return false;

  let validCount = 0;
  for (const partial of multiSig.signatures) {
    if (!config.signers.includes(partial.signerDid)) continue;

    const resolved = await resolveIdentity(partial.signerDid, options);
    const valid = await resolved.verifySignature(data, partial.signature);
    if (valid) validCount++;
    if (validCount >= config.threshold) return true;
  }

  return false;
}

// ── Utility exports ─────────────────────────────────────────────────────────

export { encodeBase58, decodeBase58, publicKeyToDidKey, didKeyToPublicKey, base64urlEncode };
