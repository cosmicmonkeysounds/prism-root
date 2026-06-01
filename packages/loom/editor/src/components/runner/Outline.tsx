// Phase 6 / follow-up: Outline panel — driven by `loom-lsp`'s
// `documentSymbol` over the wasm bundle. Re-syncs the active file's
// contents into the LSP workspace on every change, then renders the
// returned `DocumentSymbol[]` tree (single level for now — Loom's
// grammar is flat).

import { useEffect, useState } from "react";
import { useWorkspace } from "@/store/workspace";
import { lspWorkspace, uriFor } from "@/lib/lsp-client";

type LspDocSymbol = {
  name: string;
  kind: number;
  range: { start: { line: number; character: number } };
};

// Subset of LSP SymbolKind we care to label.
const KIND_LABEL: Record<number, string> = {
  3: "ns",
  5: "class",
  6: "method",
  10: "enum",
  11: "iface",
  12: "function",
  13: "variable",
  17: "object",
  19: "package",
  22: "struct",
  23: "event",
  24: "operator",
  25: "array",
};

export function OutlinePanel() {
  const activePath = useWorkspace((s) => s.activePath);
  const file = useWorkspace((s) =>
    activePath ? s.openFiles[activePath] : null,
  );
  // Symbols are stored keyed by the path they belong to so a stale
  // `setState` from a previous file never overwrites the current one.
  const [state, setState] = useState<{
    path: string | null;
    symbols: LspDocSymbol[] | null;
    error: string | null;
  }>({ path: null, symbols: null, error: null });

  useEffect(() => {
    if (!activePath || !file) return;
    let cancelled = false;
    (async () => {
      try {
        const ws = await lspWorkspace();
        ws.open(uriFor(activePath), file.contents);
        const resp = ws.documentSymbols(uriFor(activePath));
        if (cancelled) return;
        const arr = Array.isArray(resp) ? (resp as LspDocSymbol[]) : [];
        setState({ path: activePath, symbols: arr, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({ path: activePath, symbols: null, error: String(err) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activePath, file]);

  const symbols = state.path === activePath ? state.symbols : null;
  const error = state.path === activePath ? state.error : null;

  if (!activePath) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        Open a file to see its outline.
      </div>
    );
  }
  if (error) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-rose-300 text-xs px-4 text-center">
        Outline failed: {error}
      </div>
    );
  }
  if (!symbols || symbols.length === 0) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        No top-level declarations in this file.
      </div>
    );
  }
  return (
    <div className="h-full overflow-auto bg-zinc-950 px-2 py-2 font-mono text-xs">
      {symbols.map((s, i) => (
        <div
          key={i}
          className="flex gap-2 px-1 py-0.5 hover:bg-white/5 rounded"
        >
          <span className="text-zinc-600 w-10 text-right">
            {s.range.start.line + 1}
          </span>
          <span className="text-amber-300 w-16 shrink-0">
            {KIND_LABEL[s.kind] ?? "decl"}
          </span>
          <span className="text-zinc-200 truncate">{s.name}</span>
        </div>
      ))}
    </div>
  );
}
