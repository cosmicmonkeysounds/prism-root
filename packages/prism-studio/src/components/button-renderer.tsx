/**
 * Button renderer — real interactive button preview for the Puck builder.
 *
 * Renders an actual `<button>` when no href is set, or an `<a>` when one is —
 * not the generic dashed-border placeholder the other schema-driven widgets
 * fall back to. Supports the extended set of button fields declared in
 * `entities.ts` (variant, size, icons, full-width, disabled, loading, rounded,
 * shadow, hoverEffect, target/rel, button type, aria label) so authors get a
 * live preview of how the button will look in their page.
 *
 * Everything that can be computed without JSX lives as a pure helper
 * (`resolveVariant`, `resolveSize`, `buildButtonStyles`, etc.) so the helpers
 * are unit-testable in a node environment without React DOM.
 */

import { useState, type CSSProperties, type ReactNode } from "react";

// ── Types ──────────────────────────────────────────────────────────────────

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "ghost"
  | "danger"
  | "success"
  | "gradient";

export type ButtonSize = "xs" | "sm" | "md" | "lg" | "xl";

export type ButtonRounded = "none" | "sm" | "md" | "lg" | "full";

export type ButtonShadow = "none" | "sm" | "md" | "lg";

export type ButtonHoverEffect = "none" | "lift" | "glow" | "scale";

export type ButtonIconPosition = "left" | "right";

export type ButtonTarget = "_self" | "_blank" | "_parent" | "_top";

export type ButtonType = "button" | "submit" | "reset";

export interface ButtonRendererProps {
  label?: string | undefined;
  href?: string | undefined;
  variant?: ButtonVariant | undefined;
  size?: ButtonSize | undefined;
  icon?: string | undefined;
  iconPosition?: ButtonIconPosition | undefined;
  fullWidth?: boolean | undefined;
  disabled?: boolean | undefined;
  loading?: boolean | undefined;
  rounded?: ButtonRounded | undefined;
  shadow?: ButtonShadow | undefined;
  hoverEffect?: ButtonHoverEffect | undefined;
  target?: ButtonTarget | undefined;
  rel?: string | undefined;
  buttonType?: ButtonType | undefined;
  ariaLabel?: string | undefined;
}

// ── Pure style helpers ─────────────────────────────────────────────────────

export interface VariantPalette {
  background: string;
  color: string;
  borderColor: string;
  hoverBackground: string;
  hoverColor: string;
}

export function resolveVariant(variant: ButtonVariant | undefined): VariantPalette {
  switch (variant) {
    case "secondary":
      return {
        background: "#e2e8f0",
        color: "#0f172a",
        borderColor: "#cbd5e1",
        hoverBackground: "#cbd5e1",
        hoverColor: "#0f172a",
      };
    case "outline":
      return {
        background: "transparent",
        color: "#6366f1",
        borderColor: "#6366f1",
        hoverBackground: "#eef2ff",
        hoverColor: "#4f46e5",
      };
    case "ghost":
      return {
        background: "transparent",
        color: "#475569",
        borderColor: "transparent",
        hoverBackground: "#f1f5f9",
        hoverColor: "#0f172a",
      };
    case "danger":
      return {
        background: "#dc2626",
        color: "#ffffff",
        borderColor: "#dc2626",
        hoverBackground: "#b91c1c",
        hoverColor: "#ffffff",
      };
    case "success":
      return {
        background: "#16a34a",
        color: "#ffffff",
        borderColor: "#16a34a",
        hoverBackground: "#15803d",
        hoverColor: "#ffffff",
      };
    case "gradient":
      return {
        background: "linear-gradient(135deg, #6366f1 0%, #ec4899 100%)",
        color: "#ffffff",
        borderColor: "transparent",
        hoverBackground: "linear-gradient(135deg, #4f46e5 0%, #db2777 100%)",
        hoverColor: "#ffffff",
      };
    case "primary":
    default:
      return {
        background: "#6366f1",
        color: "#ffffff",
        borderColor: "#6366f1",
        hoverBackground: "#4f46e5",
        hoverColor: "#ffffff",
      };
  }
}

export interface SizeTokens {
  paddingY: number;
  paddingX: number;
  fontSize: number;
  iconSize: number;
  gap: number;
}

export function resolveSize(size: ButtonSize | undefined): SizeTokens {
  switch (size) {
    case "xs":
      return { paddingY: 4, paddingX: 8, fontSize: 11, iconSize: 12, gap: 4 };
    case "sm":
      return { paddingY: 6, paddingX: 12, fontSize: 13, iconSize: 14, gap: 6 };
    case "lg":
      return { paddingY: 12, paddingX: 22, fontSize: 16, iconSize: 20, gap: 10 };
    case "xl":
      return { paddingY: 16, paddingX: 28, fontSize: 18, iconSize: 22, gap: 12 };
    case "md":
    default:
      return { paddingY: 9, paddingX: 16, fontSize: 14, iconSize: 16, gap: 8 };
  }
}

export function resolveRadius(rounded: ButtonRounded | undefined): number | string {
  switch (rounded) {
    case "none":
      return 0;
    case "sm":
      return 4;
    case "lg":
      return 12;
    case "full":
      return 9999;
    case "md":
    default:
      return 6;
  }
}

export function resolveShadow(
  shadow: ButtonShadow | undefined,
  hovered: boolean,
  hoverEffect: ButtonHoverEffect | undefined,
): string | undefined {
  const base = (() => {
    switch (shadow) {
      case "sm":
        return "0 1px 2px rgba(15, 23, 42, 0.08)";
      case "md":
        return "0 2px 6px rgba(15, 23, 42, 0.12)";
      case "lg":
        return "0 6px 18px rgba(15, 23, 42, 0.18)";
      case "none":
      default:
        return undefined;
    }
  })();
  if (hovered && hoverEffect === "glow") {
    return "0 0 0 4px rgba(99, 102, 241, 0.25), 0 6px 20px rgba(99, 102, 241, 0.35)";
  }
  if (hovered && hoverEffect === "lift") {
    return base
      ? `${base}, 0 10px 22px rgba(15, 23, 42, 0.18)`
      : "0 10px 22px rgba(15, 23, 42, 0.18)";
  }
  return base;
}

export function resolveTransform(
  hovered: boolean,
  hoverEffect: ButtonHoverEffect | undefined,
): string | undefined {
  if (!hovered) return undefined;
  switch (hoverEffect) {
    case "lift":
      return "translateY(-2px)";
    case "scale":
      return "scale(1.04)";
    default:
      return undefined;
  }
}

export interface ButtonStylesInput {
  variant?: ButtonVariant | undefined;
  size?: ButtonSize | undefined;
  rounded?: ButtonRounded | undefined;
  shadow?: ButtonShadow | undefined;
  hoverEffect?: ButtonHoverEffect | undefined;
  fullWidth?: boolean | undefined;
  disabled?: boolean | undefined;
  hovered: boolean;
}

export function buildButtonStyles(input: ButtonStylesInput): CSSProperties {
  const palette = resolveVariant(input.variant);
  const tokens = resolveSize(input.size);
  const radius = resolveRadius(input.rounded);
  const hovered = input.hovered && !input.disabled;

  const background = hovered ? palette.hoverBackground : palette.background;
  const color = hovered ? palette.hoverColor : palette.color;
  const boxShadow = resolveShadow(input.shadow, hovered, input.hoverEffect);
  const transform = resolveTransform(hovered, input.hoverEffect);

  const style: CSSProperties = {
    display: input.fullWidth ? "flex" : "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: tokens.gap,
    padding: `${tokens.paddingY}px ${tokens.paddingX}px`,
    fontSize: tokens.fontSize,
    fontWeight: 600,
    lineHeight: 1.2,
    background,
    color,
    border: `1px solid ${palette.borderColor}`,
    borderRadius: radius,
    cursor: input.disabled ? "not-allowed" : "pointer",
    opacity: input.disabled ? 0.55 : 1,
    textDecoration: "none",
    whiteSpace: "nowrap",
    fontFamily: "inherit",
    width: input.fullWidth ? "100%" : undefined,
    transition: "background 120ms ease, color 120ms ease, box-shadow 160ms ease, transform 160ms ease",
    userSelect: "none",
  };
  if (boxShadow !== undefined) style.boxShadow = boxShadow;
  if (transform !== undefined) style.transform = transform;
  return style;
}

/** Default `rel` filled in for external targets when the author hasn't set one. */
export function resolveRel(
  target: ButtonTarget | undefined,
  rel: string | undefined,
): string | undefined {
  if (rel && rel.trim().length > 0) return rel;
  if (target === "_blank") return "noopener noreferrer";
  return undefined;
}

// ── Spinner ────────────────────────────────────────────────────────────────

function Spinner({ size }: { size: number }) {
  return (
    <span
      aria-hidden
      style={{
        display: "inline-block",
        width: size,
        height: size,
        border: "2px solid currentColor",
        borderRightColor: "transparent",
        borderRadius: "50%",
        animation: "prism-button-spin 0.7s linear infinite",
      }}
    />
  );
}

const SPIN_KEYFRAMES = `@keyframes prism-button-spin { to { transform: rotate(360deg); } }`;

// ── Component ──────────────────────────────────────────────────────────────

/**
 * Visual button for the Puck canvas. Renders an `<a>` when `href` is set so
 * authors see link semantics, otherwise a real `<button>`. Clicks are
 * prevented inside the preview so a misdirected click doesn't navigate the
 * editor away — the render is for layout, not interaction.
 */
export function ButtonRenderer(props: ButtonRendererProps) {
  const {
    label = "Button",
    href,
    variant = "primary",
    size = "md",
    icon,
    iconPosition = "left",
    fullWidth = false,
    disabled = false,
    loading = false,
    rounded = "md",
    shadow = "none",
    hoverEffect = "none",
    target,
    rel,
    buttonType = "button",
    ariaLabel,
  } = props;

  const [hovered, setHovered] = useState(false);

  const style = buildButtonStyles({
    variant,
    size,
    rounded,
    shadow,
    hoverEffect,
    fullWidth,
    disabled: disabled || loading,
    hovered,
  });

  const tokens = resolveSize(size);
  const iconNode: ReactNode = icon ? (
    <span
      aria-hidden
      style={{
        fontSize: tokens.iconSize,
        lineHeight: 1,
        display: "inline-flex",
        alignItems: "center",
      }}
    >
      {icon}
    </span>
  ) : null;

  const body: ReactNode = (
    <>
      <style>{SPIN_KEYFRAMES}</style>
      {loading ? <Spinner size={tokens.iconSize} /> : null}
      {!loading && iconNode && iconPosition === "left" ? iconNode : null}
      <span>{label || "Button"}</span>
      {!loading && iconNode && iconPosition === "right" ? iconNode : null}
    </>
  );

  const commonProps = {
    style,
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
    "data-testid": "puck-button",
    ...(ariaLabel ? { "aria-label": ariaLabel } : {}),
  } as const;

  if (href && !disabled && !loading) {
    const resolvedRel = resolveRel(target, rel);
    return (
      <a
        href={href}
        {...(target ? { target } : {})}
        {...(resolvedRel ? { rel: resolvedRel } : {})}
        onClick={(e) => e.preventDefault()}
        {...commonProps}
      >
        {body}
      </a>
    );
  }

  return (
    <button
      type={buttonType}
      disabled={disabled || loading}
      onClick={(e) => e.preventDefault()}
      {...commonProps}
    >
      {body}
    </button>
  );
}
