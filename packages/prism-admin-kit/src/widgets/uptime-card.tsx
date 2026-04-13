import { useAdminSnapshot } from "../admin-context.js";
import { formatUptime } from "../admin-helpers.js";
import { widgetStyles } from "./styles.js";

export interface UptimeCardProps {
  label?: string;
}

export function UptimeCard({ label }: UptimeCardProps) {
  const snapshot = useAdminSnapshot();
  return (
    <div style={widgetStyles.card} data-testid="uptime-card">
      <div style={widgetStyles.cardTitle}>{label ?? "Uptime"}</div>
      <div style={widgetStyles.metricValue}>{formatUptime(snapshot.uptimeSeconds)}</div>
      <div style={widgetStyles.metricHint}>
        Captured {new Date(snapshot.capturedAt).toLocaleTimeString()}
      </div>
    </div>
  );
}
