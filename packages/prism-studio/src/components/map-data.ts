/**
 * Pure marker / bounds helpers for the map widget. Lives in a separate
 * file from `map-widget-renderer.tsx` so vitest (node env) can test the
 * geometry without pulling in leaflet's DOM-only runtime.
 */

import type { GraphObject } from "@prism/core/object-model";

export interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  title: string;
}

export interface MapBounds {
  minLat: number;
  maxLat: number;
  minLng: number;
  maxLng: number;
}

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

export function computeBounds(markers: MapMarker[]): MapBounds {
  if (markers.length === 0) return { minLat: -90, maxLat: 90, minLng: -180, maxLng: 180 };
  let minLat = Infinity, maxLat = -Infinity, minLng = Infinity, maxLng = -Infinity;
  for (const m of markers) {
    if (m.lat < minLat) minLat = m.lat;
    if (m.lat > maxLat) maxLat = m.lat;
    if (m.lng < minLng) minLng = m.lng;
    if (m.lng > maxLng) maxLng = m.lng;
  }
  const padLat = Math.max((maxLat - minLat) * 0.1, 0.5);
  const padLng = Math.max((maxLng - minLng) * 0.1, 0.5);
  return {
    minLat: minLat - padLat,
    maxLat: maxLat + padLat,
    minLng: minLng - padLng,
    maxLng: maxLng + padLng,
  };
}
