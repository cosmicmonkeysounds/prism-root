/**
 * AdminProvider — React context wiring a data source to a reactive snapshot.
 *
 * Widgets call `useAdminSnapshot()` and re-render whenever the underlying
 * source emits new data. The provider handles subscribe/dispose, periodic
 * polling for pull-only sources, and safe hand-off when the source changes.
 */

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { AdminDataSource, AdminSnapshot } from "./types.js";
import { emptySnapshot } from "./types.js";

interface AdminContextValue {
  source: AdminDataSource;
  snapshot: AdminSnapshot;
  /** Manual re-fetch trigger (useful for "Refresh" buttons). */
  refresh: () => void;
  /** True while a fetch is in-flight. */
  loading: boolean;
  /** Last error from a fetch attempt, if any. */
  error: string | null;
}

const AdminContext = createContext<AdminContextValue | null>(null);

export interface AdminProviderProps {
  source: AdminDataSource;
  /** Poll interval in ms for sources without subscribe(). Default 5000. */
  pollMs?: number;
  children: ReactNode;
}

export function AdminProvider({ source, pollMs = 5000, children }: AdminProviderProps) {
  const [snapshot, setSnapshot] = useState<AdminSnapshot>(() => emptySnapshot(source.id, source.label));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fetchTick, setFetchTick] = useState(0);

  const refresh = () => setFetchTick((n) => n + 1);

  // Fetch-on-demand path (initial load + refresh + polling fallback).
  useEffect(() => {
    let cancelled = false;
    async function run() {
      setLoading(true);
      try {
        const snap = await source.snapshot();
        if (!cancelled) {
          setSnapshot(snap);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void run();
    return () => {
      cancelled = true;
    };
  }, [source, fetchTick]);

  // Subscribe path — if the source supports push updates, wire listeners.
  // Otherwise fall back to a setInterval poll.
  useEffect(() => {
    if (typeof source.subscribe === "function") {
      const unsubscribe = source.subscribe((snap) => setSnapshot(snap));
      return () => unsubscribe();
    }
    if (pollMs <= 0) return;
    const handle = setInterval(() => setFetchTick((n) => n + 1), pollMs);
    return () => clearInterval(handle);
  }, [source, pollMs]);

  const value = useMemo<AdminContextValue>(
    () => ({ source, snapshot, refresh, loading, error }),
    [source, snapshot, loading, error],
  );

  return <AdminContext.Provider value={value}>{children}</AdminContext.Provider>;
}

export function useAdminContext(): AdminContextValue {
  const ctx = useContext(AdminContext);
  if (!ctx) {
    throw new Error("useAdminContext must be used inside <AdminProvider>");
  }
  return ctx;
}

export function useAdminSnapshot(): AdminSnapshot {
  return useAdminContext().snapshot;
}
