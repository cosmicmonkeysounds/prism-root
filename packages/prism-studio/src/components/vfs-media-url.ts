/**
 * vfs:// media URL scheme — a stable way to reference VFS-stored binaries
 * from block props that the assets panel already writes (`vfs://<hash>`).
 *
 * Renderers and custom fields use these helpers to resolve a content-
 * addressed hash through `kernel.vfs` to a blob URL the DOM can consume.
 *
 * The blob-URL cache is module-scoped and keyed by hash so the same asset
 * referenced from multiple blocks reuses one URL — saving both memory and
 * network/IPC work. Cached URLs live for the lifetime of the page; that's
 * acceptable because VFS content is immutable under its hash.
 */

import { useEffect, useState } from "react";
import type { VfsManager } from "@prism/core/vfs";

export const VFS_SCHEME = "vfs://";

/** Is this value a `vfs://<hash>` URL? */
export function isVfsMediaUrl(input: unknown): input is string {
  return typeof input === "string" && input.startsWith(VFS_SCHEME) && input.length > VFS_SCHEME.length;
}

/** Extract the hash from a `vfs://<hash>` URL, or null if the input is not one. */
export function parseVfsHash(input: unknown): string | null {
  if (!isVfsMediaUrl(input)) return null;
  const hash = input.slice(VFS_SCHEME.length);
  return hash.length > 0 ? hash : null;
}

/** Build a canonical `vfs://<hash>` URL. */
export function buildVfsMediaUrl(hash: string): string {
  return `${VFS_SCHEME}${hash}`;
}

/** True for any URL shape a browser media element can consume directly. */
export function isBrowserMediaUrl(input: unknown): input is string {
  if (typeof input !== "string") return false;
  return /^(https?:|data:|blob:)/i.test(input.trim());
}

// ── Blob URL cache ──────────────────────────────────────────────────────────

const blobUrlCache = new Map<string, string>();

/** Test-only: reset the module-scoped cache (and revoke any live URLs). */
export function __resetVfsMediaCache(): void {
  for (const url of blobUrlCache.values()) {
    try {
      URL.revokeObjectURL(url);
    } catch {
      // ignore — test env may not support revoke
    }
  }
  blobUrlCache.clear();
}

// ── Resolver ────────────────────────────────────────────────────────────────

/**
 * Resolve a block-prop source string to a URL the DOM can render.
 *
 * - `http(s)://`, `data:`, `blob:` → passthrough
 * - `vfs://<hash>` → blob URL minted from `vfs.exportFile`
 * - Anything else → `null` (caller renders a placeholder)
 */
export async function resolveVfsMediaUrl(
  src: string | undefined | null,
  vfs: VfsManager,
): Promise<string | null> {
  if (!src || typeof src !== "string") return null;
  const trimmed = src.trim();
  if (trimmed === "") return null;
  if (!isVfsMediaUrl(trimmed)) {
    return isBrowserMediaUrl(trimmed) ? trimmed : null;
  }

  const hash = parseVfsHash(trimmed);
  if (!hash) return null;

  const cached = blobUrlCache.get(hash);
  if (cached) return cached;

  const stat = await vfs.stat(hash);
  if (!stat) return null;

  const bytes = await vfs.exportFile({
    hash,
    filename: "",
    mimeType: stat.mimeType,
    size: stat.size,
    importedAt: stat.createdAt,
  });
  if (!bytes) return null;

  const blob = new Blob([bytes as BlobPart], { type: stat.mimeType });
  const url = URL.createObjectURL(blob);
  blobUrlCache.set(hash, url);
  return url;
}

// ── React hook ──────────────────────────────────────────────────────────────

/**
 * Reactive version of `resolveVfsMediaUrl`. Returns a tuple-ish object:
 *   - `url`: the resolved URL (or `null` while loading / unknown)
 *   - `loading`: true while an async resolve is in flight
 *
 * For passthrough URLs (http/https/data/blob) this resolves synchronously on
 * mount, so there is no flash. Only `vfs://` hashes incur an await.
 */
export function useResolvedMediaUrl(
  src: string | undefined | null,
  vfs: VfsManager,
): { url: string | null; loading: boolean } {
  const initial = (() => {
    if (!src || typeof src !== "string") return null;
    if (isVfsMediaUrl(src)) {
      const hash = parseVfsHash(src);
      return hash ? (blobUrlCache.get(hash) ?? null) : null;
    }
    return isBrowserMediaUrl(src) ? src : null;
  })();

  const [url, setUrl] = useState<string | null>(initial);
  const [loading, setLoading] = useState<boolean>(
    !!src && isVfsMediaUrl(src) && !parseVfsHash(src || "")
      ? false
      : !!src && isVfsMediaUrl(src) && !blobUrlCache.has(parseVfsHash(src) ?? ""),
  );

  useEffect(() => {
    if (!src || typeof src !== "string") {
      setUrl(null);
      setLoading(false);
      return;
    }
    if (!isVfsMediaUrl(src)) {
      setUrl(isBrowserMediaUrl(src) ? src : null);
      setLoading(false);
      return;
    }
    const hash = parseVfsHash(src);
    if (hash && blobUrlCache.has(hash)) {
      setUrl(blobUrlCache.get(hash) ?? null);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void resolveVfsMediaUrl(src, vfs).then((resolved) => {
      if (cancelled) return;
      setUrl(resolved);
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [src, vfs]);

  return { url, loading };
}
