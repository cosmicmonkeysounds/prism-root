// Phase 2 of the Loom IDE redesign: choice buttons split out from the
// monolithic PlayPanel so the dock layout can place them next to the
// transcript or stash them in a floating panel during performance.

import { useSession } from "@/store/session";
import { useActiveHead, usePrimaryHeadId } from "./use-head";
import clsx from "clsx";

export function ChoicesPanel() {
  const active = useSession((s) => s.active);
  const sendChoice = useSession((s) => s.sendChoice);
  const startPlay = useSession((s) => s.startPlay);
  const stopPlay = useSession((s) => s.stopPlay);
  const headId = usePrimaryHeadId();
  const head = useActiveHead();

  if (!active) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        No workspace open.
      </div>
    );
  }
  const play = active.play;
  return (
    <div className="h-full w-full flex flex-col bg-zinc-950 text-zinc-200">
      <header className="h-9 px-3 flex items-center gap-3 border-b border-white/10 text-xs">
        <span className="font-medium text-zinc-300">Choices</span>
        {play && (
          <span className="text-zinc-500">
            host: {play.starter} · head {headId}
          </span>
        )}
        <span className="ml-auto">
          {play ? (
            <button
              type="button"
              onClick={stopPlay}
              className="px-2 py-0.5 rounded border border-white/10 text-zinc-300 hover:text-zinc-100 hover:border-white/20"
            >
              Stop
            </button>
          ) : (
            <button
              type="button"
              onClick={startPlay}
              disabled={active.files.length === 0}
              className={clsx(
                "px-2 py-0.5 rounded border",
                active.files.length === 0
                  ? "border-white/5 text-zinc-600"
                  : "border-emerald-400/40 text-emerald-300 hover:bg-emerald-400/10",
              )}
            >
              Start play
            </button>
          )}
        </span>
      </header>
      <div className="flex-1 min-h-0 overflow-auto px-3 py-3">
        {!play && (
          <div className="text-zinc-500 text-xs">
            Click <strong>Start play</strong> above to begin.
          </div>
        )}
        {play && head?.ended && (
          <div className="text-zinc-500 text-xs">
            Session ended on this head.
          </div>
        )}
        {play && !head?.ended && head && head.choices.length === 0 && (
          <div className="text-zinc-500 text-xs italic">
            (waiting — no choice prompt yet)
          </div>
        )}
        {play && head && !head.ended && head.choices.length > 0 && (
          <ul className="flex flex-col gap-1.5">
            {head.choices.map((c) => (
              <li key={c.index}>
                <button
                  type="button"
                  onClick={() => sendChoice(c.index, headId)}
                  className="w-full text-left text-sm px-3 py-1.5 rounded border border-white/10 hover:border-emerald-400/40 hover:bg-emerald-400/5"
                >
                  <span className="text-zinc-500 mr-2">{c.index + 1}.</span>
                  <span className="text-zinc-100">{c.text}</span>
                  {c.sticky && (
                    <span className="ml-2 text-[10px] text-amber-400">sticky</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
