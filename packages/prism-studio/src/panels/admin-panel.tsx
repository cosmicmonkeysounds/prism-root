/**
 * Admin Panel — universal admin dashboard for Prism runtimes.
 *
 * Studio is a client-only SPA, so this one lens covers every runtime we
 * can reach:
 *   - Kernel source → the in-process StudioKernel (daemon-embedded runtime)
 *   - Relay source  → any configured @prism/relay server over HTTP
 *
 * The panel is just a Puck editor wired to `@prism/admin-kit`: the user
 * builds their dashboard by dragging widgets, and each widget reads its
 * live data through `<AdminProvider>`.
 */

import { useEffect, useMemo, useState } from "react";
import { Puck, type Data } from "@measured/puck";

import {
  AdminProvider,
  createAdminPuckConfig,
  createDefaultAdminLayout,
  createKernelDataSource,
  createRelayDataSource,
  type AdminDataSource,
} from "@prism/admin-kit";

import { useKernel, useRelay } from "../kernel/index.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";

const KERNEL_SOURCE_ID = "kernel";

const containerStyle: React.CSSProperties = {
  height: "100%",
  display: "flex",
  flexDirection: "column",
  fontFamily: "system-ui, -apple-system, sans-serif",
  background: "#1e1e1e",
  color: "#e5e5e5",
};

const headerStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 12,
  padding: "8px 14px",
  borderBottom: "1px solid #1e293b",
  background: "#0f172a",
  color: "#e2e8f0",
  flex: "0 0 auto",
};

const selectStyle: React.CSSProperties = {
  background: "#1e293b",
  border: "1px solid #334155",
  borderRadius: 4,
  color: "#e2e8f0",
  fontSize: 12,
  padding: "4px 8px",
  outline: "none",
};

export function AdminPanel() {
  const kernel = useKernel();
  const { relays } = useRelay();

  const [sourceId, setSourceId] = useState<string>(KERNEL_SOURCE_ID);
  const [data, setData] = useState<Data>(() => createDefaultAdminLayout());

  const puckConfig = useMemo(() => createAdminPuckConfig(), []);

  const dataSource = useMemo<AdminDataSource>(() => {
    if (sourceId === KERNEL_SOURCE_ID) {
      return createKernelDataSource(kernel, {
        id: KERNEL_SOURCE_ID,
        label: "Studio Kernel",
      });
    }
    const relay = relays.find((r) => r.id === sourceId);
    if (!relay) {
      return createKernelDataSource(kernel, {
        id: KERNEL_SOURCE_ID,
        label: "Studio Kernel",
      });
    }
    return createRelayDataSource({
      id: `relay:${relay.id}`,
      label: relay.name,
      url: relay.url,
    });
  }, [sourceId, kernel, relays]);

  useEffect(() => {
    return () => dataSource.dispose?.();
  }, [dataSource]);

  useEffect(() => {
    if (sourceId === KERNEL_SOURCE_ID) return;
    if (!relays.some((r) => r.id === sourceId)) {
      setSourceId(KERNEL_SOURCE_ID);
    }
  }, [sourceId, relays]);

  return (
    <div style={containerStyle} data-testid="admin-panel">
      <div style={headerStyle} data-testid="admin-panel-header">
        <div style={{ fontSize: 13, fontWeight: 600 }}>Admin Dashboard</div>
        <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 8 }}>
          <label
            htmlFor="admin-source-picker"
            style={{ fontSize: 11, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em" }}
          >
            Source
          </label>
          <select
            id="admin-source-picker"
            value={sourceId}
            onChange={(e) => setSourceId(e.target.value)}
            style={selectStyle}
            data-testid="admin-source-picker"
          >
            <option value={KERNEL_SOURCE_ID}>Studio Kernel</option>
            {relays.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name} ({r.status})
              </option>
            ))}
          </select>
        </div>
      </div>
      <div style={{ flex: 1, minHeight: 0 }}>
        <AdminProvider source={dataSource}>
          <Puck config={puckConfig} data={data} onChange={setData} />
        </AdminProvider>
      </div>
    </div>
  );
}

export const ADMIN_LENS_ID = lensId("admin");

export const adminLensManifest: LensManifest = {
  id: ADMIN_LENS_ID,
  name: "Admin",
  icon: "\u2699",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      {
        id: "switch-admin",
        name: "Switch to Admin Dashboard",
        shortcut: ["Shift+A"],
        section: "Navigation",
      },
    ],
  },
};

export const adminLensBundle: LensBundle = defineLensBundle(adminLensManifest, AdminPanel);
