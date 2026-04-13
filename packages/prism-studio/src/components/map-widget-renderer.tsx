/**
 * MapWidgetRenderer — geographic scatter of objects with lat/lng fields,
 * powered by react-leaflet + OpenStreetMap tiles.
 *
 * Pure marker / bounds helpers live in `./map-data.ts` (re-exported here
 * so existing imports keep working). Leaflet's CSS must be loaded by the
 * host application (Studio's `main.tsx`, the playground entry, etc.) —
 * we do NOT import it from this module so that vitest's node env can
 * still parse this file without a CSS loader.
 */

import { useMemo, useEffect, useRef } from "react";
import { MapContainer, TileLayer, Marker, Popup, useMap } from "react-leaflet";
import L from "leaflet";
import type { GraphObject } from "@prism/core/object-model";
import {
  extractMarkers,
  computeBounds,
  type MapMarker,
  type MapBounds,
} from "./map-data.js";

export { extractMarkers, computeBounds, type MapMarker, type MapBounds };

// Leaflet ships marker icons as relative URLs that break under bundlers.
// Resolve them via Vite's `?url` handling so the playground / Studio bundle
// picks up the assets from `node_modules/leaflet/dist/images/`.
//
// We patch L.Icon.Default's `_getIconUrl` to point at the bundled URLs.
import iconRetinaUrl from "leaflet/dist/images/marker-icon-2x.png";
import iconUrl from "leaflet/dist/images/marker-icon.png";
import shadowUrl from "leaflet/dist/images/marker-shadow.png";

let iconsPatched = false;
function patchLeafletIcons() {
  if (iconsPatched) return;
  iconsPatched = true;
  delete (L.Icon.Default.prototype as { _getIconUrl?: unknown })._getIconUrl;
  L.Icon.Default.mergeOptions({
    iconRetinaUrl: iconRetinaUrl as unknown as string,
    iconUrl: iconUrl as unknown as string,
    shadowUrl: shadowUrl as unknown as string,
  });
}

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

function FitBounds({ bounds }: { bounds: MapBounds }) {
  const map = useMap();
  useEffect(() => {
    const corners: L.LatLngBoundsLiteral = [
      [bounds.minLat, bounds.minLng],
      [bounds.maxLat, bounds.maxLng],
    ];
    map.fitBounds(corners, { padding: [24, 24], animate: false });
  }, [map, bounds.minLat, bounds.maxLat, bounds.minLng, bounds.maxLng]);
  return null;
}

export function MapWidgetRenderer(props: MapWidgetProps) {
  const {
    objects,
    latField,
    lngField,
    titleField,
    initialZoom = 4,
    height = 320,
    onSelectObject,
  } = props;

  const containerRef = useRef<HTMLDivElement>(null);
  patchLeafletIcons();

  const markers = useMemo(
    () => extractMarkers(objects, latField, lngField, titleField),
    [objects, latField, lngField, titleField],
  );

  const bounds = useMemo(() => computeBounds(markers), [markers]);

  const center: L.LatLngExpression = markers.length
    ? [
        (bounds.minLat + bounds.maxLat) / 2,
        (bounds.minLng + bounds.maxLng) / 2,
      ]
    : [20, 0];

  return (
    <div
      ref={containerRef}
      data-testid="map-widget"
      style={{
        border: "1px solid #059669",
        borderRadius: 6,
        background: "#0f172a",
        padding: 8,
        color: "#e2e8f0",
      }}
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          color: "#059669",
          textTransform: "uppercase",
          marginBottom: 6,
        }}
      >
        Map — {markers.length} marker{markers.length === 1 ? "" : "s"}
      </div>
      {markers.length === 0 ? (
        <div
          style={{
            padding: 24,
            color: "#94a3b8",
            fontSize: 12,
            textAlign: "center",
          }}
        >
          No objects with valid {latField}/{lngField} fields.
        </div>
      ) : (
        <div
          style={{
            width: "100%",
            height,
            borderRadius: 4,
            overflow: "hidden",
          }}
        >
          <MapContainer
            center={center}
            zoom={initialZoom}
            scrollWheelZoom
            style={{ width: "100%", height: "100%" }}
          >
            <TileLayer
              attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
              url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
            />
            <FitBounds bounds={bounds} />
            {markers.map((m) => (
              <Marker
                key={m.id}
                position={[m.lat, m.lng]}
                {...(onSelectObject
                  ? { eventHandlers: { click: () => onSelectObject(m.id) } }
                  : {})}
              >
                <Popup>
                  <div
                    data-testid={`map-marker-${m.id}`}
                    style={{ fontSize: 12, fontWeight: 600 }}
                  >
                    {m.title}
                  </div>
                  <div style={{ fontSize: 10, color: "#64748b" }}>
                    {m.lat.toFixed(4)}, {m.lng.toFixed(4)}
                  </div>
                </Popup>
              </Marker>
            ))}
          </MapContainer>
        </div>
      )}
    </div>
  );
}
