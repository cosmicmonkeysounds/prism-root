// Phase 3 of the Loom IDE redesign: hover/click-aware envelope table.
// Each row is a focusable envelope ref; the highlight + dim passes
// flow through the shared focus bus.

import {
  eventSummary,
  eventTag,
  EVENT_COLORS,
  type LedgerEvent,
} from "./event-format";
import { useDim, useFocusableProps, useIsFocused } from "./focus-helpers";
import { useActiveHead, usePrimaryHeadId } from "./use-head";
import clsx from "clsx";

export function LedgerPanel() {
  const head = usePrimaryHeadId();
  const active = useActiveHead();
  const events = (active?.transcript ?? []) as LedgerEvent[];
  const meta = active?.meta ?? [];

  if (events.length === 0) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        No ledger yet — start a play session.
      </div>
    );
  }
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono text-xs text-zinc-300">
      <table className="w-full border-collapse">
        <tbody>
          {events.map((e, i) => (
            <LedgerRow
              key={i}
              idx={i}
              head={head}
              event={e}
              track={meta[i]?.track ?? 0}
              cause={meta[i]?.cause ?? null}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function LedgerRow({
  idx,
  head,
  event,
  track,
  cause,
}: {
  idx: number;
  head: string;
  event: LedgerEvent;
  track: number;
  cause: number | null;
}) {
  const ref = { kind: "envelope" as const, head, idx };
  const props = useFocusableProps(ref, "panel");
  const dim = useDim((r) => r.envelopes.has(idx));
  const focused = useIsFocused(ref);
  const [tag] = eventTag(event);
  const colour = EVENT_COLORS[tag] ?? "#78909c";
  return (
    <tr
      {...props}
      className={clsx(
        "border-b border-white/5 cursor-pointer transition-opacity",
        dim,
        focused ? "bg-blue-400/10" : "hover:bg-white/5",
      )}
    >
      <td className="px-2 py-0.5 text-right text-zinc-600 w-12">#{idx}</td>
      <td className="px-2 py-0.5 text-zinc-600 w-12">t{track}</td>
      <td className="px-2 py-0.5 text-zinc-700 w-12">
        {cause != null ? `←#${cause}` : ""}
      </td>
      <td className="px-2 py-0.5 w-32">
        <span
          className="inline-block w-2 h-2 rounded-sm mr-2 align-middle"
          style={{ backgroundColor: colour }}
        />
        <span style={{ color: colour }}>{tag}</span>
      </td>
      <td className="px-2 py-0.5 text-zinc-300 truncate">{eventSummary(event)}</td>
    </tr>
  );
}
