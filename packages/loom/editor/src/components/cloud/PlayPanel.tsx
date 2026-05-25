// Phase 7 — co-playing UI. Shows the server-hosted Playhead's
// transcript + choice prompts for the active workspace. All
// participants see the same server-authoritative state; "the user
// who clicks a choice" advances the playhead for everybody.
//
// Renders read-only from `useSession.active.play`; emits `startPlay`
// / `sendChoice` / `stopPlay` through the same store.

import { useSession } from "@/store/session";
import clsx from "clsx";

type LedgerEvent = unknown;

export function PlayPanel() {
    const active = useSession((s) => s.active);
    const startPlay = useSession((s) => s.startPlay);
    const sendChoice = useSession((s) => s.sendChoice);
    const stopPlay = useSession((s) => s.stopPlay);

    if (!active) {
        return (
            <div className="h-full grid place-items-center text-zinc-500 text-sm bg-zinc-950">
                <div className="text-center space-y-2 max-w-sm">
                    <div className="text-zinc-300">No workspace open</div>
                    <div className="text-xs text-zinc-500">
                        Open a workspace in the Cloud panel to start a play
                        session.
                    </div>
                </div>
            </div>
        );
    }

    const play = active.play;

    return (
        <div className="h-full w-full flex flex-col bg-zinc-950 text-zinc-200">
            <header className="h-9 px-3 flex items-center gap-3 border-b border-white/10 text-xs">
                <span className="font-medium text-zinc-200 truncate">
                    Play · {active.meta.name}
                </span>
                {play && (
                    <span className="text-zinc-500">
                        host: {play.starter}
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

            <Transcript events={play?.transcript ?? []} />

            <Choices
                ended={play?.ended ?? false}
                choices={play?.choices ?? []}
                hasPlay={play !== null}
                onChoose={sendChoice}
            />
        </div>
    );
}

function Transcript({ events }: { events: LedgerEvent[] }) {
    if (events.length === 0) {
        return (
            <div className="flex-1 min-h-0 grid place-items-center text-zinc-500 text-xs px-4">
                Nothing has been played yet. Start a session to begin.
            </div>
        );
    }
    return (
        <ol className="flex-1 min-h-0 overflow-auto px-3 py-3 space-y-2 text-sm font-mono">
            {events.map((e, i) => (
                <li key={i} className="flex gap-3">
                    <span className="text-zinc-600 w-8 text-right shrink-0">
                        {i + 1}
                    </span>
                    <EventLine event={e} />
                </li>
            ))}
        </ol>
    );
}

function EventLine({ event }: { event: LedgerEvent }) {
    if (typeof event === "string") {
        return <span className="text-zinc-300">{event}</span>;
    }
    if (typeof event !== "object" || event === null) {
        return <span className="text-zinc-500 text-xs">unknown</span>;
    }
    // `loom_runtime::ledger::Event` is serialised as an externally-
    // tagged enum: `{ "Action": { "text": "…" } }`, `{ "Dialogue":
    // { speaker, text, … } }`, `"Ended"`, etc.
    const [tag, body] = firstKey(event as Record<string, unknown>);
    switch (tag) {
        case "Action":
            return (
                <span className="text-zinc-200">
                    {String((body as { text?: string }).text ?? "")}
                </span>
            );
        case "Dialogue": {
            const d = body as {
                speaker?: string;
                parenthetical?: string | null;
                text?: string;
            };
            return (
                <span>
                    <span className="text-amber-300 font-semibold">
                        {d.speaker ?? "—"}
                    </span>
                    {d.parenthetical && (
                        <span className="text-zinc-500 italic">
                            {" "}
                            ({d.parenthetical})
                        </span>
                    )}
                    <span className="text-zinc-200">: {d.text ?? ""}</span>
                </span>
            );
        }
        case "Scene":
            return (
                <span className="text-blue-300 uppercase tracking-wider">
                    {String((body as { text?: string }).text ?? "")}
                </span>
            );
        case "ChoiceTaken": {
            const c = body as { index?: number; text?: string };
            return (
                <span className="text-emerald-300">
                    ▸ chose “{c.text ?? ""}”
                </span>
            );
        }
        case "Diverted":
        case "Tunneled": {
            const d = body as { target?: string; beat?: string };
            return (
                <span className="text-zinc-500">
                    → {d.target ?? d.beat ?? ""}
                </span>
            );
        }
        case "BeatEntered": {
            const b = body as { beat?: string };
            return (
                <span className="text-zinc-500">
                    ╴ {String(b.beat ?? "")}
                </span>
            );
        }
        case "Ended":
            return <span className="text-zinc-500 italic">— end —</span>;
        default:
            return (
                <span className="text-zinc-500 text-xs truncate">
                    {tag}
                </span>
            );
    }
}

function Choices({
    ended,
    choices,
    hasPlay,
    onChoose,
}: {
    ended: boolean;
    choices: { index: number; text: string; sticky: boolean }[];
    hasPlay: boolean;
    onChoose: (index: number) => void;
}) {
    if (!hasPlay) return null;
    if (ended) {
        return (
            <div className="border-t border-white/10 px-3 py-3 text-xs text-zinc-500">
                Session ended. Click <strong>Start play</strong> to replay.
            </div>
        );
    }
    if (choices.length === 0) return null;
    return (
        <div className="border-t border-white/10 px-3 py-3 flex flex-col gap-1.5">
            <div className="text-[11px] uppercase tracking-wider text-zinc-500">
                Choose
            </div>
            {choices.map((c) => (
                <button
                    key={c.index}
                    type="button"
                    onClick={() => onChoose(c.index)}
                    className="text-left text-sm px-3 py-1.5 rounded border border-white/10 hover:border-emerald-400/40 hover:bg-emerald-400/5"
                >
                    <span className="text-zinc-500 mr-2">{c.index + 1}.</span>
                    <span className="text-zinc-100">{c.text}</span>
                    {c.sticky && (
                        <span className="ml-2 text-[10px] text-amber-400">
                            sticky
                        </span>
                    )}
                </button>
            ))}
        </div>
    );
}

function firstKey(obj: Record<string, unknown>): [string, unknown] {
    for (const k in obj) return [k, obj[k]];
    return ["", undefined];
}
