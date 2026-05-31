// Phase 3 of the Loom IDE redesign: Inspector reads the pinned focus
// from the bus and delegates to the shared DetailRegistry — so the
// content matches whatever the dock Detail panel / popover / modal
// would render.
//
// When nothing is pinned the panel shows the latest envelope (a
// reasonable default while authoring) and reminds the user that they
// can pin anything by clicking it.

import { useFocus } from "@/store/focus";
import { DetailFor } from "@/components/detail/registry";
import { eventTag, type LedgerEvent } from "./event-format";
import { useActiveHead } from "./use-head";

export function InspectorPanel() {
  const pinned = useFocus((s) => s.pinned);
  const head = useActiveHead();
  if (pinned) return <DetailFor for={pinned} />;
  if (!head || head.transcript.length === 0) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        <div>
          <div>Inspector</div>
          <div className="text-zinc-600 mt-1">
            Click any envelope, character, beat, world key, or track
            row to pin it here.
          </div>
        </div>
      </div>
    );
  }
  const events = head.transcript as LedgerEvent[];
  const last = events[events.length - 1];
  const [tag, body] = eventTag(last);
  const rows = Object.entries(body);
  return (
    <div className="h-full overflow-auto bg-zinc-950 px-3 py-3 font-mono text-xs">
      <div className="text-zinc-400 text-[11px] uppercase tracking-wider mb-1">
        Latest envelope (nothing pinned)
      </div>
      <div className="text-amber-300 mb-2">{tag}</div>
      <table className="w-full">
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k} className="border-b border-white/5">
              <td className="text-zinc-500 pr-3 py-0.5 align-top w-1/3 truncate">
                {k}
              </td>
              <td className="text-zinc-200 py-0.5 break-all whitespace-pre-wrap">
                {typeof v === "string" ? v : JSON.stringify(v)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
