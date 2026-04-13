import { useAdminSnapshot } from "../admin-context.js";
import { HEALTH_COLORS } from "../admin-helpers.js";
import { palette, widgetStyles } from "./styles.js";

export interface ServiceListProps {
  /** Filter services by kind (e.g. "relay-module", "crdt"). */
  kind?: string;
  /** Override card title. */
  title?: string;
  /** Max rows to render. */
  limit?: number;
}

export function ServiceList({ kind, title, limit }: ServiceListProps) {
  const snapshot = useAdminSnapshot();
  const all = snapshot.services.filter((s) => !kind || s.kind === kind);
  const services = typeof limit === "number" ? all.slice(0, limit) : all;

  return (
    <div style={widgetStyles.card} data-testid="service-list">
      <div style={widgetStyles.cardTitle}>{title ?? "Services"}</div>
      {services.length === 0 ? (
        <div style={widgetStyles.empty}>No services registered</div>
      ) : (
        <div>
          {services.map((service, idx) => {
            const colors = HEALTH_COLORS[service.health];
            const rowStyle = idx === services.length - 1 ? widgetStyles.rowLast : widgetStyles.row;
            return (
              <div key={service.id} style={rowStyle} data-testid={`service-row-${service.id}`}>
                <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                  <span style={{ color: palette.textStrong, fontWeight: 500 }}>{service.name}</span>
                  {service.status && (
                    <span style={{ fontSize: "0.6875rem", color: palette.textDim }}>
                      {service.status}
                    </span>
                  )}
                </div>
                <span
                  style={{
                    ...widgetStyles.badge,
                    background: colors.bg,
                    color: colors.fg,
                    borderColor: colors.border,
                  }}
                >
                  {service.health}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
