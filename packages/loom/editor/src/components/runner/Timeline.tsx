// Phase 3 of the Loom IDE redesign: multitrack canvas wired into the
// focus + projection bus. Hovering a block sets envelope focus —
// every other panel dims accordingly; clicking opens the envelope
// detail in a popover anchored to the block. Hovering a row label
// sets track focus.

import { useMemo } from "react";
import {
  eventTag,
  EVENT_COLORS,
  TRACK_COLORS,
  type LedgerEvent,
} from "./event-format";
import { useFocus, useRelated, refKey, useEffectiveFocus } from "@/store/focus";
import { useActiveHead, usePrimaryHeadId } from "./use-head";
import type { PlayEnvelopeMeta, PlayTrackInfo } from "@/lib/sync";
import { useSession } from "@/store/session";
import { openContextMenu } from "@/store/context-menu";

const ROW_HEIGHT = 28;
const LABEL_WIDTH = 130;
const BLOCK_WIDTH = 12;
const BLOCK_GAP = 2;
const TOP_PAD = 12;

type Block = {
  idx: number;
  trackId: number;
  row: number;
  x: number;
  y: number;
  colour: string;
  tag: string;
  cause: number | null;
};

export function TimelinePanel() {
  const head = usePrimaryHeadId();
  const active = useActiveHead();
  const related = useRelated();
  const focus = useEffectiveFocus();
  const setHover = useFocus((s) => s.setHover);
  const openDetail = useFocus((s) => s.openDetail);
  const snapshotPlay = useSession((s) => s.snapshotPlay);
  const forkPlay = useSession((s) => s.forkPlay);

  const { rows, blocks, width, height } = useMemo(() => {
    const tracks: PlayTrackInfo[] = active?.tracks ?? [];
    const events: LedgerEvent[] = active?.transcript ?? [];
    const meta: PlayEnvelopeMeta[] = active?.meta ?? [];
    const rowOf = new Map<number, number>();
    tracks.forEach((t, i) => rowOf.set(t.id, i));
    const nextX = new Map<number, number>();
    const blocks: Block[] = [];
    for (let i = 0; i < events.length; i++) {
      const m = meta[i];
      if (!m) continue;
      const row = rowOf.get(m.track);
      if (row == null) continue;
      const x = nextX.get(m.track) ?? LABEL_WIDTH;
      const y = TOP_PAD + row * ROW_HEIGHT;
      const [tag] = eventTag(events[i]);
      blocks.push({
        idx: i,
        trackId: m.track,
        row,
        x,
        y,
        colour: EVENT_COLORS[tag] ?? "#78909c",
        tag,
        cause: m.cause,
      });
      nextX.set(m.track, x + BLOCK_WIDTH + BLOCK_GAP);
    }
    const widest = Math.max(LABEL_WIDTH + 200, ...nextX.values());
    return {
      rows: tracks,
      blocks,
      width: widest + 40,
      height: TOP_PAD * 2 + tracks.length * ROW_HEIGHT,
    };
  }, [active]);

  if (rows.length === 0) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4">
        Timeline is empty — start a play session.
      </div>
    );
  }

  const focusKey = refKey(focus);
  const blockByIdx = new Map(blocks.map((b) => [b.idx, b]));
  const hoverActive = useFocus.getState().hover !== null;
  const dimEnv = (idx: number) =>
    hoverActive && !related.envelopes.has(idx) ? 0.3 : 1;
  const dimRow = (track: number) =>
    hoverActive && !related.tracks.has(track) ? 0.4 : 1;

  return (
    <div className="h-full flex flex-col bg-[#15191e]">
      <HeadTabs />
      <div className="flex-1 min-h-0 overflow-auto">
      <svg width={width} height={height} className="block select-none">
        {rows.map((t, i) => {
          const y = TOP_PAD + i * ROW_HEIGHT;
          const colour = TRACK_COLORS[t.kind] ?? "#90a4ae";
          const ref = { kind: "track" as const, head, track: t.id };
          const focused = focusKey === refKey(ref);
          return (
            <g
              key={t.id}
              style={{ opacity: dimRow(t.id), cursor: "pointer" }}
              onPointerEnter={() => setHover(ref)}
              onPointerLeave={() => setHover(null)}
              onClick={(e) => {
                const r = (e.currentTarget as SVGGElement).getBoundingClientRect();
                openDetail(ref, {
                  sink: "panel",
                  anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
                });
              }}
              data-focusable
            >
              {focused && (
                <rect
                  x={0}
                  y={y}
                  width={LABEL_WIDTH}
                  height={ROW_HEIGHT - 4}
                  fill="rgb(96 165 250 / 0.08)"
                />
              )}
              <rect x={0} y={y} width={5} height={ROW_HEIGHT - 4} fill={colour} />
              <text
                x={12}
                y={y + 12}
                fill="#eceff1"
                fontFamily="Menlo, monospace"
                fontSize={10}
                fontWeight="bold"
              >
                {t.label}
              </text>
              <text
                x={12}
                y={y + 23}
                fill="#78909c"
                fontFamily="Menlo, monospace"
                fontSize={8}
              >
                {t.kind}
              </text>
              <line
                x1={LABEL_WIDTH}
                y1={y + ROW_HEIGHT / 2}
                x2={width - 8}
                y2={y + ROW_HEIGHT / 2}
                stroke="#2a3038"
                strokeWidth={1}
              />
            </g>
          );
        })}
        {blocks.map((b) => {
          if (b.cause == null) return null;
          const src = blockByIdx.get(b.cause);
          if (!src || src.trackId === b.trackId) return null;
          const dim =
            hoverActive &&
            !(related.envelopes.has(b.idx) && related.envelopes.has(b.cause))
              ? 0.15
              : 0.7;
          return (
            <line
              key={`a${b.idx}`}
              x1={src.x + BLOCK_WIDTH / 2}
              y1={src.y + ROW_HEIGHT / 2}
              x2={b.x + BLOCK_WIDTH / 2}
              y2={b.y + ROW_HEIGHT / 2}
              stroke="#ef5350"
              strokeWidth={0.8}
              opacity={dim}
            />
          );
        })}
        {blocks.map((b) => {
          const ref = { kind: "envelope" as const, head, idx: b.idx };
          const focused = focusKey === refKey(ref);
          return (
            <g
              key={`b${b.idx}`}
              style={{ opacity: dimEnv(b.idx), cursor: "pointer" }}
              onPointerEnter={() => setHover(ref)}
              onPointerLeave={() => setHover(null)}
              onClick={(e) => {
                const r = (
                  e.currentTarget as SVGGElement
                ).getBoundingClientRect();
                openDetail(ref, {
                  sink: "popover",
                  anchor: {
                    x: r.left,
                    y: r.top,
                    width: r.width,
                    height: r.height,
                  },
                });
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                openContextMenu(
                  [
                    {
                      label: `Snapshot @ #${b.idx}`,
                      onSelect: () =>
                        snapshotPlay({
                          head,
                          label: `at #${b.idx}`,
                        }),
                    },
                    {
                      label: `Fork from #${b.idx}`,
                      onSelect: () => {
                        // The server's snapshot captures the head's
                        // CURRENT state, not the state at envelope #N
                        // — true mid-history forking needs ledger
                        // truncation server-side. As a first-cut UX
                        // we snapshot the current state with a label
                        // pointing at #N and immediately fork from
                        // that snapshot, giving you a sibling head
                        // you can replay.
                        snapshotPlay({
                          head,
                          label: `fork base @ #${b.idx}`,
                        });
                        forkPlay({ parent: head });
                      },
                    },
                    {
                      label: "Open in detail panel",
                      onSelect: () =>
                        openDetail(ref, { sink: "panel" }),
                    },
                  ],
                  { x: e.clientX, y: e.clientY },
                );
              }}
              data-focusable
            >
              <rect
                x={b.x}
                y={b.y + 4}
                width={BLOCK_WIDTH}
                height={ROW_HEIGHT - 8}
                fill={b.colour}
                stroke={focused ? "#60a5fa" : "#11151b"}
                strokeWidth={focused ? 1.5 : 0.5}
              >
                <title>
                  {`#${b.idx}  ${b.tag}\ntrack ${b.trackId}, cause ${b.cause ?? "—"}\nright-click for fork / snapshot`}
                </title>
              </rect>
            </g>
          );
        })}
      </svg>
      </div>
    </div>
  );
}

// ─── Head tabs strip ────────────────────────────────────────────────
import clsx from "clsx";

function HeadTabs() {
  const play = useSession((s) => s.active?.play) ?? null;
  const setPrimaryHead = useSession((s) => s.setPrimaryHead);
  const forkPlay = useSession((s) => s.forkPlay);
  const snapshotPlay = useSession((s) => s.snapshotPlay);
  const dropHead = useSession((s) => s.dropHead);
  const focus = useEffectiveFocus();
  if (!play) return null;
  const primary = play.primary;
  return (
    <header className="h-9 px-2 flex items-center gap-1 border-b border-white/10 bg-zinc-950 text-xs overflow-x-auto shrink-0">
      {play.heads.map((h) => {
        const active = h.id === primary;
        return (
          <button
            key={h.id}
            type="button"
            onClick={() => setPrimaryHead(h.id)}
            className={clsx(
              "h-6 px-2 rounded flex items-center gap-1 border whitespace-nowrap",
              active
                ? "border-blue-400/40 bg-blue-400/10 text-blue-200"
                : "border-white/10 text-zinc-400 hover:text-zinc-200 hover:border-white/20",
            )}
            title={
              h.parent
                ? `forked from ${h.parent}${h.forkedFrom ? ` @ ${h.forkedFrom}` : ""}`
                : "primary head"
            }
          >
            <span>{h.id}</span>
            {h.ended && <span className="text-zinc-500 text-[10px]">end</span>}
            {h.choices.length > 0 && (
              <span className="text-emerald-400 text-[10px]">●</span>
            )}
            {h.id !== "h0" && (
              <span
                role="button"
                tabIndex={0}
                aria-label={`Drop ${h.id}`}
                onClick={(e) => {
                  e.stopPropagation();
                  dropHead(h.id);
                }}
                className="ml-1 text-zinc-600 hover:text-rose-400"
              >
                ×
              </span>
            )}
          </button>
        );
      })}
      <button
        type="button"
        onClick={() => {
          // Fork from a captured snapshot if the focus is on an
          // envelope (rewind-from-here), else fork live.
          if (focus?.kind === "envelope") {
            // Snapshot the parent at its current state, then fork
            // from the snapshot — the new head replays history up to
            // the snapshot ledger length.
            snapshotPlay({
              head: primary,
              label: `fork @ #${focus.idx}`,
            });
            // The server emits a play-state with the new snapshot id.
            // For Phase 4 first cut we still fork from live state
            // here; the dedicated "Fork from #N" affordance lands
            // when Timeline gets right-click context menus.
          }
          forkPlay({ parent: primary });
        }}
        className="h-6 px-2 rounded border border-emerald-400/30 text-emerald-300 hover:bg-emerald-400/10 ml-1 whitespace-nowrap"
      >
        + fork
      </button>
      <button
        type="button"
        onClick={() => snapshotPlay({ head: primary })}
        className="h-6 px-2 rounded border border-white/10 text-zinc-400 hover:text-zinc-200 hover:border-white/20 whitespace-nowrap"
      >
        snapshot
      </button>
      <span className="ml-auto text-zinc-600">{play.heads.length} head{play.heads.length === 1 ? "" : "s"}</span>
    </header>
  );
}
