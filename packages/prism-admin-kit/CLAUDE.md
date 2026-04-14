# @prism/admin-kit

Puck-native admin dashboard components for Prism runtimes. Provides the shared admin infrastructure used by Studio, Relay, and Daemon.

## Build & Test
- `pnpm typecheck`
- `npx vitest run` — unit tests (9 test files)

## Architecture

### Data Model
- `AdminSnapshot` — normalised runtime state (health, uptime, metrics, services, activity)
- `AdminDataSource` — interface: `snapshot()`, optional `subscribe()`, optional `dispose()`
- `emptySnapshot()` — zero-value seed for initial renders

### Data Sources (`./data-sources`)
- `createKernelDataSource(kernel, options)` — projects a StudioKernel-shaped object into AdminSnapshot. Accepts structural `KernelAdminTarget` (not a concrete import) to avoid circular deps. Subscribes to PrismBus events for live activity feed.
- `createRelayDataSource({ url })` — HTTP polling of `/api/health`, `/api/modules`, `/metrics` (Prometheus). Falls back gracefully when endpoints are unavailable.
- `createDaemonDataSource({ url })` — HTTP to daemon's axum transport. Tries `POST /invoke/daemon.admin` first, falls back to `/healthz` + `/capabilities`.
- `parsePrometheus(text)` / `findSample(samples, name)` — Prometheus text format parser for relay metrics scraping.

### React Widgets (`./widgets`)
Seven composable Puck components that read live data via `useAdminSnapshot()`:
- `SourceHeader` — runtime label + live source indicator
- `HealthBadge` — colored health status pill (ok/warn/error/unknown)
- `MetricCard` — single KPI with value, unit, delta, hint
- `MetricChart` — recharts line/bar chart of a metric over time
- `ServiceList` — list of services with health dots
- `ActivityTail` — reverse-chronological event feed
- `UptimeCard` — formatted uptime display

### Puck Config (`./puck`)
- `createAdminPuckConfig()` — registers all widgets as Puck components with fields
- `createDefaultAdminLayout()` — seed Puck `Data` for a sensible default dashboard

### HTML Renderer (`./html`)
- `renderAdminHtml(options)` — generates a self-contained HTML page with inline CSS/JS that polls a JSON endpoint and renders the dashboard. Used by Relay and Daemon to serve `/admin`.
- `renderSnapshotBody(snap, now)` — server-side HTML fragment renderer
- Individual widget renderers: `renderHealthBadge`, `renderMetricCard`, `renderUptimeCard`, `renderServiceList`, `renderActivityTail`

### React Context (`./admin-context`)
- `AdminProvider` — wraps children with data source + polling/subscription
- `useAdminContext()` — full context (source, snapshot, refresh, loading, error)
- `useAdminSnapshot()` — just the current snapshot

### Helpers (`./admin-helpers`)
- `formatUptime`, `formatBytes`, `formatMetricValue`, `formatRelativeTime`
- `HEALTH_COLORS` — palette for ok/warn/error/unknown
- `rollupHealth` — fold service healths into overall level

## Exports
- `.` — main barrel (types, context, data sources, widgets, helpers)
- `./widgets` — widget components only
- `./data-sources` — data source factories only
- `./puck` — Puck config factory
- `./html` — HTML renderer for server embedding
