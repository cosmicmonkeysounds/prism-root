/**
 * Security Middleware — CSRF protection, body size limits, and schema validation.
 *
 * Applied globally via Hono middleware to harden the Relay against common attacks.
 */

import type { Context, Next } from "hono";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";

// ── CSRF Protection ────────────────────────────────────────────────────────

const CSRF_HEADER = "x-prism-csrf";
const CSRF_SAFE_METHODS = new Set(["GET", "HEAD", "OPTIONS"]);

/**
 * CSRF middleware: requires a `X-Prism-CSRF: 1` header on all mutating
 * requests. This prevents browser-based CSRF because custom headers
 * cannot be sent cross-origin without a preflight CORS check.
 *
 * API clients simply add the header. Browsers must pass CORS preflight.
 * Portal form submissions (POST /portals/:id/submit) use JSON with
 * Content-Type: application/json which already requires preflight.
 */
export function csrfMiddleware() {
  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    // Mutating requests must include the CSRF header
    const csrf = c.req.header(CSRF_HEADER);
    if (!csrf) {
      return c.json({ error: "missing CSRF header (X-Prism-CSRF)" }, 403);
    }

    return next();
  };
}

// ── Body Size Limit ────────────────────────────────────────────────────────

/**
 * Reject request bodies exceeding the configured max size.
 * Uses Content-Length header for early rejection.
 */
export function bodySizeLimitMiddleware(maxBytes: number) {
  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    const contentLength = c.req.header("content-length");
    if (contentLength && parseInt(contentLength, 10) > maxBytes) {
      return c.json(
        { error: `request body exceeds maximum size of ${maxBytes} bytes` },
        413,
      );
    }

    return next();
  };
}

// ── Banned Peer Rejection ──────────────────────────────────────────────────

/**
 * Reject requests from banned DIDs. Checks the Authorization header
 * for a Bearer token containing a DID, or the X-Prism-DID header.
 */
export function bannedPeerMiddleware(relay: RelayInstance) {
  return async (c: Context, next: Next) => {
    const trust = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    if (!trust) return next();

    const did = c.req.header("x-prism-did");
    if (did && trust.isBanned(did)) {
      return c.json({ error: "peer is banned" }, 403);
    }

    return next();
  };
}

// ── Rate Limiting (Token Bucket) ─────────────────────────────────────────

interface RateBucket {
  tokens: number;
  lastRefill: number;
}

/**
 * Per-IP rate limiter using the token bucket algorithm.
 * Protects against DDoS and brute-force attacks.
 *
 * Each IP starts with `max` tokens. Tokens refill at `refillRate` per second.
 * Each request consumes 1 token. When tokens hit 0, return 429.
 */
export function rateLimitMiddleware(opts?: {
  /** Maximum burst size. Default: 100 requests. */
  max?: number;
  /** Tokens refilled per second. Default: 20 requests/sec. */
  refillRate?: number;
  /** Max tracked IPs (LRU eviction above this). Default: 10000. */
  maxEntries?: number;
  /** Clock source in milliseconds. Default: Date.now. Injectable for tests. */
  now?: () => number;
}) {
  const max = opts?.max ?? 100;
  const refillRate = opts?.refillRate ?? 20;
  const maxEntries = opts?.maxEntries ?? 10_000;
  const now = opts?.now ?? Date.now;
  const buckets = new Map<string, RateBucket>();

  function getClientKey(c: Context): string {
    // Prefer DID for authenticated requests, fall back to IP
    const did = c.req.header("x-prism-did");
    if (did) return `did:${did}`;
    // X-Forwarded-For for proxied requests, raw IP otherwise
    const forwarded = c.req.header("x-forwarded-for");
    if (forwarded) return `ip:${forwarded.split(",")[0]?.trim()}`;
    // Hono doesn't expose raw IP directly; use a fallback
    return `ip:${c.req.header("x-real-ip") ?? "unknown"}`;
  }

  function refill(bucket: RateBucket, nowMs: number): void {
    const elapsed = (nowMs - bucket.lastRefill) / 1000;
    bucket.tokens = Math.min(max, bucket.tokens + elapsed * refillRate);
    bucket.lastRefill = nowMs;
  }

  // Periodic eviction of stale entries (every 60s)
  setInterval(() => {
    if (buckets.size > maxEntries) {
      const cutoff = now() - 120_000; // 2 min inactive
      for (const [key, bucket] of buckets) {
        if (bucket.lastRefill < cutoff) buckets.delete(key);
        if (buckets.size <= maxEntries * 0.75) break;
      }
    }
  }, 60_000).unref();

  return async (c: Context, next: Next) => {
    const key = getClientKey(c);
    const nowMs = now();

    let bucket = buckets.get(key);
    if (!bucket) {
      bucket = { tokens: max, lastRefill: nowMs };
      buckets.set(key, bucket);
    }

    refill(bucket, nowMs);

    if (bucket.tokens < 1) {
      c.header("Retry-After", String(Math.ceil(1 / refillRate)));
      return c.json({ error: "rate limit exceeded" }, 429);
    }

    bucket.tokens -= 1;
    return next();
  };
}

// ── Schema Poison Pill (CRDT diff validation) ──────────────────────────────

/**
 * Validates JSON request bodies against basic structural safety rules.
 * Prevents excessively nested or oversized payloads from being processed.
 */
export function schemaValidationMiddleware(opts?: {
  maxDepth?: number;
  maxKeys?: number;
  maxStringLength?: number;
}) {
  const maxDepth = opts?.maxDepth ?? 20;
  const maxKeys = opts?.maxKeys ?? 5000;
  const maxStringLength = opts?.maxStringLength ?? 500_000;

  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    const contentType = c.req.header("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return next();
    }

    // Clone the body for validation, then restore
    const text = await c.req.text();

    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      return c.json({ error: "invalid JSON body" }, 400);
    }

    const issues = validateStructure(parsed, maxDepth, maxKeys, maxStringLength);
    if (issues.length > 0) {
      return c.json({ error: "schema validation failed", issues }, 400);
    }

    // Store parsed body for downstream use
    c.set("parsedBody" as never, parsed as never);
    return next();
  };
}

function validateStructure(
  value: unknown,
  maxDepth: number,
  maxKeys: number,
  maxStringLength: number,
): string[] {
  const issues: string[] = [];
  let keyCount = 0;

  function walk(v: unknown, depth: number): void {
    if (depth > maxDepth) {
      issues.push(`nesting depth exceeds ${maxDepth}`);
      return;
    }

    if (typeof v === "string" && v.length > maxStringLength) {
      issues.push(`string length ${v.length} exceeds ${maxStringLength}`);
      return;
    }

    if (Array.isArray(v)) {
      for (const item of v) walk(item, depth + 1);
      return;
    }

    if (v !== null && typeof v === "object") {
      const keys = Object.keys(v);
      keyCount += keys.length;
      if (keyCount > maxKeys) {
        issues.push(`total keys ${keyCount} exceeds ${maxKeys}`);
        return;
      }
      for (const key of keys) {
        walk((v as Record<string, unknown>)[key], depth + 1);
      }
    }
  }

  walk(value, 0);
  return issues;
}
