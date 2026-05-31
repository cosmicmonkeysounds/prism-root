// Phase 3 of the Loom IDE redesign: focus-bus-aware World tree.
// Entity headers focus a `character` ref; individual rows focus a
// `world-key` ref. Hovering either dims unrelated envelopes on the
// Timeline / Ledger / Transcript panels simultaneously.

import { useMemo } from "react";
import clsx from "clsx";
import { useActiveHead } from "./use-head";
import { useDim, useFocusableProps, useIsFocused } from "./focus-helpers";

type Row = { fullKey: string; key: string; value: string };
type Group = { entity: string; rows: Row[] };

function groupEntries(entries: [string, string][]): Group[] {
  const groups = new Map<string, Row[]>();
  for (const [fullKey, value] of entries) {
    const dot = fullKey.indexOf(".");
    const entity = dot === -1 ? "(scope)" : fullKey.slice(0, dot);
    const rest = dot === -1 ? fullKey : fullKey.slice(dot + 1);
    if (!groups.has(entity)) groups.set(entity, []);
    groups.get(entity)!.push({ fullKey, key: rest, value });
  }
  return [...groups.entries()]
    .map(([entity, rows]) => ({
      entity,
      rows: rows.sort((a, b) => a.key.localeCompare(b.key)),
    }))
    .sort((a, b) => a.entity.localeCompare(b.entity));
}

export function WorldPanel() {
  const head = useActiveHead();
  const entries = head?.world;
  const groups = useMemo(() => groupEntries(entries ?? []), [entries]);
  if (groups.length === 0) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        World is empty — start a play session.
      </div>
    );
  }
  return (
    <div className="h-full overflow-auto bg-zinc-950 px-2 py-2 font-mono text-xs">
      {groups.map((g) => (
        <WorldGroup key={g.entity} group={g} />
      ))}
    </div>
  );
}

function WorldGroup({ group }: { group: Group }) {
  return (
    <details open className="mb-1.5">
      {group.entity === "(scope)" ? (
        <ScopeHeader count={group.rows.length} />
      ) : (
        <EntityHeader entity={group.entity} count={group.rows.length} />
      )}
      <table className="w-full mt-0.5">
        <tbody>
          {group.rows.map((r) => (
            <WorldRow key={r.fullKey} row={r} />
          ))}
        </tbody>
      </table>
    </details>
  );
}

function ScopeHeader({ count }: { count: number }) {
  return (
    <summary className="text-zinc-300 px-1 py-0.5 hover:bg-white/5">
      <span className="text-amber-300">(scope)</span>
      <span className="text-zinc-600 ml-2">({count})</span>
    </summary>
  );
}

function EntityHeader({ entity, count }: { entity: string; count: number }) {
  const ref = { kind: "character" as const, name: entity };
  const props = useFocusableProps(ref, "panel");
  const dim = useDim((r) => r.characters.has(entity));
  const focused = useIsFocused(ref);
  return (
    <summary
      {...props}
      className={clsx(
        "cursor-pointer text-zinc-300 px-1 py-0.5",
        dim,
        focused ? "bg-blue-400/10" : "hover:bg-white/5",
      )}
    >
      <span className="text-amber-300">{entity}</span>
      <span className="text-zinc-600 ml-2">({count})</span>
    </summary>
  );
}

function WorldRow({ row }: { row: Row }) {
  const ref = { kind: "world-key" as const, key: row.fullKey };
  const props = useFocusableProps(ref, "popover");
  const dim = useDim((r) => r.worldKeys.has(row.fullKey));
  const focused = useIsFocused(ref);
  return (
    <tr
      {...props}
      className={clsx(
        "cursor-pointer transition-opacity",
        dim,
        focused ? "bg-blue-400/10" : "hover:bg-white/5",
      )}
    >
      <td className="pl-5 pr-2 py-0.5 text-zinc-400 w-1/2 truncate">{row.key}</td>
      <td className="px-2 py-0.5 text-zinc-200 truncate">{row.value}</td>
    </tr>
  );
}
