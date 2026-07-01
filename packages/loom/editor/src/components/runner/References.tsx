// References panel — driven by `@loom/core/lsp`. The focus bus supplies
// either a bare needle (pinned character / beat / world-key) resolved via
// `referencesByName`, or a positional `symbol` ref (from Shift-F12 in the
// editor) resolved via `referencesAt` — the parser-resolved token under the
// cursor. Rows are click-to-jump. Now that the whole project is indexed,
// matches (and previews) span unopened files too.

import { useEffect, useMemo, useState } from "react";
import { useFocus } from "@/store/focus";
import { useWorkspace } from "@/store/workspace";
import { docText, lspWorkspace, pathForUri, uriFor } from "@/lib/lsp-client";
import { navigateToLocation } from "@/lib/lsp-nav";
import { useLspIndexGen } from "@/lib/lsp-index";

type Location = {
  uri: string;
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
};

export function ReferencesPanel() {
  const pinned = useFocus((s) => s.pinned);
  const files = useWorkspace((s) => s.openFiles);
  // Re-query when the Workspace's indexed text actually changes (buffer sync /
  // project index completing), rather than on every keystroke via `files`.
  const indexGen = useLspIndexGen((s) => s.gen);
  const label = useMemo(() => {
    if (!pinned) return null;
    if (pinned.kind === "character" || pinned.kind === "beat") return pinned.name;
    if (pinned.kind === "world-key") return pinned.key;
    if (pinned.kind === "symbol") return pinned.label;
    return null;
  }, [pinned]);

  const [state, setState] = useState<{
    key: string | null;
    hits: Location[] | null;
    error: string | null;
  }>({ key: null, hits: null, error: null });

  const pinKey = pinned
    ? pinned.kind === "symbol"
      ? `sym:${pinned.uri}:${pinned.pos.line}:${pinned.pos.character}`
      : label
    : null;

  useEffect(() => {
    if (!pinned || !pinKey) return;
    let cancelled = false;
    (async () => {
      try {
        const ws = await lspWorkspace();
        const out =
          pinned.kind === "symbol"
            ? ws.referencesAt(pinned.uri, pinned.pos)
            : label
              ? ws.referencesByName(label)
              : [];
        if (cancelled) return;
        const arr = Array.isArray(out) ? (out as Location[]) : [];
        setState({ key: pinKey, hits: arr, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({ key: pinKey, hits: null, error: String(err) });
      }
    })();
    return () => {
      cancelled = true;
    };
    // `indexGen` bumps whenever the Workspace text changes (a buffer sync or a
    // project (re)index), so results track the live buffers without re-running
    // on every keystroke ahead of the sync debounce.
  }, [pinned, pinKey, label, indexGen]);

  const hits = state.key === pinKey ? state.hits : null;
  const error = state.key === pinKey ? state.error : null;

  if (!label) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        <div>
          <div>References</div>
          <div className="text-zinc-600 mt-1 max-w-xs">
            Pin a character, beat, or world key (click it on any panel), or press{" "}
            <kbd className="px-1 rounded bg-white/5 border border-white/10">⇧F12</kbd> in the
            editor to find references to the symbol under the cursor.
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
        References to <span className="text-amber-300">{label}</span>
        <span className="text-zinc-600 ml-2">({hits?.length ?? "…"})</span>
      </div>
      {hits === null ? (
        <div className="px-1 text-zinc-600 italic">Scanning…</div>
      ) : hits.length === 0 ? (
        <div className="px-1 text-zinc-600 italic">No matches in project.</div>
      ) : (
        hits.map((h, i) => {
          const path = pathForUri(h.uri);
          return (
            <button
              key={i}
              type="button"
              onClick={() => void navigateToLocation(h)}
              className="w-full text-left flex gap-2 px-1 py-0.5 hover:bg-white/5 rounded cursor-pointer"
            >
              <span className="text-zinc-600 w-32 shrink-0 truncate">{path}</span>
              <span className="text-zinc-600 w-10 text-right">{h.range.start.line + 1}</span>
              <span className="text-zinc-200 truncate">
                {previewFor(files, path, h.range.start.line)}
              </span>
            </button>
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
  // Prefer the live open buffer; fall back to the indexed doc text so hits in
  // unopened files still render a preview.
  const source = files[path]?.contents ?? docText(uriFor(path));
  if (!source) return "";
  return source.split("\n")[line]?.trim() ?? "";
}
