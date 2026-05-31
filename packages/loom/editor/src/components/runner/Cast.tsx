// Phase 6 of the Loom IDE redesign: Cast panel — surfaces the live
// LiveStage (participants / cohorts / locations) and lets the booth
// re-cast roles. First-cut: read-only view derived from the active
// head's world entries. Full LiveStage editing lands when the relay
// protocol carries cohort + location membership envelopes.

import { useMemo } from "react";
import { useActiveHead } from "./use-head";

export function CastPanel() {
  const head = useActiveHead();

  const cast = useMemo(() => {
    const entries = head?.world ?? [];
    const roles = new Map<string, string[]>();
    for (const [k, v] of entries) {
      if (k.startsWith("Roster.")) {
        const role = k.slice("Roster.".length);
        const list = String(v ?? "").split(/[, ]+/).filter(Boolean);
        roles.set(role, list);
      }
    }
    return [...roles.entries()];
  }, [head]);

  if (!head) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        Cast appears once a play session starts.
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto bg-zinc-950 px-3 py-3 font-mono text-xs space-y-2">
      <div className="text-zinc-400 text-[11px] uppercase tracking-wider">
        Roster
      </div>
      {cast.length === 0 ? (
        <div className="text-zinc-600 italic">
          No <span className="text-amber-300">Roster.*</span> entries on this
          head. Bind a role to a person to see them here.
        </div>
      ) : (
        cast.map(([role, people]) => (
          <div key={role} className="flex gap-3">
            <div className="text-amber-300 w-28 shrink-0">{role}</div>
            <div className="text-zinc-200">{people.join(", ")}</div>
          </div>
        ))
      )}
    </div>
  );
}
