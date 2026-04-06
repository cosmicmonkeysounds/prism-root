/**
 * @prism/core — Identity Types
 *
 * W3C DID-based identity for Prism nodes. Ed25519 keypairs provide
 * signing for CRDT updates and authentication for federation.
 */

// ── DID Methods ─────────────────────────────────────────────────────────────

/** Supported DID methods. */
export type DIDMethod = "key" | "web";

/** A W3C DID string (e.g. "did:key:z6Mk..."). */
export type DID = `did:${string}:${string}`;

// ── Key Material ────────────────────────────────────────────────────────────

/** Raw Ed25519 keypair as exportable bytes. */
export interface Ed25519KeyPair {
  /** 32-byte public key. */
  publicKey: Uint8Array;
  /** 64-byte private key (seed + public). */
  privateKey: Uint8Array;
}

/** Opaque handle wrapping CryptoKey objects for sign/verify. */
export interface KeyHandle {
  /** CryptoKey for signing. */
  signingKey: CryptoKey;
  /** CryptoKey for verification. */
  verifyKey: CryptoKey;
  /** Raw public key bytes. */
  publicKeyBytes: Uint8Array;
}

// ── DID Document ────────────────────────────────────────────────────────────

/** Verification method inside a DID document. */
export interface VerificationMethod {
  id: string;
  type: "Ed25519VerificationKey2020";
  controller: string;
  /** Base64url-encoded public key. */
  publicKeyMultibase: string;
}

/** Simplified W3C DID Document. */
export interface DIDDocument {
  "@context": string[];
  id: DID;
  verificationMethod: VerificationMethod[];
  authentication: string[];
  assertionMethod: string[];
  created: string;
}

// ── Prism Identity ──────────────────────────────────────────────────────────

/** A fully-resolved Prism identity with signing capabilities. */
export interface PrismIdentity {
  /** The DID string. */
  did: DID;
  /** The DID document. */
  document: DIDDocument;
  /** Opaque handle for cryptographic operations. */
  keyHandle: KeyHandle;
  /** Sign arbitrary payload bytes. Returns Ed25519 signature. */
  signPayload(data: Uint8Array): Promise<Uint8Array>;
  /** Verify a signature against this identity's public key. */
  verifySignature(data: Uint8Array, signature: Uint8Array): Promise<boolean>;
}

/** Resolved identity (public key only, no signing capability). */
export interface ResolvedIdentity {
  did: DID;
  document: DIDDocument;
  publicKey: Uint8Array;
  /** Verify a signature against this identity's public key. */
  verifySignature(data: Uint8Array, signature: Uint8Array): Promise<boolean>;
}

// ── Multi-sig ───────────────────────────────────────────────────────────────

/** A partial signature contribution from one signer. */
export interface PartialSignature {
  /** DID of the signer. */
  signerDid: DID;
  /** Ed25519 signature bytes. */
  signature: Uint8Array;
}

/** Aggregated multi-signature with threshold verification. */
export interface MultiSignature {
  /** Minimum number of valid signatures required. */
  threshold: number;
  /** All partial signatures collected. */
  signatures: PartialSignature[];
}

/** Configuration for multi-sig vault ownership. */
export interface MultiSigConfig {
  /** Required number of signatures to authorise an action. */
  threshold: number;
  /** DIDs of all authorised signers. */
  signers: DID[];
}

// ── Options ─────────────────────────────────────────────────────────────────

export interface CreateIdentityOptions {
  /** DID method to use. Default: "key". */
  method?: DIDMethod;
  /** For did:web — the domain (e.g. "example.com"). Required if method is "web". */
  domain?: string;
  /** For did:web — optional path (e.g. "users/alice"). */
  path?: string;
  /** Override crypto.subtle for testing. */
  subtle?: SubtleCrypto;
}

export interface ResolveIdentityOptions {
  /** Override crypto.subtle for testing. */
  subtle?: SubtleCrypto;
}
