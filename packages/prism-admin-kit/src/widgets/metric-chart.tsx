import { useEffect, useRef, useState } from "react";
import {
  BarChart,
  Bar,
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  CartesianGrid,
  ResponsiveContainer,
} from "recharts";
import { useAdminSnapshot } from "../admin-context.js";
import { palette, widgetStyles } from "./styles.js";

export type MetricChartKind = "bar" | "line";

export interface MetricChartProps {
  /** Which numeric metric to track over time. */
  metricId: string;
  kind?: MetricChartKind;
  title?: string;
  /** Max data points to keep. Default 30. */
  window?: number;
  height?: number;
}

interface Point {
  t: string;
  v: number;
}

export function MetricChart({
  metricId,
  kind = "line",
  title,
  window = 30,
  height = 140,
}: MetricChartProps) {
  const snapshot = useAdminSnapshot();
  const [points, setPoints] = useState<Point[]>([]);
  const lastCapture = useRef<string>("");

  useEffect(() => {
    if (snapshot.capturedAt === lastCapture.current) return;
    lastCapture.current = snapshot.capturedAt;
    const metric = snapshot.metrics.find((m) => m.id === metricId);
    if (!metric || typeof metric.value !== "number") return;
    setPoints((prev) => {
      const next = [...prev, { t: snapshot.capturedAt, v: metric.value as number }];
      return next.length > window ? next.slice(next.length - window) : next;
    });
  }, [snapshot, metricId, window]);

  const metric = snapshot.metrics.find((m) => m.id === metricId);
  const heading = title ?? `${metric?.label ?? metricId} over time`;

  return (
    <div style={widgetStyles.card} data-testid={`metric-chart-${metricId}`}>
      <div style={widgetStyles.cardTitle}>{heading}</div>
      {points.length < 2 ? (
        <div style={{ ...widgetStyles.empty, padding: "1.25rem" }}>
          Collecting samples…
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={height}>
          {kind === "bar" ? (
            <BarChart data={points}>
              <CartesianGrid stroke={palette.border} strokeDasharray="3 3" />
              <XAxis dataKey="t" hide />
              <YAxis stroke={palette.textDim} fontSize={10} />
              <Tooltip
                contentStyle={{
                  background: palette.card,
                  border: `1px solid ${palette.border}`,
                  color: palette.text,
                  fontSize: 11,
                }}
              />
              <Bar dataKey="v" fill={palette.accent} />
            </BarChart>
          ) : (
            <LineChart data={points}>
              <CartesianGrid stroke={palette.border} strokeDasharray="3 3" />
              <XAxis dataKey="t" hide />
              <YAxis stroke={palette.textDim} fontSize={10} />
              <Tooltip
                contentStyle={{
                  background: palette.card,
                  border: `1px solid ${palette.border}`,
                  color: palette.text,
                  fontSize: 11,
                }}
              />
              <Line
                type="monotone"
                dataKey="v"
                stroke={palette.accentSoft}
                strokeWidth={2}
                dot={false}
              />
            </LineChart>
          )}
        </ResponsiveContainer>
      )}
    </div>
  );
}
