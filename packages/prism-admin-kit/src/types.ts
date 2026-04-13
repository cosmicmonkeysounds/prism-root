/**
 * @prism/admin-kit — core types.
 *
 * The admin kit is a Puck-native component library for admin panels.
 * Any "runtime that runs" (daemon, relay, studio kernel) exposes its state
 * through an AdminDataSource; widgets render from a normalised AdminSnapshot.
 */

/** Overall health rollup. */
export type HealthLevel = "ok" | "warn" | "error" | "unknown";

export interface HealthStatus {
  level: HealthLevel;
  /** Human-readable label. */
  label: string;
  /** Optional extra detail (shown on hover or in card body). */
  detail?: string;
}

/**
 * A single scalar metric (KPI). Numeric values render in MetricCard;
 * string values render as a literal label (e.g. "v0.1.0").
 */
export interface Metric {
  id: string;
  label: string;
  value: number | string;
  /** Optional unit suffix — "ms", "MB", "req/s", etc. */
  unit?: string;
  /** Optional hint string (e.g. "across 3 peers"). */
  hint?: string;
  /** Delta vs previous sample, if known. */
  delta?: number;
}

/**
 * A runnable service / module / actor the data source is reporting.
 * Maps to daemon modules, relay modules, kernel subsystems, etc.
 */
export interface Service {
  id: string;
  name: string;
  kind?: string;
  health: HealthLevel;
  /** Optional one-line status message. */
  status?: string;
}

/** A single item in the activity tail. */
export interface ActivityItem {
  id: string;
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** Short label — e.g. "object.created", "relay.connected". */
  kind: string;
  /** Human-readable message. */
  message: string;
  level?: HealthLevel;
}

/**
 * Immutable snapshot of a runtime at a point in time. Data sources
 * normalise whatever raw API they talk to into this shape.
 */
export interface AdminSnapshot {
  sourceId: string;
  sourceLabel: string;
  /** ISO-8601 time the snapshot was captured. */
  capturedAt: string;
  health: HealthStatus;
  /** Uptime in seconds. -1 when unknown. */
  uptimeSeconds: number;
  metrics: Metric[];
  services: Service[];
  activity: ActivityItem[];
}

/**
 * A live connection to a runtime being administered.
 *
 * `snapshot()` is the pull contract — the widget tree just reads this.
 * `subscribe()` is an optional push channel for sources that already have
 * a reactive store (kernel) or a WebSocket feed.
 */
export interface AdminDataSource {
  readonly id: string;
  readonly label: string;
  snapshot(): Promise<AdminSnapshot>;
  subscribe?(listener: (snapshot: AdminSnapshot) => void): () => void;
  dispose?(): void;
}

/** Empty snapshot used as an initial value before the first fetch. */
export function emptySnapshot(sourceId: string, sourceLabel: string): AdminSnapshot {
  return {
    sourceId,
    sourceLabel,
    capturedAt: new Date(0).toISOString(),
    health: { level: "unknown", label: "Unknown" },
    uptimeSeconds: -1,
    metrics: [],
    services: [],
    activity: [],
  };
}
