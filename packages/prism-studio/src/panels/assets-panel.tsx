/**
 * Assets Panel — virtual file system browser.
 *
 * Import binary files, browse content-addressed blobs, manage locks.
 */

import { useState, useCallback } from "react";
import { useVfs, useKernel } from "../kernel/index.js";
import type { BinaryRef } from "@prism/core/vfs";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "1rem",
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  header: {
    fontSize: "1.25rem",
    fontWeight: 600,
    marginBottom: "1rem",
    color: "#e5e5e5",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  cardHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: "0.375rem",
  },
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  btn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnPrimary: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  btnDanger: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#3b1111",
    border: "1px solid #5c2020",
    borderRadius: 3,
    color: "#f87171",
    cursor: "pointer",
  },
  mono: {
    fontFamily: "monospace",
    fontSize: "0.6875rem",
    color: "#4fc1ff",
    wordBreak: "break-all" as const,
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#333",
    color: "#888",
    marginLeft: "0.375rem",
  },
  lockBadge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#3b2911",
    color: "#f59e0b",
    marginLeft: "0.375rem",
  },
} as const;

// ── File size formatter ───────────────────────────────────────────────────

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ── Asset Card ────────────────────────────────────────────────────────────

function AssetCard({
  asset,
  locked,
  onRemove,
  onLock,
  onUnlock,
}: {
  asset: BinaryRef;
  locked: boolean;
  onRemove: () => void;
  onLock: () => void;
  onUnlock: () => void;
}) {
  return (
    <div style={styles.card} data-testid={`asset-${asset.hash.slice(0, 8)}`}>
      <div style={styles.cardHeader as React.CSSProperties}>
        <div>
          <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
            {asset.filename}
          </span>
          <span style={styles.badge}>{asset.mimeType}</span>
          <span style={styles.badge}>{formatSize(asset.size)}</span>
          {locked && <span style={styles.lockBadge}>Locked</span>}
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          {locked ? (
            <button style={styles.btn} onClick={onUnlock} data-testid={`unlock-${asset.hash.slice(0, 8)}`}>
              Unlock
            </button>
          ) : (
            <button style={styles.btn} onClick={onLock} data-testid={`lock-${asset.hash.slice(0, 8)}`}>
              Lock
            </button>
          )}
          <button style={styles.btnDanger} onClick={onRemove} data-testid={`remove-asset-${asset.hash.slice(0, 8)}`}>
            Remove
          </button>
        </div>
      </div>
      <div style={styles.mono}>{asset.hash}</div>
      <div style={styles.meta}>
        Imported: {new Date(asset.importedAt).toLocaleString()}
      </div>
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────

export function AssetsPanel() {
  const kernel = useKernel();
  const { files, locks, importFile, removeFile, acquireLock, releaseLock } = useVfs();
  const [importName, setImportName] = useState("");
  const [importMime, setImportMime] = useState("application/octet-stream");
  const [importData, setImportData] = useState("");
  const lockedHashes = new Set(locks.map((l) => l.hash));

  const handleImportText = useCallback(async () => {
    if (!importName.trim() || !importData.trim()) return;
    const data = new TextEncoder().encode(importData);
    const ref = await importFile(data, importName.trim(), importMime.trim() || "text/plain");
    setImportName("");
    setImportData("");
    kernel.notifications.add({ title: `Imported: ${ref.filename}`, kind: "success" });
  }, [importName, importMime, importData, importFile, kernel]);

  /**
   * Import a File picked from the OS file picker. Reads the whole blob,
   * routes it through the VFS, and — when an image is dropped — offers to
   * create a matching `image` block under the current selection.
   */
  const handleImportBinary = useCallback(
    async (fileList: FileList | null) => {
      if (!fileList || fileList.length === 0) return;
      for (const file of Array.from(fileList)) {
        const buffer = await file.arrayBuffer();
        const ref = await importFile(
          new Uint8Array(buffer),
          file.name,
          file.type || "application/octet-stream",
        );
        kernel.notifications.add({
          title: `Imported: ${ref.filename}`,
          body: `${ref.size} bytes — ${ref.mimeType}`,
          kind: "success",
        });

        // Auto-create an image block when the selection is a page/section
        // and the file is actually an image.
        if (file.type.startsWith("image/")) {
          const sel = kernel.atoms.getState().selectedId;
          const parent = sel ? kernel.store.getObject(sel) : null;
          if (parent && (parent.type === "section" || parent.type === "page")) {
            const siblings = kernel.store.listObjects({ parentId: parent.id });
            kernel.createObject({
              type: "image",
              name: file.name,
              parentId: parent.id,
              position: siblings.length,
              status: null,
              tags: [],
              date: null,
              endDate: null,
              description: "",
              color: null,
              image: `vfs://${ref.hash}`,
              pinned: false,
              data: {
                src: `vfs://${ref.hash}`,
                alt: file.name,
                mimeType: ref.mimeType,
                hash: ref.hash,
              },
            });
          }
        }
      }
    },
    [importFile, kernel],
  );

  const handleRemove = useCallback(
    async (hash: string, filename: string) => {
      await removeFile(hash);
      kernel.notifications.add({ title: `Removed: ${filename}`, kind: "info" });
    },
    [removeFile, kernel],
  );

  const handleLock = useCallback(
    (hash: string) => {
      try {
        acquireLock(hash, "Manual lock from Assets panel");
        kernel.notifications.add({ title: "Lock acquired", kind: "info" });
      } catch (err) {
        kernel.notifications.add({
          title: "Lock failed",
          kind: "error",
          body: err instanceof Error ? err.message : "Unknown error",
        });
      }
    },
    [acquireLock, kernel],
  );

  const handleUnlock = useCallback(
    (hash: string) => {
      try {
        releaseLock(hash);
        kernel.notifications.add({ title: "Lock released", kind: "info" });
      } catch (err) {
        kernel.notifications.add({
          title: "Unlock failed",
          kind: "error",
          body: err instanceof Error ? err.message : "Unknown error",
        });
      }
    },
    [releaseLock, kernel],
  );

  return (
    <div style={styles.container} data-testid="assets-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Assets</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {files.length} file(s) | {locks.length} lock(s)
        </span>
      </div>

      {/* Upload binary file(s) from the OS */}
      <div style={styles.card} data-testid="upload-binary-form">
        <div style={styles.sectionTitle}>Upload File(s)</div>
        <label
          style={{
            display: "block",
            padding: "8px 10px",
            fontSize: 11,
            background: "#333",
            border: "1px dashed #555",
            borderRadius: 3,
            color: "#ccc",
            cursor: "pointer",
            textAlign: "center",
          }}
          data-testid="upload-binary-label"
        >
          Choose image or binary file(s)…
          <input
            type="file"
            multiple
            style={{ display: "none" }}
            data-testid="upload-binary-input"
            onChange={(e) => {
              void handleImportBinary(e.target.files);
              e.target.value = "";
            }}
          />
        </label>
        <div style={{ fontSize: 10, color: "#888", marginTop: 4 }}>
          Selecting an image while a page or section is selected will auto-create an image block.
        </div>
      </div>

      {/* Import form */}
      <div style={styles.card} data-testid="import-asset-form">
        <div style={styles.sectionTitle}>Import File</div>
        <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
          <input
            style={{ ...styles.btn, flex: 1, background: "#333", border: "1px solid #444", color: "#e5e5e5", padding: "4px 8px" }}
            placeholder="Filename..."
            value={importName}
            onChange={(e) => setImportName(e.target.value)}
            data-testid="import-filename-input"
          />
          <input
            style={{ ...styles.btn, flex: 1, background: "#333", border: "1px solid #444", color: "#e5e5e5", padding: "4px 8px" }}
            placeholder="MIME type..."
            value={importMime}
            onChange={(e) => setImportMime(e.target.value)}
            data-testid="import-mime-input"
          />
        </div>
        <textarea
          style={{
            background: "#333",
            border: "1px solid #444",
            borderRadius: "0.25rem",
            padding: "0.375rem 0.5rem",
            color: "#e5e5e5",
            fontSize: "0.75rem",
            fontFamily: "monospace",
            width: "100%",
            outline: "none",
            boxSizing: "border-box" as const,
            resize: "vertical" as const,
            height: 60,
            marginBottom: 6,
          }}
          placeholder="File content (text)..."
          value={importData}
          onChange={(e) => setImportData(e.target.value)}
          data-testid="import-data-input"
        />
        <button
          style={styles.btnPrimary}
          onClick={handleImportText}
          data-testid="import-asset-btn"
        >
          Import
        </button>
      </div>

      {/* Lock summary */}
      {locks.length > 0 && (
        <>
          <div style={styles.sectionTitle}>Active Locks ({locks.length})</div>
          {locks.map((lock) => (
            <div key={lock.hash} style={{ ...styles.card, borderColor: "#5c4a11" }} data-testid={`lock-${lock.hash.slice(0, 8)}`}>
              <div style={styles.mono}>{lock.hash}</div>
              <div style={styles.meta}>
                By: {lock.lockedBy} | At: {new Date(lock.lockedAt).toLocaleString()}
                {lock.reason && ` | ${lock.reason}`}
              </div>
            </div>
          ))}
        </>
      )}

      {/* Asset list */}
      <div style={styles.sectionTitle}>Files ({files.length})</div>
      {files.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
          No files imported yet. Use the form above to import.
        </div>
      )}
      {files.map((asset) => (
        <AssetCard
          key={asset.hash}
          asset={asset}
          locked={lockedHashes.has(asset.hash)}
          onRemove={() => handleRemove(asset.hash, asset.filename)}
          onLock={() => handleLock(asset.hash)}
          onUnlock={() => handleUnlock(asset.hash)}
        />
      ))}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const ASSETS_LENS_ID = lensId("assets");

export const assetsLensManifest: LensManifest = {

  id: ASSETS_LENS_ID,
  name: "Assets",
  icon: "\uD83D\uDCC1",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-assets", name: "Switch to Assets", shortcut: ["f"], section: "Navigation" }],
  },
};

export const assetsLensBundle: LensBundle = defineLensBundle(
  assetsLensManifest,
  AssetsPanel,
);
