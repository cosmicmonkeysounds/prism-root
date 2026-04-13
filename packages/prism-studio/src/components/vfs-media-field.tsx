/**
 * `mediaUploadField` — Puck custom field that stores a media URL via the
 * kernel's VFS, not a raw blob or data URL.
 *
 * Integration points (this is the "make sure it's wired into Prism's
 * systems" side of the design pass):
 *
 *   - `kernel.importFile()` → routes through `VfsManager` → content-
 *     addressed SHA-256 storage with automatic de-duplication.
 *   - `kernel.listFiles()` → the Assets panel and this field share the
 *     same BinaryRef index, so uploading here makes the file browseable
 *     from the Assets lens and vice versa.
 *   - Stored prop value: `vfs://<hash>` — the scheme already written by
 *     the Assets panel (`assets-panel.tsx`), resolved at render via
 *     `vfs-media-url.ts`.
 *   - `kernel.notifications.add()` → success / failure feedback in the
 *     same toast queue used by every other Studio action.
 *   - `kernel.vfs.stat()` → type/size/preview for existing blobs when
 *     picking from the vault.
 *
 * Three input modes:
 *   1. Upload from disk (file picker or drag-and-drop).
 *   2. Pick an existing file from the VFS, filtered by MIME prefix.
 *   3. Paste a plain URL (http/https) — still supported so authors can
 *      reference external assets.
 */

import { useCallback, useMemo, useRef, useState, type ReactElement } from "react";
import { FieldLabel, type Field } from "@measured/puck";
import type { BinaryRef } from "@prism/core/vfs";
import type { StudioKernel } from "../kernel/studio-kernel.js";
import {
  VFS_SCHEME,
  isVfsMediaUrl,
  parseVfsHash,
  buildVfsMediaUrl,
  useResolvedMediaUrl,
} from "./vfs-media-url.js";

// ── Styles ──────────────────────────────────────────────────────────────────

const baseInput = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 13,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#ffffff",
  color: "#0f172a",
  boxSizing: "border-box" as const,
};

const btn = {
  padding: "5px 10px",
  fontSize: 12,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#f8fafc",
  color: "#334155",
  cursor: "pointer",
};

const dropZone = {
  border: "1px dashed #94a3b8",
  borderRadius: 6,
  padding: "10px 12px",
  background: "#f8fafc",
  textAlign: "center" as const,
  fontSize: 12,
  color: "#64748b",
  cursor: "pointer",
  transition: "background 120ms ease",
};

const dropZoneActive = {
  ...dropZone,
  background: "#eef2ff",
  borderColor: "#6366f1",
  color: "#4338ca",
};

const previewBox = {
  marginTop: 8,
  border: "1px solid #e2e8f0",
  borderRadius: 6,
  padding: 8,
  background: "#ffffff",
};

const metaLine = {
  marginTop: 6,
  fontSize: 11,
  color: "#64748b",
  fontFamily: "ui-monospace, Menlo, Consolas, monospace",
  wordBreak: "break-all" as const,
};

// ── Helpers ────────────────────────────────────────────────────────────────

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Short label for the MIME-prefix filter, e.g. "image" → "images". */
function mediaKindLabel(prefix: string): string {
  switch (prefix) {
    case "image":
      return "image";
    case "video":
      return "video";
    case "audio":
      return "audio";
    default:
      return "file";
  }
}

/** Match a file's MIME against the field's accept prefix (empty = accept all). */
function matchesAccept(mimeType: string, accept: string): boolean {
  if (!accept) return true;
  return mimeType.startsWith(accept);
}

// ── Inner component (runs as a React component so hooks are legal) ─────────

export interface MediaUploadFieldInnerProps {
  kernel: StudioKernel;
  value: unknown;
  onChange: (next: string) => void;
  readOnly: boolean;
  label: string | undefined;
  /** MIME prefix (e.g. "image", "video", "audio"). Empty = all. */
  accept: string;
  /** What kind of media is this for? Drives placeholder + preview. */
  kind: "image" | "video" | "audio" | "file";
}

export function MediaUploadFieldInner(props: MediaUploadFieldInnerProps): ReactElement {
  const { kernel, value, onChange, readOnly, label, accept, kind } = props;
  const current = typeof value === "string" ? value : "";
  const [uploading, setUploading] = useState(false);
  const [picking, setPicking] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const { url: resolvedUrl, loading: resolving } = useResolvedMediaUrl(current, kernel.vfs);

  const currentIsVfs = isVfsMediaUrl(current);
  const currentHash = parseVfsHash(current);

  // Look up BinaryRef metadata for the selected VFS file so we can show
  // the original filename / MIME / size — kernel.listFiles() is the same
  // reactive source the Assets panel uses.
  const selectedRef = useMemo<BinaryRef | null>(() => {
    if (!currentHash) return null;
    const all = kernel.listFiles();
    return all.find((r) => r.hash === currentHash) ?? null;
  }, [currentHash, kernel]);

  const upload = useCallback(
    async (file: File) => {
      if (!matchesAccept(file.type, accept)) {
        const label = mediaKindLabel(accept);
        setError(`Expected ${label} file — got ${file.type || "unknown"}`);
        return;
      }
      setError(null);
      setUploading(true);
      try {
        const buffer = await file.arrayBuffer();
        const ref = await kernel.importFile(
          new Uint8Array(buffer),
          file.name,
          file.type || "application/octet-stream",
        );
        onChange(buildVfsMediaUrl(ref.hash));
        kernel.notifications.add({
          title: `Uploaded ${ref.filename}`,
          body: `${formatSize(ref.size)} — stored in vault`,
          kind: "success",
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : "Upload failed";
        setError(msg);
        kernel.notifications.add({
          title: "Upload failed",
          body: msg,
          kind: "error",
        });
      } finally {
        setUploading(false);
      }
    },
    [accept, kernel, onChange],
  );

  const onFilePicked = useCallback(
    (files: FileList | null) => {
      if (!files || files.length === 0) return;
      const file = files[0];
      if (file) void upload(file);
    },
    [upload],
  );

  const onDrop = useCallback(
    (e: React.DragEvent<HTMLLabelElement>) => {
      e.preventDefault();
      setDragging(false);
      if (readOnly) return;
      onFilePicked(e.dataTransfer?.files ?? null);
    },
    [readOnly, onFilePicked],
  );

  // Candidates to show in the "Pick from vault" picker.
  const vaultFiles = useMemo(() => {
    const all = kernel.listFiles();
    if (!accept) return all;
    return all.filter((r) => r.mimeType.startsWith(accept));
  }, [kernel, accept, picking]); // re-query when opening picker

  return (
    <FieldLabel label={label ?? ""} el="div" readOnly={readOnly}>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        {/* Drag-drop + click-to-upload */}
        <label
          style={dragging ? dropZoneActive : dropZone}
          onDragOver={(e) => {
            e.preventDefault();
            if (!readOnly) setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          data-testid="media-upload-dropzone"
        >
          {uploading
            ? "Uploading…"
            : dragging
              ? "Drop to upload"
              : `Drop ${mediaKindLabel(accept)} here or click to choose`}
          <input
            ref={fileInputRef}
            type="file"
            accept={accept ? `${accept}/*` : undefined}
            disabled={readOnly || uploading}
            style={{ display: "none" }}
            onChange={(e) => {
              onFilePicked(e.target.files);
              e.target.value = "";
            }}
            data-testid="media-upload-input"
          />
        </label>

        {/* Secondary actions */}
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          <button
            type="button"
            style={btn}
            onClick={() => setPicking((p) => !p)}
            disabled={readOnly}
            data-testid="media-upload-vault-toggle"
          >
            {picking ? "Hide vault" : `From vault (${vaultFiles.length})`}
          </button>
          {current ? (
            <button
              type="button"
              style={btn}
              onClick={() => {
                onChange("");
                setError(null);
              }}
              disabled={readOnly}
              data-testid="media-upload-clear"
            >
              Clear
            </button>
          ) : null}
        </div>

        {/* URL fallback — plain text for http(s) references */}
        <input
          type="text"
          value={current}
          placeholder="https://…  or  vfs://<hash>"
          disabled={readOnly}
          onChange={(e) => onChange(e.target.value)}
          style={baseInput}
          data-testid="media-upload-url"
          aria-label="Media URL"
        />

        {/* Error feedback */}
        {error ? (
          <div style={{ fontSize: 11, color: "#dc2626" }} data-testid="media-upload-error">
            {error}
          </div>
        ) : null}

        {/* Vault picker */}
        {picking ? (
          <div
            style={{
              maxHeight: 180,
              overflowY: "auto",
              border: "1px solid #e2e8f0",
              borderRadius: 4,
              background: "#f8fafc",
            }}
            data-testid="media-upload-vault-list"
          >
            {vaultFiles.length === 0 ? (
              <div style={{ padding: 10, fontSize: 12, color: "#94a3b8", textAlign: "center" }}>
                No {mediaKindLabel(accept)} files in the vault yet.
              </div>
            ) : (
              vaultFiles.map((ref) => {
                const active = currentHash === ref.hash;
                return (
                  <button
                    key={ref.hash}
                    type="button"
                    onClick={() => {
                      onChange(buildVfsMediaUrl(ref.hash));
                      setPicking(false);
                      setError(null);
                    }}
                    disabled={readOnly}
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      padding: "6px 10px",
                      fontSize: 12,
                      background: active ? "#eef2ff" : "transparent",
                      border: "none",
                      borderBottom: "1px solid #e2e8f0",
                      color: active ? "#4338ca" : "#334155",
                      cursor: "pointer",
                    }}
                    data-testid={`media-upload-vault-item-${ref.hash.slice(0, 8)}`}
                  >
                    <div style={{ fontWeight: 500 }}>{ref.filename}</div>
                    <div style={{ fontSize: 10, color: "#94a3b8" }}>
                      {ref.mimeType} — {formatSize(ref.size)}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        ) : null}

        {/* Preview */}
        {current && !resolving ? (
          <div style={previewBox} data-testid="media-upload-preview">
            {resolvedUrl ? (
              kind === "image" ? (
                <img
                  src={resolvedUrl}
                  alt={selectedRef?.filename ?? "Preview"}
                  style={{ maxWidth: "100%", maxHeight: 160, display: "block", margin: "0 auto" }}
                />
              ) : kind === "video" ? (
                <video
                  src={resolvedUrl}
                  controls
                  style={{ maxWidth: "100%", maxHeight: 160, display: "block", margin: "0 auto" }}
                />
              ) : kind === "audio" ? (
                <audio src={resolvedUrl} controls style={{ width: "100%" }} />
              ) : (
                <div style={{ fontSize: 12, color: "#64748b" }}>File linked.</div>
              )
            ) : (
              <div style={{ fontSize: 12, color: "#dc2626" }}>
                Could not resolve {current.startsWith(VFS_SCHEME) ? "vault file" : "URL"}.
              </div>
            )}
            {currentIsVfs && selectedRef ? (
              <div style={metaLine}>
                {selectedRef.filename} · {selectedRef.mimeType} · {formatSize(selectedRef.size)}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </FieldLabel>
  );
}

// ── Field factory ──────────────────────────────────────────────────────────

export interface MediaUploadFieldOptions {
  label?: string;
  /** MIME prefix for filtering and the HTML `accept` attribute. */
  accept?: "image" | "video" | "audio" | "";
}

/**
 * Build a Puck custom field bound to a specific StudioKernel.
 *
 * The kernel must be closed over — Puck calls the field's `render` as a
 * React component and there's no way to pass extra props from the config
 * side, so a factory that captures the kernel is the cleanest path.
 */
export function mediaUploadField(
  kernel: StudioKernel,
  opts: MediaUploadFieldOptions = {},
): Field<string> {
  const accept = opts.accept ?? "";
  const kind: "image" | "video" | "audio" | "file" = accept === "" ? "file" : accept;
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => (
      <MediaUploadFieldInner
        kernel={kernel}
        value={value}
        onChange={onChange}
        readOnly={readOnly ?? false}
        label={opts.label}
        accept={accept}
        kind={kind}
      />
    ),
  };
}
