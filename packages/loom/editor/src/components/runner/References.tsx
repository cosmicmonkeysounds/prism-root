// Phase 6 / follow-up: References panel — driven by `loom-lsp`'s
// `references_by_name` over the wasm bundle. The focus bus supplies
// the needle (pinned character / beat / world-key); the panel syncs
// every open file into the LSP workspace and renders the LSP
// `Location[]` response.

import { useEffect, useMemo, useState } from "react";
import { useFocus } from "@/store/focus";
import { useWorkspace } from "@/store/workspace";
import { lspWorkspace, syncOpenFiles } from "@/lib/lsp-client";

type Location = {
  uri: string;
  range: { start: { line: number; character: number } };
};

export function ReferencesPanel() {
  const pinned = useFocus((s) => s.pinned);
  const files = useWorkspace((s) => s.openFiles);
  const needle = useMemo(() => {
    if (!pinned) return null;
    if (pinned.kind === "character" || pinned.kind === "beat") return pinned.name;
    if (pinned.kind === "world-key") return pinned.key;
    return null;
  }, [pinned]);

  const [state, setState] = useState<{
    needle: string | null;
    hits: Location[] | null;
    error: string | null;
  }>({ needle: null, hits: null, error: null });

  useEffect(() => {
    if (!needle) return;
    let cancelled = false;
    (async () => {
      try {
        await syncOpenFiles(files);
        const ws = await lspWorkspace();
        const out = ws.referencesByName(needle);
        if (cancelled) return;
        const arr = Array.isArray(out) ? (out as Location[]) : [];
        setState({ needle, hits: arr, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({ needle, hits: null, error: String(err) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [needle, files]);

  const hits = state.needle === needle ? state.hits : null;
  const error = state.needle === needle ? state.error : null;

  if (!needle) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        <div>
          <div>References</div>
          <div className="text-zinc-600 mt-1 max-w-xs">
            Pin a character, beat, or world key (click it on any panel)
            to scan the workspace for references.
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-rose-300 text-xs px-4 text-center">
        References failed: {error}
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto bg-zinc-950 px-2 py-2 font-mono text-xs">
      <div className="text-zinc-400 text-[11px] uppercase tracking-wider px-1 mb-1">
        References to <span className="text-amber-300">{needle}</span>
        <span className="text-zinc-600 ml-2">({hits?.length ?? "…"})</span>
      </div>
      {hits === null ? (
        <div className="px-1 text-zinc-600 italic">Scanning…</div>
      ) : hits.length === 0 ? (
        <div className="px-1 text-zinc-600 italic">No matches in open files.</div>
      ) : (
        hits.map((h, i) => {
          const path = decodeURIComponent(h.uri.replace(/^inmemory:\/\//, ""));
          return (
            <div
              key={i}
              className="flex gap-2 px-1 py-0.5 hover:bg-white/5 rounded"
            >
              <span className="text-zinc-600 w-32 shrink-0 truncate">{path}</span>
              <span className="text-zinc-600 w-10 text-right">
                {h.range.start.line + 1}
              </span>
              <span className="text-zinc-200 truncate">
                {previewFor(files, path, h.range.start.line)}
              </span>
            </div>
          );
        })
      )}
    </div>
  );
}

function previewFor(
  files: Record<string, { contents: string }>,
  path: string,
  line: number,
): string {
  const file = files[path];
  if (!file) return "";
  const lines = file.contents.split("\n");
  return lines[line]?.trim() ?? "";
}
