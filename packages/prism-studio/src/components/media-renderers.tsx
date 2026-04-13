/**
 * Media renderers — video and audio player widgets for the page builder.
 *
 * Sources may be http(s) URLs or `vfs://<hash>` references stored in the
 * kernel's VFS (see `vfs-media-url.ts`). The allow-list rejects other
 * schemes so authors can't embed `javascript:` or arbitrary `data:` URLs.
 */

import type { CSSProperties } from "react";
import { useKernel } from "../kernel/index.js";
import { isVfsMediaUrl, useResolvedMediaUrl } from "./vfs-media-url.js";

/** Allow-list for media URLs. http(s) or vfs://. Blank returns false. */
export function isSafeMediaUrl(input: string | undefined | null): boolean {
  if (!input || typeof input !== "string") return false;
  const trimmed = input.trim();
  if (trimmed === "") return false;
  if (isVfsMediaUrl(trimmed)) return true;
  try {
    const url = new URL(trimmed, "https://placeholder.local");
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/** Clamp numeric pixel values into a safe UI range. */
export function clampPx(value: unknown, min: number, max: number, fallback: number): number {
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return fallback;
  if (n < min) return min;
  if (n > max) return max;
  return Math.floor(n);
}

// ── Video ──────────────────────────────────────────────────────────────────

export interface VideoWidgetProps {
  src?: string | undefined;
  poster?: string | undefined;
  caption?: string | undefined;
  width?: number | undefined;
  height?: number | undefined;
  controls?: boolean | undefined;
  autoplay?: boolean | undefined;
  loop?: boolean | undefined;
  muted?: boolean | undefined;
}

const figureStyle: CSSProperties = { margin: "0 0 8px 0" };

const captionStyle: CSSProperties = {
  marginTop: 6,
  fontSize: 12,
  color: "#64748b",
  fontStyle: "italic",
  textAlign: "center",
};

const placeholderStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: 180,
  borderRadius: 6,
  border: "1px dashed #94a3b8",
  background: "#f8fafc",
  color: "#94a3b8",
  fontSize: 12,
  padding: 12,
};

export function VideoWidgetRenderer(props: VideoWidgetProps) {
  const {
    src,
    poster,
    caption,
    width,
    height,
    controls = true,
    autoplay = false,
    loop = false,
    muted = false,
  } = props;

  const kernel = useKernel();
  const { url: resolvedSrc, loading } = useResolvedMediaUrl(src ?? null, kernel.vfs);
  const { url: resolvedPoster } = useResolvedMediaUrl(poster ?? null, kernel.vfs);

  if (!isSafeMediaUrl(src)) {
    return (
      <div data-testid="video-widget-empty" style={placeholderStyle}>
        Set a video URL (http/https) or choose a vault file to preview.
      </div>
    );
  }

  if (loading && !resolvedSrc) {
    return (
      <div data-testid="video-widget-loading" style={placeholderStyle}>
        Loading video…
      </div>
    );
  }

  if (!resolvedSrc) {
    return (
      <div data-testid="video-widget-missing" style={placeholderStyle}>
        Video source could not be resolved.
      </div>
    );
  }

  const w = width !== undefined ? clampPx(width, 80, 4096, 640) : undefined;
  const h = height !== undefined ? clampPx(height, 60, 4096, 360) : undefined;

  const videoProps: {
    style: CSSProperties;
    controls: boolean;
    autoPlay: boolean;
    loop: boolean;
    muted: boolean;
    width?: number;
    height?: number;
    poster?: string;
  } = {
    style: {
      width: "100%",
      maxWidth: w ? `${w}px` : "100%",
      height: h ? `${h}px` : "auto",
      borderRadius: 6,
      background: "#000",
      display: "block",
    },
    controls,
    autoPlay: autoplay,
    loop,
    muted: muted || autoplay, // browsers require muted to autoplay
  };
  if (w !== undefined) videoProps.width = w;
  if (h !== undefined) videoProps.height = h;
  if (resolvedPoster) videoProps.poster = resolvedPoster;

  return (
    <figure data-testid="video-widget" style={figureStyle}>
      <video {...videoProps}>
        <source src={resolvedSrc} />
        Your browser does not support the video element.
      </video>
      {caption && (
        <figcaption data-testid="video-widget-caption" style={captionStyle}>
          {caption}
        </figcaption>
      )}
    </figure>
  );
}

// ── Audio ──────────────────────────────────────────────────────────────────

export interface AudioWidgetProps {
  src?: string | undefined;
  caption?: string | undefined;
  controls?: boolean | undefined;
  autoplay?: boolean | undefined;
  loop?: boolean | undefined;
  muted?: boolean | undefined;
}

export function AudioWidgetRenderer(props: AudioWidgetProps) {
  const {
    src,
    caption,
    controls = true,
    autoplay = false,
    loop = false,
    muted = false,
  } = props;

  const kernel = useKernel();
  const { url: resolvedSrc, loading } = useResolvedMediaUrl(src ?? null, kernel.vfs);

  if (!isSafeMediaUrl(src)) {
    return (
      <div data-testid="audio-widget-empty" style={placeholderStyle}>
        Set an audio URL (http/https) or choose a vault file to preview.
      </div>
    );
  }

  if (loading && !resolvedSrc) {
    return (
      <div data-testid="audio-widget-loading" style={placeholderStyle}>
        Loading audio…
      </div>
    );
  }

  if (!resolvedSrc) {
    return (
      <div data-testid="audio-widget-missing" style={placeholderStyle}>
        Audio source could not be resolved.
      </div>
    );
  }

  return (
    <figure data-testid="audio-widget" style={figureStyle}>
      <audio
        style={{ width: "100%", display: "block" }}
        controls={controls}
        autoPlay={autoplay}
        loop={loop}
        muted={muted || autoplay}
      >
        <source src={resolvedSrc} />
        Your browser does not support the audio element.
      </audio>
      {caption && (
        <figcaption data-testid="audio-widget-caption" style={captionStyle}>
          {caption}
        </figcaption>
      )}
    </figure>
  );
}
