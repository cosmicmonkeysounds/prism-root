/**
 * MapWidgetRenderer — geographic scatter of objects with lat/lng fields.
 *
 * Simple SVG-based projection (no map tiles). Auto-fits bounds to markers
 * and projects lat/lng onto the viewBox. For a full tile-layer map, swap
 * this component body for react-leaflet once the dependency is accepted.
 */

import { useMemo } from "react";
import type { GraphObject } from "@prism/core/object-model";

export interface MapWidgetProps {
  objects: GraphObject[];
  latField: string;
  lngField: string;
  titleField: string;
  initialZoom?: number;
  width?: number;
  height?: number;
  onSelectObject?: (id: string) => void;
}

export interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  title: string;
}

/** Extract valid lat/lng markers from a set of objects. */
export function extractMarkers(
  objects: GraphObject[],
  latField: string,
  lngField: string,
  titleField: string,
): MapMarker[] {
  const out: MapMarker[] = [];
  for (const obj of objects) {
    const data = obj.data as Record<string, unknown>;
    const lat = Number(data[latField]);
    const lng = Number(data[lngField]);
    if (!Number.isFinite(lat) || !Number.isFinite(lng)) continue;
    if (lat < -90 || lat > 90 || lng < -180 || lng > 180) continue;
    out.push({
      id: obj.id,
      lat,
      lng,
      title: String(data[titleField] ?? obj.id),
    });
  }
  return out;
}

/** Compute auto-bounds with at least a small pad around the marker set. */
export function computeBounds(markers: MapMarker[]): {
  minLat: number;
  maxLat: number;
  minLng: number;
  maxLng: number;
} {
  if (markers.length === 0) return { minLat: -90, maxLat: 90, minLng: -180, maxLng: 180 };
  let minLat = Infinity, maxLat = -Infinity, minLng = Infinity, maxLng = -Infinity;
  for (const m of markers) {
    if (m.lat < minLat) minLat = m.lat;
    if (m.lat > maxLat) maxLat = m.lat;
    if (m.lng < minLng) minLng = m.lng;
    if (m.lng > maxLng) maxLng = m.lng;
  }
  // Pad bounds slightly so a single marker still has context.
  const padLat = Math.max((maxLat - minLat) * 0.1, 0.5);
  const padLng = Math.max((maxLng - minLng) * 0.1, 0.5);
  return {
    minLat: minLat - padLat,
    maxLat: maxLat + padLat,
    minLng: minLng - padLng,
    maxLng: maxLng + padLng,
  };
}

export function MapWidgetRenderer(props: MapWidgetProps) {
  const { objects, latField, lngField, titleField, width = 480, height = 280, onSelectObject } = props;

  const markers = useMemo(
    () => extractMarkers(objects, latField, lngField, titleField),
    [objects, latField, lngField, titleField],
  );

  const bounds = useMemo(() => computeBounds(markers), [markers]);

  const projectX = (lng: number): number =>
    ((lng - bounds.minLng) / (bounds.maxLng - bounds.minLng || 1)) * width;
  const projectY = (lat: number): number =>
    height - ((lat - bounds.minLat) / (bounds.maxLat - bounds.minLat || 1)) * height;

  return (
    <div
      data-testid="map-widget"
      style={{
        border: "1px solid #059669",
        borderRadius: 6,
        background: "#0f172a",
        padding: 8,
        color: "#e2e8f0",
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 600, color: "#059669", textTransform: "uppercase", marginBottom: 6 }}>
        Map — {markers.length} marker{markers.length === 1 ? "" : "s"}
      </div>
      {markers.length === 0 ? (
        <div style={{ padding: 24, color: "#94a3b8", fontSize: 12, textAlign: "center" }}>
          No objects with valid {latField}/{lngField} fields.
        </div>
      ) : (
        <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} data-testid="map-svg">
          <rect x={0} y={0} width={width} height={height} fill="#1e293b" />
          {/* Crosshair graticule for context */}
          <line x1={0} y1={height / 2} x2={width} y2={height / 2} stroke="#334155" strokeDasharray="2 4" />
          <line x1={width / 2} y1={0} x2={width / 2} y2={height} stroke="#334155" strokeDasharray="2 4" />
          {markers.map((m) => (
            <g
              key={m.id}
              data-testid={`map-marker-${m.id}`}
              onClick={() => onSelectObject?.(m.id)}
              style={{ cursor: onSelectObject ? "pointer" : "default" }}
            >
              <circle cx={projectX(m.lng)} cy={projectY(m.lat)} r={6} fill="#059669" stroke="#e2e8f0" strokeWidth={1} />
              <text
                x={projectX(m.lng) + 8}
                y={projectY(m.lat) + 4}
                fontSize="10"
                fill="#e2e8f0"
              >
                {m.title.length > 18 ? `${m.title.slice(0, 17)}…` : m.title}
              </text>
            </g>
          ))}
        </svg>
      )}
    </div>
  );
}
