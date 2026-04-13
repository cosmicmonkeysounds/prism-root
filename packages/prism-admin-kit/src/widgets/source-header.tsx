import { useAdminContext } from "../admin-context.js";
import { HEALTH_COLORS, formatUptime } from "../admin-helpers.js";
import { palette, widgetStyles } from "./styles.js";

export interface SourceHeaderProps {
  /** Optional title shown above the source name. */
  title?: string;
}

export function SourceHeader({ title }: SourceHeaderProps) {
  const { snapshot, loading, error, refresh } = useAdminContext();
  const colors = HEALTH_COLORS[snapshot.health.level];

  return (
    <div style={{ ...widgetStyles.card, background: palette.cardAlt }} data-testid="source-header">
      {title && <div style={widgetStyles.cardTitle}>{title}</div>}
      <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", flexWrap: "wrap" }}>
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
          {snapshot.health.level.toUpperCase()}
        </span>
        <div style={{ fontSize: "1rem", color: palette.textStrong, fontWeight: 600 }}>
          {snapshot.sourceLabel}
        </div>
        <div style={{ fontSize: "0.75rem", color: palette.textDim }}>
          uptime {formatUptime(snapshot.uptimeSeconds)}
        </div>
        <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 8 }}>
          {loading && <span style={{ fontSize: 11, color: palette.textDim }}>refreshing…</span>}
          <button
            type="button"
            onClick={() => refresh()}
            style={{
              background: palette.card,
              border: `1px solid ${palette.border}`,
              borderRadius: "0.25rem",
              color: palette.text,
              padding: "0.25rem 0.625rem",
              fontSize: "0.6875rem",
              cursor: "pointer",
            }}
            data-testid="source-header-refresh"
          >
            Refresh
          </button>
        </div>
      </div>
      {error && (
        <div style={{ fontSize: "0.75rem", color: HEALTH_COLORS.error.fg, marginTop: 6 }}>
          {error}
        </div>
      )}
    </div>
  );
}
