// Phase 3 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// §13): the Timeline — Run facet. Clips on tracks with a ruler, zoom +
// pan, clip extents, a playhead, and viewport culling. Reads the live
// play head (read-only); the Editing facet (author beats, draggable) is
// `BeatTimeline.tsx`.
//
// X-axis is ledger index for now (logical emission order). The hybrid
// "story clock as master ruler" (spec §13) needs a per-envelope clock
// the runtime doesn't emit yet — `unitLabel`/the ruler are factored so
// a clock accessor can swap in without touching the layout.

import { useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
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

const ROW_HEIGHT = 30;
const LABEL_WIDTH = 140;
const RULER_HEIGHT = 22;
const MIN_PPU = 4;
const MAX_PPU = 90;
const DEFAULT_PPU = 16;
const MIN_CLIP_PX = 6;

type Clip = {
  idx: number;
  track: number;
  row: number;
  start: number; // ledger index
  span: number; // units
  colour: string;
  tag: string;
  label: string;
  cause: number | null;
};

function clipLabel(event: LedgerEvent): string {
  const [tag, body] = eventTag(event);
  switch (tag) {
    case "BeatEntered":
      return String(body.beat ?? "beat");
    case "Dialogue": {
      const speakers = (body.speakers as string[]) ?? [body.speaker as string];
      return String(speakers?.[0] ?? "line");
    }
    case "Diverted":
    case "Tunneled":
      return `→ ${body.target ?? body.beat ?? ""}`;
    case "ChoiceTaken":
      return `▸ ${body.text ?? ""}`;
    case "WorldSet":
      return String(body.key ?? "set");
    case "Action":
      return "action";
    case "Scene":
      return "scene";
    default:
      return tag;
  }
}

export function TimelinePanel() {
  const head = usePrimaryHeadId();
  const active = useActiveHead();
  const related = useRelated();
  const focus = useEffectiveFocus();
  const hover = useFocus((s) => s.hover);
  const setHover = useFocus((s) => s.setHover);
  const openDetail = useFocus((s) => s.openDetail);
  const snapshotPlay = useSession((s) => s.snapshotPlay);
  const forkPlay = useSession((s) => s.forkPlay);

  const [ppu, setPpu] = useState(DEFAULT_PPU);
  const [showCauses, setShowCauses] = useState(false);
  const [view, setView] = useState({ left: 0, width: 0 });
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const { rows, clips, units } = useMemo(() => {
    const tracks: PlayTrackInfo[] = active?.tracks ?? [];
    const events: LedgerEvent[] = active?.transcript ?? [];
    const meta: PlayEnvelopeMeta[] = active?.meta ?? [];
    const rowOf = new Map<number, number>();
    tracks.forEach((t, i) => rowOf.set(t.id, i));

    // Beat clips span until the next BeatEntered; everything else is a
    // unit-width point clip.
    const beatStops: number[] = [];
    for (let i = 0; i < events.length; i++) {
      if (eventTag(events[i])[0] === "BeatEntered") beatStops.push(i);
    }
    const nextBeatAfter = (i: number) =>
      beatStops.find((b) => b > i) ?? events.length;

    const clips: Clip[] = [];
    for (let i = 0; i < events.length; i++) {
      const m = meta[i];
      if (!m) continue;
      const row = rowOf.get(m.track);
      if (row == null) continue;
      const [tag] = eventTag(events[i]);
      const span = tag === "BeatEntered" ? Math.max(1, nextBeatAfter(i) - i) : 1;
      clips.push({
        idx: i,
        track: m.track,
        row,
        start: i,
        span,
        colour: EVENT_COLORS[tag] ?? "#78909c",
        tag,
        label: clipLabel(events[i]),
        cause: m.cause,
      });
    }
    return { rows: tracks, clips, units: events.length };
  }, [active]);

  // Measure + track the scroll viewport for culling.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setView({ left: el.scrollLeft, width: el.clientWidth });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  if (rows.length === 0) {
    return (
      <div className="h-full flex flex-col bg-[#15191e]">
        <HeadTabs />
        <div className="flex-1 grid place-items-center text-zinc-500 text-xs px-4">
          Timeline is empty — start a play session.
        </div>
      </div>
    );
  }

  const laneWidth = Math.max(view.width || 0, units * ppu + 48);
  const bodyHeight = RULER_HEIGHT + rows.length * ROW_HEIGHT;
  const x = (index: number) => index * ppu;
  const focusKey = refKey(focus);
  const hoverActive = hover !== null;
  const dimEnv = (idx: number) => (hoverActive && !related.envelopes.has(idx) ? 0.25 : 1);
  const dimRow = (track: number) => (hoverActive && !related.tracks.has(track) ? 0.4 : 1);

  const latest = units - 1;
  const playheadIdx = focus?.kind === "envelope" ? focus.idx : latest;

  const inView = (c: Clip) => {
    if (!view.width) return true;
    const cx = x(c.start);
    const cw = Math.max(MIN_CLIP_PX, c.span * ppu);
    return cx + cw >= view.left - 240 && cx <= view.left + view.width + 240;
  };
  const visible = clips.filter(inView);
  const clipByIdx = new Map(clips.map((c) => [c.idx, c]));

  const tickStep = Math.max(1, Math.round(64 / ppu));

  return (
    <div className="h-full flex flex-col bg-[#15191e]">
      <HeadTabs />
      <div className="h-7 px-2 flex items-center gap-1 border-b border-white/10 text-[11px] text-zinc-400 shrink-0">
        <button type="button" className="px-1.5 hover:text-zinc-100" title="Zoom out"
          onClick={() => setPpu((p) => Math.max(MIN_PPU, p / 1.3))}>−</button>
        <button type="button" className="px-1.5 hover:text-zinc-100" title="Zoom in"
          onClick={() => setPpu((p) => Math.min(MAX_PPU, p * 1.3))}>+</button>
        <button type="button" className="px-1.5 hover:text-zinc-100" title="Fit"
          onClick={() => {
            const w = scrollRef.current?.clientWidth ?? 600;
            if (units > 0) setPpu(Math.max(MIN_PPU, Math.min(MAX_PPU, (w - 24) / units)));
          }}>fit</button>
        <span className="ml-1 text-zinc-600">#{units}</span>
        <label className="ml-auto flex items-center gap-1 cursor-pointer select-none">
          <input type="checkbox" checked={showCauses} onChange={(e) => setShowCauses(e.target.checked)} />
          causes
        </label>
      </div>

      <div className="flex-1 min-h-0 flex">
        {/* Fixed gutter: ruler corner + track labels. */}
        <div className="shrink-0 bg-[#11151b] border-r border-white/10" style={{ width: LABEL_WIDTH }}>
          <div className="border-b border-white/10" style={{ height: RULER_HEIGHT }} />
          {rows.map((t) => {
            const ref = { kind: "track" as const, head, track: t.id };
            const focused = focusKey === refKey(ref);
            const colour = TRACK_COLORS[t.kind] ?? "#90a4ae";
            return (
              <div
                key={t.id}
                style={{ height: ROW_HEIGHT, opacity: dimRow(t.id) }}
                className={clsx(
                  "relative flex items-center gap-2 px-2 cursor-pointer border-b border-white/5",
                  focused && "bg-blue-400/10",
                )}
                onPointerEnter={() => setHover(ref)}
                onPointerLeave={() => setHover(null)}
                onClick={(e) => {
                  const r = e.currentTarget.getBoundingClientRect();
                  openDetail(ref, { sink: "panel", anchor: { x: r.left, y: r.top, width: r.width, height: r.height } });
                }}
                data-focusable
              >
                <span className="w-1 h-4 rounded-sm shrink-0" style={{ background: colour }} />
                <span className="min-w-0">
                  <span className="block text-[11px] text-zinc-100 font-medium truncate">{t.label}</span>
                  <span className="block text-[9px] text-zinc-500 truncate">{t.kind}</span>
                </span>
              </div>
            );
          })}
        </div>

        {/* Scrolling lane. */}
        <div
          ref={scrollRef}
          className="flex-1 min-w-0 overflow-x-auto overflow-y-hidden"
          onScroll={(e) => setView({ left: e.currentTarget.scrollLeft, width: e.currentTarget.clientWidth })}
          onWheel={(e) => {
            if (!(e.ctrlKey || e.metaKey)) return;
            e.preventDefault();
            setPpu((p) => Math.max(MIN_PPU, Math.min(MAX_PPU, e.deltaY < 0 ? p * 1.1 : p / 1.1)));
          }}
        >
          <div className="relative" style={{ width: laneWidth, height: bodyHeight }}>
            {/* Ruler. */}
            <div className="absolute top-0 left-0 border-b border-white/10" style={{ width: laneWidth, height: RULER_HEIGHT }}>
              {Array.from({ length: Math.ceil(units / tickStep) + 1 }, (_, k) => k * tickStep).map((u) => (
                <div key={u} className="absolute top-0 h-full text-[9px] text-zinc-600" style={{ left: x(u) }}>
                  <div className="w-px h-full bg-white/10" />
                  <span className="absolute top-0.5 left-1 whitespace-nowrap">{u}</span>
                </div>
              ))}
            </div>

            {/* Row gridlines. */}
            {rows.map((t, i) => (
              <div
                key={t.id}
                className="absolute left-0 border-b border-white/5"
                style={{ top: RULER_HEIGHT + i * ROW_HEIGHT, height: ROW_HEIGHT, width: laneWidth, opacity: dimRow(t.id) }}
              />
            ))}

            {/* Cause arcs (optional). */}
            {showCauses && (
              <svg className="absolute left-0 pointer-events-none" style={{ top: RULER_HEIGHT, width: laneWidth, height: rows.length * ROW_HEIGHT }}>
                {visible.map((c) => {
                  if (c.cause == null) return null;
                  const src = clipByIdx.get(c.cause);
                  if (!src) return null;
                  const y1 = src.row * ROW_HEIGHT + ROW_HEIGHT / 2;
                  const y2 = c.row * ROW_HEIGHT + ROW_HEIGHT / 2;
                  const dim = hoverActive && !(related.envelopes.has(c.idx) && related.envelopes.has(c.cause)) ? 0.12 : 0.5;
                  return (
                    <line key={`a${c.idx}`} x1={x(src.start) + 4} y1={y1} x2={x(c.start) + 2} y2={y2} stroke="#ef5350" strokeWidth={0.8} opacity={dim} />
                  );
                })}
              </svg>
            )}

            {/* Clips. */}
            {visible.map((c) => {
              const ref = { kind: "envelope" as const, head, idx: c.idx };
              const focused = focusKey === refKey(ref);
              const w = Math.max(MIN_CLIP_PX, c.span * ppu - 1);
              return (
                <div
                  key={c.idx}
                  role="button"
                  tabIndex={0}
                  className="absolute rounded-sm overflow-hidden cursor-pointer flex items-center"
                  style={{
                    left: x(c.start),
                    top: RULER_HEIGHT + c.row * ROW_HEIGHT + 4,
                    width: w,
                    height: ROW_HEIGHT - 8,
                    background: c.colour,
                    opacity: dimEnv(c.idx),
                    outline: focused ? "1.5px solid #60a5fa" : "0.5px solid #11151b",
                  }}
                  title={`#${c.idx} ${c.tag}${c.cause != null ? ` · cause #${c.cause}` : ""}`}
                  onPointerEnter={() => setHover(ref)}
                  onPointerLeave={() => setHover(null)}
                  onClick={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    openDetail(ref, { sink: "popover", anchor: { x: r.left, y: r.top, width: r.width, height: r.height } });
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    openContextMenu(
                      [
                        { label: `Snapshot @ #${c.idx}`, onSelect: () => snapshotPlay({ head, label: `at #${c.idx}` }) },
                        {
                          label: `Fork from #${c.idx}`,
                          onSelect: () => {
                            snapshotPlay({ head, label: `fork base @ #${c.idx}` });
                            forkPlay({ parent: head });
                          },
                        },
                        { label: "Open in detail panel", onSelect: () => openDetail(ref, { sink: "panel" }) },
                      ],
                      { x: e.clientX, y: e.clientY },
                    );
                  }}
                  data-focusable
                >
                  {w > 28 && (
                    <span className="px-1 text-[9px] text-black/80 font-medium truncate">{c.label}</span>
                  )}
                </div>
              );
            })}

            {/* Playhead. */}
            {playheadIdx >= 0 && (
              <div className="absolute top-0 pointer-events-none" style={{ left: x(playheadIdx) + 1, height: bodyHeight }}>
                <div className="w-px h-full bg-rose-400/70" />
                <div className="absolute -top-0 -left-1 w-2 h-2 rotate-45 bg-rose-400" />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Head tabs strip ────────────────────────────────────────────────

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
            title={h.parent ? `forked from ${h.parent}${h.forkedFrom ? ` @ ${h.forkedFrom}` : ""}` : "primary head"}
          >
            <span>{h.id}</span>
            {h.ended && <span className="text-zinc-500 text-[10px]">end</span>}
            {h.choices.length > 0 && <span className="text-emerald-400 text-[10px]">●</span>}
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
          if (focus?.kind === "envelope") {
            snapshotPlay({ head: primary, label: `fork @ #${focus.idx}` });
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
