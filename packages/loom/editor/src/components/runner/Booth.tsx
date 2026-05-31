// Phase 6 of the Loom IDE redesign: Booth panel — the director's
// console. Houses the cross-head controls (snapshot / fork / restore /
// drop) plus the live-patch surfaces the simulator's top bar exposed
// (skip beat, force-fire directive, hot-reload bundle).
//
// First-cut wiring: snapshot + fork + drop work today via the Phase 4
// session methods; skip / force / reload need new WS envelopes
// (booth-skip / booth-force / booth-reload), called out below as TODOs.

import { useState } from "react";
import clsx from "clsx";
import { useSession } from "@/store/session";

export function BoothPanel() {
  const play = useSession((s) => s.active?.play) ?? null;
  const snapshotPlay = useSession((s) => s.snapshotPlay);
  const forkPlay = useSession((s) => s.forkPlay);
  const restorePlay = useSession((s) => s.restorePlay);
  const dropHead = useSession((s) => s.dropHead);
  const setPrimaryHead = useSession((s) => s.setPrimaryHead);
  const [label, setLabel] = useState("");

  if (!play) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        <div>
          <div>Booth</div>
          <div className="text-zinc-600 mt-1 max-w-xs">
            Director controls activate once a play session is running.
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto bg-zinc-950 px-3 py-3 space-y-4 text-xs font-mono">
      <Section title="Heads">
        <table className="w-full">
          <tbody>
            {play.heads.map((h) => {
              const isPrimary = h.id === play.primary;
              return (
                <tr key={h.id} className="border-b border-white/5">
                  <td className="py-1 pr-2 text-zinc-200">
                    {isPrimary ? "★" : "·"} {h.id}
                  </td>
                  <td className="py-1 pr-2 text-zinc-500">
                    {h.parent ? `from ${h.parent}` : "primary"}
                    {h.forkedFrom ? ` @ ${h.forkedFrom}` : ""}
                  </td>
                  <td className="py-1 pr-2 text-zinc-600">
                    {h.transcript.length} env · {h.ended ? "ended" : `${h.choices.length} choice(s)`}
                  </td>
                  <td className="py-1 text-right">
                    <button
                      type="button"
                      onClick={() => setPrimaryHead(h.id)}
                      disabled={isPrimary}
                      className={clsx(
                        "px-1 hover:text-zinc-100",
                        isPrimary ? "text-zinc-700" : "text-zinc-400",
                      )}
                    >
                      promote
                    </button>
                    <button
                      type="button"
                      onClick={() => dropHead(h.id)}
                      disabled={isPrimary}
                      className={clsx(
                        "px-1",
                        isPrimary
                          ? "text-zinc-700"
                          : "text-zinc-400 hover:text-rose-400",
                      )}
                    >
                      drop
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <div className="flex items-center gap-2 mt-2">
          <button
            type="button"
            onClick={() => forkPlay({ parent: play.primary })}
            className="px-2 py-0.5 rounded border border-emerald-400/30 text-emerald-300 hover:bg-emerald-400/10"
          >
            fork primary
          </button>
          <button
            type="button"
            onClick={() =>
              snapshotPlay({
                head: play.primary,
                label: label || undefined,
              })
            }
            className="px-2 py-0.5 rounded border border-white/10 text-zinc-300 hover:border-white/20"
          >
            snapshot
          </button>
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="snapshot label…"
            className="bg-zinc-900 border border-white/10 rounded px-2 py-0.5 text-zinc-200 flex-1 min-w-0"
          />
        </div>
      </Section>

      <Section title="Snapshots">
        {play.snapshots.length === 0 ? (
          <div className="text-zinc-600 italic">
            No snapshots yet — use the button above to capture one.
          </div>
        ) : (
          <table className="w-full">
            <tbody>
              {play.snapshots.map((s) => (
                <tr key={s.id} className="border-b border-white/5">
                  <td className="py-1 pr-2 text-zinc-200">{s.id}</td>
                  <td className="py-1 pr-2 text-zinc-500">
                    {s.headId} @ #{s.at}
                  </td>
                  <td className="py-1 pr-2 text-zinc-400 truncate">
                    {s.label ?? ""}
                  </td>
                  <td className="py-1 text-right">
                    <button
                      type="button"
                      onClick={() =>
                        forkPlay({ parent: play.primary, fromSnapshot: s.id })
                      }
                      className="px-1 text-emerald-300 hover:text-emerald-200"
                    >
                      fork from
                    </button>
                    <button
                      type="button"
                      onClick={() => restorePlay(play.primary, s.id)}
                      className="px-1 text-zinc-400 hover:text-zinc-100"
                    >
                      restore
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>

      <Section title="Live patch">
        <div className="text-zinc-600 italic">
          Skip beat / force directive / hot reload land when the relay
          protocol grows the matching booth envelopes. For now,
          drive these via the standalone <code>loom-play</code> stdio
          driver (see <code>packages/loom/simulator</code>).
        </div>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="text-[11px] uppercase tracking-wider text-zinc-500 mb-1">
        {title}
      </div>
      {children}
    </div>
  );
}
