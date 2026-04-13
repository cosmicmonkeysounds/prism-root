/**
 * Default admin dashboard seed — Puck `Data` ready to drop into a `<Puck />`.
 *
 * Host apps (Studio's admin panel, puck-playground, etc.) can use this when
 * no custom layout has been saved yet. The layout is a flat root column of
 * widgets so it looks sensible on a vanilla Puck canvas without any shell
 * component wiring.
 */

import type { Data } from "@measured/puck";

let id = 0;
const pid = (prefix: string): string => `${prefix}-${++id}`;

export function createDefaultAdminLayout(): Data {
  return {
    root: { props: { title: "Admin Dashboard" } },
    content: [
      { type: "SourceHeader", props: { id: pid("header"), title: "" } },
      { type: "HealthBadge", props: { id: pid("health"), label: "", level: "", showSource: "false" } },
      { type: "UptimeCard", props: { id: pid("uptime"), label: "" } },
      { type: "MetricCard", props: { id: pid("metric"), metricId: "objects", label: "" } },
      { type: "MetricCard", props: { id: pid("metric"), metricId: "edges", label: "" } },
      { type: "MetricCard", props: { id: pid("metric"), metricId: "files", label: "" } },
      { type: "MetricCard", props: { id: pid("metric"), metricId: "peers", label: "" } },
      { type: "MetricCard", props: { id: pid("metric"), metricId: "relays", label: "" } },
      {
        type: "MetricChart",
        props: { id: pid("chart"), metricId: "objects", kind: "line", title: "", window: 30, height: 140 },
      },
      { type: "ServiceList", props: { id: pid("services"), title: "", kind: "", limit: 10 } },
      { type: "ActivityTail", props: { id: pid("activity"), title: "", limit: 10 } },
    ],
  };
}
