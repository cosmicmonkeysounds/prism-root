// Phase 6 of the Loom IDE redesign: References panel.
// Lights up with the LSP's textDocument/references once that lands.
// Today: when the focus bus has a pinned character / beat ref, the
// panel does a workspace-wide string scan as a placeholder.

import { useMemo } from "react";
import { useFocus } from "@/store/focus";
import { useWorkspace } from "@/store/workspace";

type Hit = { path: string; line: number; text: string };

export function ReferencesPanel() {
  const pinned = useFocus((s) => s.pinned);
  const files = useWorkspace((s) => s.openFiles);
  const needle = useMemo(() => {
    if (!pinned) return null;
    if (pinned.kind === "character" || pinned.kind === "beat") return pinned.name;
    if (pinned.kind === "world-key") return pinned.key;
    return null;
  }, [pinned]);

  const hits: Hit[] = useMemo(() => {
    if (!needle) return [];
    const out: Hit[] = [];
    const re = new RegExp(`\\b${escapeRegExp(needle)}\\b`);
    for (const [path, file] of Object.entries(files)) {
      const lines = file.contents.split("\n");
      for (let i = 0; i < lines.length; i++) {
        if (re.test(lines[i])) {
          out.push({ path, line: i + 1, text: lines[i].trim() });
          if (out.length > 200) return out;
        }
      }
    }
    return out;
  }, [needle, files]);

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

  return (
    <div className="h-full overflow-auto bg-zinc-950 px-2 py-2 font-mono text-xs">
      <div className="text-zinc-400 text-[11px] uppercase tracking-wider px-1 mb-1">
        References to <span className="text-amber-300">{needle}</span>
        <span className="text-zinc-600 ml-2">({hits.length})</span>
      </div>
      {hits.length === 0 ? (
        <div className="px-1 text-zinc-600 italic">No matches in open files.</div>
      ) : (
        hits.map((h, i) => (
          <div key={i} className="flex gap-2 px-1 py-0.5 hover:bg-white/5 rounded">
            <span className="text-zinc-600 w-32 shrink-0 truncate">
              {h.path}
            </span>
            <span className="text-zinc-600 w-10 text-right">{h.line}</span>
            <span className="text-zinc-200 truncate">{h.text}</span>
          </div>
        ))
      )}
    </div>
  );
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
