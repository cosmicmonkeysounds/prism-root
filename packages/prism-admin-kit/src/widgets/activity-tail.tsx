import { useAdminSnapshot } from "../admin-context.js";
import { HEALTH_COLORS, formatRelativeTime } from "../admin-helpers.js";
import { palette, widgetStyles } from "./styles.js";

export interface ActivityTailProps {
  /** Max items to render. Default 10. */
  limit?: number;
  title?: string;
}

export function ActivityTail({ limit = 10, title }: ActivityTailProps) {
  const snapshot = useAdminSnapshot();
  const items = snapshot.activity.slice(0, limit);

  return (
    <div style={widgetStyles.card} data-testid="activity-tail">
      <div style={widgetStyles.cardTitle}>{title ?? "Recent Activity"}</div>
      {items.length === 0 ? (
        <div style={widgetStyles.empty}>Quiet. No recent activity.</div>
      ) : (
        <div>
          {items.map((item, idx) => {
            const colors = HEALTH_COLORS[item.level ?? "ok"];
            const rowStyle = idx === items.length - 1 ? widgetStyles.rowLast : widgetStyles.row;
            return (
              <div key={item.id} style={rowStyle} data-testid={`activity-row-${item.id}`}>
                <div style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
                  <span style={{ color: palette.textStrong, fontSize: "0.8125rem" }}>
                    {item.message}
                  </span>
                  <span style={{ fontSize: "0.625rem", color: colors.fg, fontFamily: "ui-monospace, monospace" }}>
                    {item.kind}
                  </span>
                </div>
                <span style={widgetStyles.timestamp}>{formatRelativeTime(item.timestamp)}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
