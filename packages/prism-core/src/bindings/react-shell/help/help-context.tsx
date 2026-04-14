import { createContext, useCallback, useContext, type ReactNode } from "react";
import { HelpRegistry } from "./help-registry.js";
import type { HelpEntry } from "./types.js";

export interface HelpContextValue {
  /** Open a documentation sheet for the given logical path and optional anchor. */
  openDoc: (docPath: string, anchor?: string) => void;
  /** Search registered help entries by title/summary (case-insensitive). */
  searchEntries: (query: string) => HelpEntry[];
}

const noopContext: HelpContextValue = {
  openDoc: () => {},
  searchEntries: () => [],
};

const HelpContext = createContext<HelpContextValue>(noopContext);

/**
 * Provides the `openDoc` callback used by HelpTooltip's "View full docs"
 * button and by DocSearch result clicks. Mount at the app root (or at the
 * panel root for a per-lens help surface) and pass `onOpenDoc` that
 * displays a DocSheet.
 */
export function HelpProvider({
  children,
  onOpenDoc,
}: {
  children: ReactNode;
  onOpenDoc: (docPath: string, anchor?: string) => void;
}) {
  const searchEntries = useCallback(
    (query: string) => HelpRegistry.search(query),
    [],
  );
  return (
    <HelpContext.Provider value={{ openDoc: onOpenDoc, searchEntries }}>
      {children}
    </HelpContext.Provider>
  );
}

export function useHelp(): HelpContextValue {
  return useContext(HelpContext);
}
