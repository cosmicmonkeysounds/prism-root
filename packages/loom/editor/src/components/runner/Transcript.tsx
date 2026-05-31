// Phase 3 of the Loom IDE redesign: hover/click-aware transcript.
// Each line is a focusable envelope. Dialogue speakers are also
// individually focusable as character refs — hovering a speaker name
// dims envelopes that don't involve that character.

import { useSession } from "@/store/session";
import { eventTag, type LedgerEvent } from "./event-format";
import { useDim, useFocusableProps, useIsFocused } from "./focus-helpers";
import { useActiveHead, usePrimaryHeadId } from "./use-head";
import clsx from "clsx";

export function TranscriptPanel() {
  const hasWorkspace = useSession((s) => s.active !== null);
  const head = usePrimaryHeadId();
  const active = useActiveHead();
  if (!hasWorkspace) return <EmptyState msg="No workspace open." />;
  const events = (active?.transcript ?? []) as LedgerEvent[];
  if (events.length === 0) {
    return <EmptyState msg="Start a play session to see the transcript." />;
  }
  return (
    <ol className="h-full overflow-auto bg-zinc-950 px-4 py-3 space-y-2 font-mono text-sm">
      {events.map((e, i) => (
        <TranscriptItem key={i} idx={i} head={head} event={e} />
      ))}
    </ol>
  );
}

function EmptyState({ msg }: { msg: string }) {
  return (
    <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
      {msg}
    </div>
  );
}

function TranscriptItem({
  idx,
  head,
  event,
}: {
  idx: number;
  head: string;
  event: LedgerEvent;
}) {
  const ref = { kind: "envelope" as const, head, idx };
  const props = useFocusableProps(ref, "popover");
  const dim = useDim((r) => r.envelopes.has(idx));
  const focused = useIsFocused(ref);
  return (
    <li
      {...props}
      className={clsx(
        "rounded transition-opacity cursor-pointer px-1",
        dim,
        focused && "ring-1 ring-blue-400/40 bg-blue-400/5",
      )}
    >
      <EventLine event={event} />
    </li>
  );
}

function EventLine({ event }: { event: LedgerEvent }) {
  const [tag, body] = eventTag(event);
  switch (tag) {
    case "Action":
      return <span className="text-zinc-200">{String(body.text ?? "")}</span>;
    case "Dialogue": {
      const speakers = (body.speakers as string[]) ?? [
        body.speaker as string ?? "—",
      ];
      return (
        <div>
          <div className="text-xs uppercase tracking-wider flex gap-2">
            {speakers.map((name) => (
              <SpeakerName key={name} name={name} />
            ))}
            {body.parenthetical ? (
              <span className="text-zinc-500 italic normal-case">
                ({String(body.parenthetical)})
              </span>
            ) : null}
          </div>
          <div className="text-zinc-200 pl-3">{String(body.text ?? "")}</div>
        </div>
      );
    }
    case "Scene":
      return (
        <div className="text-blue-300 uppercase tracking-widest border-b border-white/10 pb-1">
          {String(body.text ?? "")}
        </div>
      );
    case "ChoiceTaken":
      return (
        <span className="text-emerald-300">
          ▸ chose “{String(body.text ?? "")}”
        </span>
      );
    case "BeatEntered":
      return <BeatName name={String(body.beat ?? "")} />;
    case "Diverted":
    case "Tunneled":
      return (
        <span className="text-zinc-500 text-xs">
          → <BeatName name={String(body.target ?? body.beat ?? "")} inline />
        </span>
      );
    case "Ended":
      return <span className="text-zinc-500 italic text-xs">— end —</span>;
    default:
      return <span className="text-zinc-600 text-xs truncate">{tag}</span>;
  }
}

function SpeakerName({ name }: { name: string }) {
  const ref = { kind: "character" as const, name };
  const props = useFocusableProps(ref, "popover");
  return (
    <span
      {...props}
      className="text-amber-300 font-semibold cursor-pointer hover:underline"
    >
      {name}
    </span>
  );
}

function BeatName({ name, inline = false }: { name: string; inline?: boolean }) {
  const ref = { kind: "beat" as const, name };
  const props = useFocusableProps(ref, "popover");
  return (
    <span
      {...props}
      className={clsx(
        "cursor-pointer hover:underline",
        inline
          ? "text-zinc-500"
          : "text-zinc-500 text-xs uppercase tracking-wider block",
      )}
    >
      {inline ? name : `══ ${name} ══`}
    </span>
  );
}
