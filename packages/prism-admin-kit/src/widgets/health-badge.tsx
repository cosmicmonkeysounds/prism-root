import { useAdminSnapshot } from "../admin-context.js";
import { HEALTH_COLORS } from "../admin-helpers.js";
import type { HealthLevel } from "../types.js";
import { palette, widgetStyles } from "./styles.js";

export interface HealthBadgeProps {
  /** Override the label shown next to the dot. Defaults to snapshot health label. */
  label?: string;
  /** Override the level (otherwise read from admin snapshot). */
  level?: HealthLevel;
  /** Show the source label as the prefix ("Prism Kernel: Healthy"). */
  showSource?: boolean;
}

export function HealthBadge({ label, level, showSource = false }: HealthBadgeProps) {
  const snapshot = useAdminSnapshot();
  const resolvedLevel = level ?? snapshot.health.level;
  const resolvedLabel = label ?? snapshot.health.label;
  const colors = HEALTH_COLORS[resolvedLevel];

  return (
    <div style={widgetStyles.card} data-testid="health-badge">
      <div style={widgetStyles.cardTitle}>Health</div>
      <div style={{ display: "flex", alignItems: "center", gap: "0.75rem" }}>
        <span
          style={{
            ...widgetStyles.badge,
            background: colors.bg,
            color: colors.fg,
            borderColor: colors.border,
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: colors.fg,
              display: "inline-block",
            }}
          />
          {resolvedLevel.toUpperCase()}
        </span>
        <div style={{ fontSize: "0.9375rem", color: palette.textStrong }}>
          {showSource ? `${snapshot.sourceLabel}: ` : ""}
          {resolvedLabel}
        </div>
      </div>
      {snapshot.health.detail && (
        <div style={widgetStyles.metricHint}>{snapshot.health.detail}</div>
      )}
    </div>
  );
}
