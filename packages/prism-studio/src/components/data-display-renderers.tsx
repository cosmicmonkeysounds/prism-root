/**
 * Data display renderers — stat, badge, alert, progress-bar widgets.
 *
 * StatWidget computes a single aggregate (count/sum/avg/min/max) over
 * kernel-provided objects — the same aggregation pipeline used by
 * chart-widget, but rendered as a KPI card instead of a chart.
 *
 * Badge and Alert are static presentational widgets with optional icons
 * and color tones. ProgressBar shows a numeric value against a max,
 * either as a literal number or a computed aggregate.
 */

import type { GraphObject } from "@prism/core/object-model";

export type StatAggregation = "count" | "sum" | "avg" | "min" | "max";

export interface StatWidgetProps {
  objects: GraphObject[];
  label?: string | undefined;
  /** Aggregation function. */
  aggregation: StatAggregation;
  /** Field to aggregate. Only used when aggregation !== "count". */
  valueField?: string | undefined;
  /** Optional prefix/suffix (e.g. "$", " pts"). */
  prefix?: string | undefined;
  suffix?: string | undefined;
  /** Format with thousands separators. */
  thousands?: boolean | undefined;
  /** Decimal places. */
  decimals?: number | undefined;
}

/** Compute a single aggregate over a set of objects. Missing values are skipped. */
export function computeStat(
  objects: GraphObject[],
  aggregation: StatAggregation,
  valueField: string | undefined,
): number {
  if (objects.length === 0) return 0;
  if (aggregation === "count" || !valueField) return objects.length;
  const nums: number[] = [];
  for (const obj of objects) {
    const data = obj.data as Record<string, unknown>;
    const raw = data[valueField];
    const n = typeof raw === "number" ? raw : Number(raw);
    if (Number.isFinite(n)) nums.push(n);
  }
  if (nums.length === 0) return 0;
  switch (aggregation) {
    case "sum":
      return nums.reduce((s, v) => s + v, 0);
    case "avg":
      return nums.reduce((s, v) => s + v, 0) / nums.length;
    case "min":
      return Math.min(...nums);
    case "max":
      return Math.max(...nums);
  }
}

/** Format a number with optional thousands separator and decimals. */
export function formatStatValue(value: number, thousands: boolean, decimals: number): string {
  const fixed = decimals > 0 ? value.toFixed(decimals) : Math.round(value).toString();
  if (!thousands) return fixed;
  const [int, frac] = fixed.split(".");
  const withCommas = (int ?? "").replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac ? `${withCommas}.${frac}` : withCommas;
}

export function StatWidgetRenderer(props: StatWidgetProps) {
  const {
    objects,
    label = "Total",
    aggregation,
    valueField,
    prefix = "",
    suffix = "",
    thousands = true,
    decimals = 0,
  } = props;
  const value = computeStat(objects, aggregation, valueField);
  const display = formatStatValue(value, thousands, decimals);
  return (
    <div
      data-testid="stat-widget"
      style={{
        border: "1px solid #0ea5e9",
        borderRadius: 8,
        padding: 16,
        background: "#f0f9ff",
        margin: "0 0 8px 0",
        minWidth: 160,
        display: "inline-flex",
        flexDirection: "column",
        gap: 4,
      }}
    >
      <div
        data-testid="stat-widget-label"
        style={{
          fontSize: 11,
          fontWeight: 600,
          color: "#0369a1",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
        }}
      >
        {label}
      </div>
      <div
        data-testid="stat-widget-value"
        style={{ fontSize: 28, fontWeight: 700, color: "#0f172a", lineHeight: 1.1 }}
      >
        {prefix}
        {display}
        {suffix}
      </div>
      <div style={{ fontSize: 11, color: "#64748b" }}>
        {aggregation === "count" ? `${objects.length} records` : `${aggregation} of ${valueField ?? "?"}`}
      </div>
    </div>
  );
}

// ── Badge ──────────────────────────────────────────────────────────────────

export type BadgeTone = "neutral" | "info" | "success" | "warning" | "danger";

export interface BadgeProps {
  label: string;
  tone?: BadgeTone | undefined;
  icon?: string | undefined;
  outline?: boolean | undefined;
}

const BADGE_COLORS: Record<BadgeTone, { bg: string; fg: string; border: string }> = {
  neutral: { bg: "#f1f5f9", fg: "#334155", border: "#cbd5e1" },
  info: { bg: "#dbeafe", fg: "#1e40af", border: "#93c5fd" },
  success: { bg: "#dcfce7", fg: "#166534", border: "#86efac" },
  warning: { bg: "#fef3c7", fg: "#92400e", border: "#fcd34d" },
  danger: { bg: "#fee2e2", fg: "#991b1b", border: "#fca5a5" },
};

export function BadgeRenderer(props: BadgeProps) {
  const { label, tone = "neutral", icon, outline = false } = props;
  const c = BADGE_COLORS[tone];
  return (
    <span
      data-testid="badge"
      data-tone={tone}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "2px 8px",
        borderRadius: 999,
        fontSize: 11,
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: "0.05em",
        background: outline ? "transparent" : c.bg,
        color: c.fg,
        border: `1px solid ${c.border}`,
        margin: "0 4px 4px 0",
      }}
    >
      {icon ? <span aria-hidden="true">{icon}</span> : null}
      {label}
    </span>
  );
}

// ── Alert / Callout ────────────────────────────────────────────────────────

export interface AlertProps {
  title?: string | undefined;
  message: string;
  tone?: BadgeTone | undefined;
  icon?: string | undefined;
}

const ALERT_ICONS: Record<BadgeTone, string> = {
  neutral: "\u2139",
  info: "\u2139",
  success: "\u2714",
  warning: "\u26A0",
  danger: "\u2716",
};

export function AlertRenderer(props: AlertProps) {
  const { title, message, tone = "info", icon } = props;
  const c = BADGE_COLORS[tone];
  const effectiveIcon = icon ?? ALERT_ICONS[tone];
  return (
    <div
      data-testid="alert"
      data-tone={tone}
      role="alert"
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 10,
        padding: "10px 12px",
        background: c.bg,
        border: `1px solid ${c.border}`,
        borderLeft: `4px solid ${c.border}`,
        borderRadius: 4,
        margin: "0 0 8px 0",
        color: c.fg,
      }}
    >
      <span aria-hidden="true" style={{ fontSize: 16, lineHeight: 1 }}>
        {effectiveIcon}
      </span>
      <div style={{ flex: 1, fontSize: 13 }}>
        {title ? <div style={{ fontWeight: 700, marginBottom: 2 }}>{title}</div> : null}
        <div>{message}</div>
      </div>
    </div>
  );
}

// ── Progress Bar ───────────────────────────────────────────────────────────

export interface ProgressBarProps {
  value: number;
  max?: number | undefined;
  label?: string | undefined;
  tone?: BadgeTone | undefined;
  showPercent?: boolean | undefined;
}

/** Clamp value/max and return a 0..1 ratio. */
export function progressRatio(value: number, max: number): number {
  if (!Number.isFinite(value) || !Number.isFinite(max) || max <= 0) return 0;
  return Math.max(0, Math.min(1, value / max));
}

export function ProgressBarRenderer(props: ProgressBarProps) {
  const { value, max = 100, label, tone = "info", showPercent = true } = props;
  const ratio = progressRatio(Number(value), Number(max));
  const pct = Math.round(ratio * 100);
  const c = BADGE_COLORS[tone];
  return (
    <div data-testid="progress-bar" style={{ margin: "0 0 8px 0" }}>
      {label || showPercent ? (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            fontSize: 11,
            color: "#64748b",
            marginBottom: 4,
          }}
        >
          <span>{label ?? ""}</span>
          {showPercent ? <span>{pct}%</span> : null}
        </div>
      ) : null}
      <div
        role="progressbar"
        aria-valuenow={value}
        aria-valuemin={0}
        aria-valuemax={max}
        style={{
          width: "100%",
          height: 8,
          background: "#e2e8f0",
          borderRadius: 999,
          overflow: "hidden",
        }}
      >
        <div
          data-testid="progress-bar-fill"
          style={{
            width: `${pct}%`,
            height: "100%",
            background: c.border,
            transition: "width 0.2s",
          }}
        />
      </div>
    </div>
  );
}
