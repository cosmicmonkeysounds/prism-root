/**
 * Card renderer — real card preview for the Puck builder.
 *
 * The card entity previously fell through to the generic dashed-border
 * preview chip. This component draws an actual card with variant-aware
 * elevation, configurable layout (vertical / horizontal / overlay), hover
 * lift, and an optional CTA button — so authors see what their page will
 * actually look like as they drag cards around.
 *
 * Pure helpers (`resolveCardVariant`, `resolveCardLayout`, `buildCardStyles`)
 * are exported so tests can cover them without a React DOM.
 */

import { useState, type CSSProperties, type ReactNode } from "react";
import { ButtonRenderer, type ButtonVariant } from "./button-renderer.js";

// ── Types ──────────────────────────────────────────────────────────────────

export type CardVariant = "elevated" | "outlined" | "filled" | "ghost";

export type CardLayout = "vertical" | "horizontal" | "overlay";

export type CardHoverEffect = "none" | "lift" | "glow";

export type CardMediaFit = "cover" | "contain";

export interface CardRendererProps {
  title?: string | undefined;
  body?: string | undefined;
  imageUrl?: string | undefined;
  linkUrl?: string | undefined;
  variant?: CardVariant | undefined;
  layout?: CardLayout | undefined;
  hoverEffect?: CardHoverEffect | undefined;
  mediaFit?: CardMediaFit | undefined;
  mediaAspectRatio?: string | undefined;
  eyebrow?: string | undefined;
  ctaLabel?: string | undefined;
  ctaVariant?: ButtonVariant | undefined;
  overlayOpacity?: number | undefined;
}

// ── Pure style helpers ─────────────────────────────────────────────────────

export interface CardPalette {
  background: string;
  color: string;
  borderColor: string;
  shadow: string | undefined;
  hoverShadow: string | undefined;
}

export function resolveCardVariant(variant: CardVariant | undefined): CardPalette {
  switch (variant) {
    case "outlined":
      return {
        background: "#ffffff",
        color: "#0f172a",
        borderColor: "#e2e8f0",
        shadow: undefined,
        hoverShadow: "0 6px 18px rgba(15, 23, 42, 0.10)",
      };
    case "filled":
      return {
        background: "#f1f5f9",
        color: "#0f172a",
        borderColor: "#f1f5f9",
        shadow: undefined,
        hoverShadow: "0 6px 18px rgba(15, 23, 42, 0.08)",
      };
    case "ghost":
      return {
        background: "transparent",
        color: "#0f172a",
        borderColor: "transparent",
        shadow: undefined,
        hoverShadow: undefined,
      };
    case "elevated":
    default:
      return {
        background: "#ffffff",
        color: "#0f172a",
        borderColor: "#f1f5f9",
        shadow: "0 4px 14px rgba(15, 23, 42, 0.10)",
        hoverShadow: "0 12px 28px rgba(15, 23, 42, 0.16)",
      };
  }
}

export interface CardLayoutTokens {
  direction: "row" | "column";
  mediaBasis: string;
  contentPadding: number;
  gap: number;
}

export function resolveCardLayout(layout: CardLayout | undefined): CardLayoutTokens {
  switch (layout) {
    case "horizontal":
      return { direction: "row", mediaBasis: "40%", contentPadding: 16, gap: 0 };
    case "overlay":
      return { direction: "column", mediaBasis: "100%", contentPadding: 20, gap: 0 };
    case "vertical":
    default:
      return { direction: "column", mediaBasis: "100%", contentPadding: 16, gap: 0 };
  }
}

export function clampOverlayOpacity(value: number | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0.55;
  if (value < 0) return 0;
  if (value > 1) return 1;
  return value;
}

export interface CardStylesInput {
  variant?: CardVariant | undefined;
  layout?: CardLayout | undefined;
  hoverEffect?: CardHoverEffect | undefined;
  hovered: boolean;
}

export function buildCardStyles(input: CardStylesInput): CSSProperties {
  const palette = resolveCardVariant(input.variant);
  const tokens = resolveCardLayout(input.layout);
  const hovered = input.hovered;
  const effect = input.hoverEffect ?? "none";
  const boxShadow =
    hovered && effect !== "none"
      ? effect === "glow"
        ? "0 0 0 3px rgba(99, 102, 241, 0.18), 0 14px 36px rgba(99, 102, 241, 0.20)"
        : (palette.hoverShadow ?? palette.shadow)
      : palette.shadow;

  const style: CSSProperties = {
    display: "flex",
    flexDirection: tokens.direction,
    gap: tokens.gap,
    background: palette.background,
    color: palette.color,
    border: `1px solid ${palette.borderColor}`,
    borderRadius: 12,
    overflow: "hidden",
    fontFamily: "inherit",
    position: "relative",
    transition:
      "box-shadow 180ms ease, transform 180ms ease, border-color 180ms ease",
  };
  if (boxShadow !== undefined) style.boxShadow = boxShadow;
  if (hovered && effect === "lift") style.transform = "translateY(-3px)";
  return style;
}

// ── Component ──────────────────────────────────────────────────────────────

/**
 * Visual card for the Puck canvas. Draws a real layout with media, title,
 * body, and an optional CTA — reuses `ButtonRenderer` for the CTA so the
 * card's call-to-action inherits every button variant for free.
 */
export function CardRenderer(props: CardRendererProps) {
  const {
    title,
    body,
    imageUrl,
    linkUrl,
    variant = "elevated",
    layout = "vertical",
    hoverEffect = "lift",
    mediaFit = "cover",
    mediaAspectRatio,
    eyebrow,
    ctaLabel,
    ctaVariant = "primary",
    overlayOpacity,
  } = props;

  const [hovered, setHovered] = useState(false);
  const cardStyle = buildCardStyles({ variant, layout, hoverEffect, hovered });
  const tokens = resolveCardLayout(layout);
  const isOverlay = layout === "overlay";
  const hasMedia = typeof imageUrl === "string" && imageUrl.trim().length > 0;

  const mediaStyle: CSSProperties = isOverlay
    ? {
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        objectFit: mediaFit,
      }
    : {
        width: layout === "horizontal" ? tokens.mediaBasis : "100%",
        flex: layout === "horizontal" ? `0 0 ${tokens.mediaBasis}` : undefined,
        aspectRatio: mediaAspectRatio ?? (layout === "horizontal" ? undefined : "16 / 9"),
        objectFit: mediaFit,
        background: "#f1f5f9",
      };

  const overlayBg = `rgba(15, 23, 42, ${clampOverlayOpacity(overlayOpacity)})`;

  const contentStyle: CSSProperties = {
    padding: tokens.contentPadding,
    display: "flex",
    flexDirection: "column",
    gap: 8,
    flex: 1,
    ...(isOverlay
      ? {
          position: "relative",
          zIndex: 2,
          color: "#ffffff",
          minHeight: 200,
          background: `linear-gradient(180deg, transparent 0%, ${overlayBg} 60%, ${overlayBg} 100%)`,
          justifyContent: "flex-end",
        }
      : {}),
  };

  const titleColor = isOverlay ? "#ffffff" : "#0f172a";
  const bodyColor = isOverlay ? "rgba(255,255,255,0.88)" : "#475569";

  const content: ReactNode = (
    <div style={contentStyle} data-testid="card-content">
      {eyebrow ? (
        <div
          style={{
            fontSize: 11,
            fontWeight: 700,
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            color: isOverlay ? "rgba(255,255,255,0.75)" : "#6366f1",
          }}
        >
          {eyebrow}
        </div>
      ) : null}
      {title ? (
        <div style={{ fontSize: 18, fontWeight: 700, color: titleColor, lineHeight: 1.25 }}>
          {title}
        </div>
      ) : null}
      {body ? (
        <div style={{ fontSize: 14, color: bodyColor, lineHeight: 1.55 }}>{body}</div>
      ) : null}
      {ctaLabel ? (
        <div style={{ marginTop: 6 }}>
          <ButtonRenderer
            label={ctaLabel}
            variant={ctaVariant}
            {...(linkUrl ? { href: linkUrl } : {})}
            size="sm"
          />
        </div>
      ) : null}
    </div>
  );

  return (
    <div
      data-testid="puck-card"
      style={cardStyle}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {hasMedia ? (
        <img src={imageUrl} alt={title ?? ""} style={mediaStyle} />
      ) : null}
      {content}
    </div>
  );
}
