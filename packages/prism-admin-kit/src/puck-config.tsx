/**
 * createAdminPuckConfig — builds a Puck `Config` registering every admin-kit
 * widget as a composable component.
 *
 * Pure factory: the result is a `Config` object; no React is mounted until
 * Studio (or any other host) renders `<Puck config={…} />`. Widgets read
 * live data through `useAdminSnapshot`, so whatever `<AdminProvider>` wraps
 * the Puck tree determines which runtime they reflect.
 */

import type { Config, Fields } from "@measured/puck";
import {
  ActivityTail,
  HealthBadge,
  MetricCard,
  MetricChart,
  ServiceList,
  SourceHeader,
  UptimeCard,
} from "./widgets/index.js";

const textField = { type: "text" } as unknown as Fields[string];
const numberField = { type: "number" } as unknown as Fields[string];

const chartKindField = {
  type: "select",
  options: [
    { label: "Line", value: "line" },
    { label: "Bar", value: "bar" },
  ],
} as unknown as Fields[string];

const healthLevelField = {
  type: "select",
  options: [
    { label: "(from snapshot)", value: "" },
    { label: "ok", value: "ok" },
    { label: "warn", value: "warn" },
    { label: "error", value: "error" },
    { label: "unknown", value: "unknown" },
  ],
} as unknown as Fields[string];

const showSourceField = {
  type: "radio",
  options: [
    { label: "no", value: "false" },
    { label: "yes", value: "true" },
  ],
} as unknown as Fields[string];

export function createAdminPuckConfig(): Config {
  return {
    components: {
      SourceHeader: {
        fields: { title: textField },
        defaultProps: { title: "" },
        render: (props) => {
          const title = (props as { title?: string }).title;
          return <SourceHeader {...(title ? { title } : {})} />;
        },
      },

      HealthBadge: {
        fields: {
          label: textField,
          level: healthLevelField,
          showSource: showSourceField,
        },
        defaultProps: { label: "", level: "", showSource: "false" },
        render: (props) => {
          const p = props as { label?: string; level?: string; showSource?: string };
          return (
            <HealthBadge
              {...(p.label ? { label: p.label } : {})}
              {...(p.level ? { level: p.level as "ok" | "warn" | "error" | "unknown" } : {})}
              showSource={p.showSource === "true"}
            />
          );
        },
      },

      MetricCard: {
        fields: {
          metricId: textField,
          label: textField,
        },
        defaultProps: { metricId: "objects", label: "" },
        render: (props) => {
          const p = props as { metricId?: string; label?: string };
          return (
            <MetricCard
              metricId={p.metricId ?? ""}
              {...(p.label ? { label: p.label } : {})}
            />
          );
        },
      },

      MetricChart: {
        fields: {
          metricId: textField,
          kind: chartKindField,
          title: textField,
          window: numberField,
          height: numberField,
        },
        defaultProps: { metricId: "objects", kind: "line", title: "", window: 30, height: 140 },
        render: (props) => {
          const p = props as {
            metricId?: string;
            kind?: "bar" | "line";
            title?: string;
            window?: number;
            height?: number;
          };
          return (
            <MetricChart
              metricId={p.metricId ?? ""}
              kind={p.kind ?? "line"}
              {...(p.title ? { title: p.title } : {})}
              window={p.window ?? 30}
              height={p.height ?? 140}
            />
          );
        },
      },

      ServiceList: {
        fields: {
          title: textField,
          kind: textField,
          limit: numberField,
        },
        defaultProps: { title: "", kind: "", limit: 10 },
        render: (props) => {
          const p = props as { title?: string; kind?: string; limit?: number };
          return (
            <ServiceList
              {...(p.title ? { title: p.title } : {})}
              {...(p.kind ? { kind: p.kind } : {})}
              {...(typeof p.limit === "number" ? { limit: p.limit } : {})}
            />
          );
        },
      },

      ActivityTail: {
        fields: {
          title: textField,
          limit: numberField,
        },
        defaultProps: { title: "", limit: 10 },
        render: (props) => {
          const p = props as { title?: string; limit?: number };
          return (
            <ActivityTail
              {...(p.title ? { title: p.title } : {})}
              limit={typeof p.limit === "number" ? p.limit : 10}
            />
          );
        },
      },

      UptimeCard: {
        fields: { label: textField },
        defaultProps: { label: "" },
        render: (props) => {
          const label = (props as { label?: string }).label;
          return <UptimeCard {...(label ? { label } : {})} />;
        },
      },
    },

    categories: {
      summary: {
        title: "Summary",
        components: ["SourceHeader", "HealthBadge", "UptimeCard"],
      },
      metrics: {
        title: "Metrics",
        components: ["MetricCard", "MetricChart"],
      },
      lists: {
        title: "Lists",
        components: ["ServiceList", "ActivityTail"],
      },
    },
  };
}
