import { useAdminSnapshot } from "../admin-context.js";
import { formatMetricValue } from "../admin-helpers.js";
import { palette, widgetStyles } from "./styles.js";

export interface MetricCardProps {
  /** Metric id to look up in the current snapshot. */
  metricId: string;
  /** Optional label override. Falls back to the metric's own label. */
  label?: string;
}

export function MetricCard({ metricId, label }: MetricCardProps) {
  const snapshot = useAdminSnapshot();
  const metric = snapshot.metrics.find((m) => m.id === metricId);

  if (!metric) {
    return (
      <div style={widgetStyles.card} data-testid={`metric-card-${metricId}`}>
        <div style={widgetStyles.cardTitle}>{label ?? metricId}</div>
        <div style={{ ...widgetStyles.metricValue, color: palette.textDimmer, fontSize: "1rem" }}>
          unavailable
        </div>
      </div>
    );
  }

  const delta = metric.delta;
  const deltaColor =
    delta === undefined || delta === 0
      ? palette.textDim
      : delta > 0
        ? "#4ade80"
        : "#f87171";

  return (
    <div style={widgetStyles.card} data-testid={`metric-card-${metricId}`}>
      <div style={widgetStyles.cardTitle}>{label ?? metric.label}</div>
      <div style={widgetStyles.metricValue}>{formatMetricValue(metric)}</div>
      {metric.hint && <div style={widgetStyles.metricHint}>{metric.hint}</div>}
      {delta !== undefined && delta !== 0 && (
        <div style={{ ...widgetStyles.metricHint, color: deltaColor }}>
          {delta > 0 ? "▲" : "▼"} {Math.abs(delta).toFixed(2)}
        </div>
      )}
    </div>
  );
}
