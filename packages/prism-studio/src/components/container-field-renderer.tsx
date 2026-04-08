/**
 * ContainerFieldRenderer — inline file/image/PDF/audio preview for ContainerSlots.
 *
 * Auto-detects MIME type from the VFS BinaryRef and renders:
 *   - Images: inline thumbnail with lazy loading
 *   - Audio: native audio player
 *   - Video: native video player
 *   - PDF: embedded object viewer
 *   - Other: file icon with name/size
 */

import { type CSSProperties } from "react";

// ── Types ───────────────────────────────────────────────────────────────────

export interface ContainerFieldProps {
  /** Blob hash from VFS. */
  blobHash?: string;
  /** MIME type of the file. */
  mimeType?: string;
  /** File name for display. */
  fileName?: string;
  /** File size in bytes. */
  fileSize?: number;
  /** Blob URL for rendering (created from VFS blob). */
  blobUrl?: string;
  /** Render mode: 'preview' shows inline content, 'icon' shows file icon. */
  renderMode?: "preview" | "icon";
  /** Container width. */
  width?: number;
  /** Container height. */
  height?: number;
  /** Click handler for opening file manager. */
  onSelect?: () => void;
  /** Drop handler for file upload. */
  onDrop?: (file: File) => void;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles: Record<string, CSSProperties> = {
  container: {
    border: "1px dashed #555",
    borderRadius: 4,
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    overflow: "hidden",
    cursor: "pointer",
    background: "#1e1e1e",
    position: "relative",
    minHeight: 60,
  },
  image: {
    maxWidth: "100%",
    maxHeight: "100%",
    objectFit: "contain",
  },
  media: {
    maxWidth: "100%",
  },
  placeholder: {
    color: "#888",
    fontSize: 12,
    textAlign: "center",
    padding: 8,
  },
  fileName: {
    color: "#ccc",
    fontSize: 11,
    marginTop: 4,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    maxWidth: "100%",
    padding: "0 4px",
  },
  fileSize: {
    color: "#666",
    fontSize: 10,
  },
  icon: {
    fontSize: 24,
    marginBottom: 4,
  },
};

// ── MIME helpers ─────────────────────────────────────────────────────────────

function mimeCategory(mimeType: string): "image" | "audio" | "video" | "pdf" | "other" {
  if (mimeType.startsWith("image/")) return "image";
  if (mimeType.startsWith("audio/")) return "audio";
  if (mimeType.startsWith("video/")) return "video";
  if (mimeType === "application/pdf") return "pdf";
  return "other";
}

function fileIcon(mimeType: string): string {
  const cat = mimeCategory(mimeType);
  switch (cat) {
    case "image": return "\u{1F5BC}";  // framed picture
    case "audio": return "\u{1F3B5}";  // musical note
    case "video": return "\u{1F3AC}";  // clapper board
    case "pdf": return "\u{1F4C4}";    // page facing up
    default: return "\u{1F4CE}";       // paperclip
  }
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ── Component ───────────────────────────────────────────────────────────────

export function ContainerFieldRenderer(props: ContainerFieldProps) {
  const {
    blobHash,
    mimeType = "application/octet-stream",
    fileName,
    fileSize,
    blobUrl,
    renderMode = "preview",
    width,
    height,
    onSelect,
    onDrop,
  } = props;

  const containerStyle: CSSProperties = {
    ...styles.container,
    ...(width ? { width } : {}),
    ...(height ? { height } : {}),
  };

  // Empty state
  if (!blobHash && !blobUrl) {
    return (
      <div
        style={containerStyle}
        onClick={onSelect}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          const file = e.dataTransfer.files[0];
          if (file && onDrop) onDrop(file);
        }}
      >
        <div style={styles.placeholder}>
          <div style={styles.icon}>{"\u{1F4CE}"}</div>
          <div>Click or drop a file</div>
        </div>
      </div>
    );
  }

  // Icon mode — just show icon + metadata
  if (renderMode === "icon") {
    return (
      <div style={containerStyle} onClick={onSelect}>
        <div style={styles.icon}>{fileIcon(mimeType)}</div>
        {fileName && <div style={styles.fileName}>{fileName}</div>}
        {fileSize !== undefined && <div style={styles.fileSize}>{formatFileSize(fileSize)}</div>}
      </div>
    );
  }

  // Preview mode — render based on MIME category
  const cat = mimeCategory(mimeType);

  if (cat === "image" && blobUrl) {
    return (
      <div style={containerStyle} onClick={onSelect}>
        <img
          src={blobUrl}
          alt={fileName ?? "Image"}
          style={styles.image}
          loading="lazy"
        />
      </div>
    );
  }

  if (cat === "audio" && blobUrl) {
    return (
      <div style={containerStyle}>
        <audio controls style={styles.media} src={blobUrl} />
        {fileName && <div style={styles.fileName}>{fileName}</div>}
      </div>
    );
  }

  if (cat === "video" && blobUrl) {
    return (
      <div style={containerStyle}>
        <video controls style={{ ...styles.media, maxHeight: height ?? 200 }} src={blobUrl} />
      </div>
    );
  }

  if (cat === "pdf" && blobUrl) {
    return (
      <div style={containerStyle}>
        <object
          data={blobUrl}
          type="application/pdf"
          style={{ width: "100%", height: height ?? 200 }}
        >
          <div style={styles.placeholder}>PDF Preview</div>
        </object>
      </div>
    );
  }

  // Fallback: icon view
  return (
    <div style={containerStyle} onClick={onSelect}>
      <div style={styles.icon}>{fileIcon(mimeType)}</div>
      {fileName && <div style={styles.fileName}>{fileName}</div>}
      {fileSize !== undefined && <div style={styles.fileSize}>{formatFileSize(fileSize)}</div>}
    </div>
  );
}
