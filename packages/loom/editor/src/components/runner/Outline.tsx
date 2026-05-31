// Phase 6 of the Loom IDE redesign: Outline panel.
// Will be backed by `loom-lsp`'s `textDocument/documentSymbol` once
// the LSP runs inside the editor (currently only the parser/linter
// half of the wasm bundle is loaded). Today: extracts beat / scene /
// character headers from the active file by a single-line scan so the
// slot has signal while the LSP integration lands.

import { useMemo } from "react";
import { useWorkspace } from "@/store/workspace";

type Symbol = { kind: string; name: string; line: number };

const HEADER_RE = /^\s*(==|SCENE|CHARACTER|TRAIT|STATS|TREE|ITEM|FACTION|COHORT|LOCATION|GENERATOR|PERSON|ROSTER|SCENE)\b\s*([^\s(]+)?/;

function extract(text: string): Symbol[] {
  const out: Symbol[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const m = HEADER_RE.exec(lines[i]);
    if (!m) continue;
    const kind = m[1] === "==" ? "beat" : m[1].toLowerCase();
    out.push({ kind, name: m[2] ?? "(unnamed)", line: i + 1 });
  }
  return out;
}

export function OutlinePanel() {
  const activePath = useWorkspace((s) => s.activePath);
  const file = useWorkspace((s) =>
    activePath ? s.openFiles[activePath] : null,
  );
  const symbols = useMemo(() => (file ? extract(file.contents) : []), [file]);
  if (!activePath) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        Open a file to see its outline.
      </div>
    );
  }
  if (symbols.length === 0) {
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
          <span className="text-zinc-600 w-10 text-right">{s.line}</span>
          <span className="text-amber-300 w-16 shrink-0">{s.kind}</span>
          <span className="text-zinc-200 truncate">{s.name}</span>
        </div>
      ))}
    </div>
  );
}
