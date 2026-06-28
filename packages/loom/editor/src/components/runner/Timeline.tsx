// Phase 3/6 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// §13): the Timeline — Run facet. Clips on tracks with a ruler, zoom,
// playhead, viewport culling, a **clock axis** (story time, when the
// runtime stamps `meta.clock`) toggleable with a ledger-index axis, and
// **multi-head lanes** (one lane per live head) toggleable with the
// per-track view of the primary head.
//
// Beats are no longer drawn as wide bars on the Main row (where they
// buried the point events underneath); they ride a dedicated **Beats
// band** under the ruler, as the story spine every track row reads
// against. Each track row now shows only its own envelopes — dialogue
// lands on the speaker's row (runtime attributes it there), so you can
// see which characters are interacting. A multi-speaker cue draws an
// **interaction link** connecting the speakers' rows at that moment.

import { useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import {
  eventTag,
  EVENT_COLORS,
  TRACK_COLORS,
  type LedgerEvent,
} from "./event-format";
import { useFocus, useRelated, refKey, useEffectiveFocus } from "@/store/focus";
import type { PlayEnvelopeMeta } from "@/lib/sync";
import { useSession } from "@/store/session";
import { openContextMenu } from "@/store/context-menu";

const ROW_HEIGHT = 30;
const BAND_HEIGHT = 26;
const LABEL_WIDTH = 150;
const RULER_HEIGHT = 22;
const MIN_PPU = 0.05;
const MAX_PPU = 120;
const MIN_CLIP_PX = 6;

type AxisMode = "clock" | "index";
type LanesMode = "primary" | "all";

type Clip = {
  idx: number;
  head: string;
  row: number;
  startU: number;
  spanU: number;
  colour: string;
  tag: string;
  label: string;
  cause: number | null;
  /** Multi-speaker cue → the other rows it links to (interaction). */
  speakers?: string[];
};

type BeatSeg = {
  idx: number;
  head: string;
  startU: number;
  endU: number;
  label: string;
};

type Row = {
  key: string;
  kind: "track" | "head";
  label: string;
  sub: string;
  colour: string;
  headId: string;
  track?: number;
  forkAt?: number | null;
  isPrimary?: boolean;
};

function clipLabel(event: LedgerEvent): string {
  const [tag, body] = eventTag(event);
  switch (tag) {
    case "BeatEntered":
      return String(body.beat ?? "beat");
    case "Dialogue": {
      const text = String(body.text ?? "").trim();
      if (text) return text;
      const speakers = (body.speakers as string[]) ?? [body.speaker as string];
      return String(speakers?.[0] ?? "line");
    }
    case "Diverted":
    case "Tunneled":
      return `→ ${body.target ?? body.beat ?? ""}`;
    case "ChoiceTaken":
      return `▸ ${body.text ?? ""}`;
    case "WorldSet":
      return String(body.path ?? body.key ?? "set");
    case "KnowledgeChanged":
      return `${body.character ?? "?"}.${body.field ?? "?"}`;
    case "Action":
      return "action";
    case "Scene":
      return "scene";
    default:
      return tag;
  }
}

/** Forward-fill the story clock so every envelope has an effective time. */
function effClocks(meta: PlayEnvelopeMeta[]): number[] {
  const out: number[] = [];
  let last = 0;
  for (let i = 0; i < meta.length; i++) {
    const c = meta[i]?.clock;
    if (c != null) last = c;
    out[i] = last;
  }
  return out;
}

function fmtClock(mins: number): string {
  const hh = Math.floor(mins / 60) % 24;
  const mm = Math.round(mins) % 60;
  return `${hh}:${String(mm).padStart(2, "0")}`;
}

const CLOCK_STEPS = [15, 30, 60, 120, 180, 240, 360, 720];

export function TimelinePanel() {
  const play = useSession((s) => s.active?.play) ?? null;
  const related = useRelated();
  const focus = useEffectiveFocus();
  const hover = useFocus((s) => s.hover);
  const setHover = useFocus((s) => s.setHover);
  const openDetail = useFocus((s) => s.openDetail);
  const snapshotPlay = useSession((s) => s.snapshotPlay);
  const forkPlay = useSession((s) => s.forkPlay);
  const setPrimaryHead = useSession((s) => s.setPrimaryHead);

  const [ppu, setPpu] = useState(16);
  const [axisMode, setAxisMode] = useState<AxisMode>("clock");
  const [lanesMode, setLanesMode] = useState<LanesMode>("primary");
  const [showCauses, setShowCauses] = useState(false);
  const [showLinks, setShowLinks] = useState(true);
  const [view, setView] = useState({ left: 0, width: 0 });
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const primaryId = play?.primary ?? "";

  const built = useMemo(() => {
    const empty = {
      rows: [] as Row[],
      clips: [] as Clip[],
      beats: [] as BeatSeg[],
      maxU: 0,
      clockAvailable: false,
      useClock: false,
      posFor: (_h: string, i: number) => i,
    };
    if (!play) return empty;
    const shown =
      lanesMode === "all" ? play.heads : play.heads.filter((h) => h.id === play.primary);
    const clockVals = new Set<number>();
    for (const h of shown)
      for (const m of h.meta ?? []) if (m?.clock != null) clockVals.add(m.clock);
    const clockAvailable = clockVals.size >= 2;
    const useClock = axisMode === "clock" && clockAvailable;

    const effByHead = new Map<string, number[]>();
    for (const h of shown) effByHead.set(h.id, effClocks(h.meta ?? []));
    const posFor = (head: string, i: number) => {
      if (!useClock) return i;
      const e = effByHead.get(head);
      return e ? (e[i] ?? (e.length ? e[e.length - 1] : 0)) : i;
    };

    const snapById = new Map(play.snapshots.map((s) => [s.id, s]));
    const rows: Row[] = [];
    const clips: Clip[] = [];
    const beats: BeatSeg[] = [];
    let rowIdx = 0;
    let maxU = 0;

    for (const h of shown) {
      const events = h.transcript as LedgerEvent[];
      const meta: PlayEnvelopeMeta[] = h.meta ?? [];
      const posOf = (i: number) => posFor(h.id, i);
      const beatStops: number[] = [];
      for (let i = 0; i < events.length; i++)
        if (eventTag(events[i])[0] === "BeatEntered") beatStops.push(i);
      const lastPos = events.length ? posOf(events.length - 1) : 0;
      const endPosOf = (i: number) => {
        const nb = beatStops.find((b) => b > i);
        return nb != null ? posOf(nb) : lastPos + (useClock ? 0 : 1);
      };

      if (lanesMode === "primary") {
        const rowOf = new Map<number, number>();
        for (const t of h.tracks ?? []) {
          rowOf.set(t.id, rowIdx);
          rows.push({
            key: `${h.id}:${t.id}`,
            kind: "track",
            label: t.label,
            sub: t.kind,
            colour: TRACK_COLORS[t.kind] ?? "#90a4ae",
            headId: h.id,
            track: t.id,
          });
          rowIdx++;
        }
        for (let i = 0; i < events.length; i++) {
          const m = meta[i];
          if (!m) continue;
          const [tag, body] = eventTag(events[i]);
          const startU = posOf(i);
          // Beats become the spine band, not a row clip — so they no
          // longer bury the point events sharing the Main row.
          if (tag === "BeatEntered") {
            beats.push({
              idx: i,
              head: h.id,
              startU,
              endU: Math.max(startU, endPosOf(i)),
              label: String(body.beat ?? "beat"),
            });
            maxU = Math.max(maxU, endPosOf(i));
            continue;
          }
          const r = rowOf.get(m.track);
          if (r == null) continue;
          const speakers =
            tag === "Dialogue"
              ? ((body.speakers as string[]) ?? []).filter(Boolean)
              : undefined;
          clips.push({
            idx: i,
            head: h.id,
            row: r,
            startU,
            spanU: 0,
            colour: EVENT_COLORS[tag] ?? "#78909c",
            tag,
            label: clipLabel(events[i]),
            cause: m.cause,
            speakers: speakers && speakers.length > 1 ? speakers : undefined,
          });
          maxU = Math.max(maxU, startU);
        }
      } else {
        const r = rowIdx;
        const isPrimary = h.id === play.primary;
        const forkAt = h.forkedFrom ? (snapById.get(h.forkedFrom)?.at ?? null) : null;
        rows.push({
          key: h.id,
          kind: "head",
          label: h.id,
          sub: isPrimary ? "primary" : h.parent ? `fork of ${h.parent}` : "",
          colour: isPrimary ? "#60a5fa" : "#7c4dff",
          headId: h.id,
          forkAt,
          isPrimary,
        });
        rowIdx++;
        for (let i = 0; i < events.length; i++) {
          const m = meta[i];
          if (!m) continue;
          const [tag] = eventTag(events[i]);
          const startU = posOf(i);
          clips.push({
            idx: i,
            head: h.id,
            row: r,
            startU,
            spanU: 0,
            colour: EVENT_COLORS[tag] ?? "#78909c",
            tag,
            label: "",
            cause: m.cause,
          });
          maxU = Math.max(maxU, startU);
        }
      }
    }
    return { rows, clips, beats, maxU, clockAvailable, useClock, posFor };
  }, [play, axisMode, lanesMode]);

  // Auto-fit when the axis flips (clock & index need very different ppu).
  useEffect(() => {
    const w = scrollRef.current?.clientWidth ?? 0;
    if (w > 0 && built.maxU > 0) {
      setPpu(Math.max(MIN_PPU, Math.min(MAX_PPU, (w - 28) / built.maxU)));
    }
  }, [axisMode, built.maxU]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setView({ left: el.scrollLeft, width: el.clientWidth });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  if (!play || built.rows.length === 0) {
    return (
      <div className="h-full flex flex-col bg-[#15191e]">
        <HeadTabs />
        <div className="flex-1 grid place-items-center text-zinc-500 text-xs px-4">
          Timeline is empty — start a play session.
        </div>
      </div>
    );
  }

  const { rows, clips, beats, maxU, clockAvailable, useClock, posFor } = built;
  const hasBand = lanesMode === "primary" && beats.length > 0;
  const bandH = hasBand ? BAND_HEIGHT : 0;
  const laneWidth = Math.max(view.width || 0, maxU * ppu + 48);
  const rowsTop = RULER_HEIGHT + bandH;
  const bodyHeight = rowsTop + rows.length * ROW_HEIGHT;
  const x = (u: number) => u * ppu;
  const focusKey = refKey(focus);
  const dimEnabled = lanesMode === "primary" && hover !== null;
  const dimEnv = (idx: number) => (dimEnabled && !related.envelopes.has(idx) ? 0.25 : 1);

  // Speaker label (case-insensitive) → row index, for interaction links.
  const rowByLabel = new Map<string, number>();
  rows.forEach((r, i) => {
    if (r.kind === "track") rowByLabel.set(r.label.toLowerCase(), i);
  });
  const rowCenter = (r: number) => rowsTop + r * ROW_HEIGHT + ROW_HEIGHT / 2;

  const inView = (startU: number, spanU: number) => {
    if (!view.width) return true;
    const cx = x(startU);
    const cw = Math.max(MIN_CLIP_PX, spanU * ppu);
    return cx + cw >= view.left - 240 && cx <= view.left + view.width + 240;
  };
  const visible = clips.filter((c) => inView(c.startU, c.spanU));
  const visibleBeats = beats.filter((b) => inView(b.startU, b.endU - b.startU));
  const clipByKey = new Map(clips.map((c) => [`${c.head}:${c.idx}`, c]));

  // Playhead at the focused envelope (or the primary head's latest).
  let playheadU: number | null = null;
  if (focus?.kind === "envelope") playheadU = posFor(focus.head, focus.idx);
  else {
    const ph = play.heads.find((h) => h.id === primaryId);
    if (ph && ph.transcript.length) playheadU = posFor(primaryId, ph.transcript.length - 1);
  }

  // Ruler ticks.
  const ticks: { u: number; label: string }[] = [];
  if (useClock) {
    const step = CLOCK_STEPS.find((s) => s * ppu >= 64) ?? 720;
    const start = Math.floor(0 / step) * step;
    for (let u = start; u <= maxU + step; u += step) ticks.push({ u, label: fmtClock(u) });
  } else {
    const step = Math.max(1, Math.round(64 / ppu));
    for (let u = 0; u <= maxU + step; u += step) ticks.push({ u, label: String(u) });
  }

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
            if (maxU > 0) setPpu(Math.max(MIN_PPU, Math.min(MAX_PPU, (w - 28) / maxU)));
          }}>fit</button>
        <span className="mx-1 w-px h-3 bg-white/10" />
        <button type="button"
          className={clsx("px-1.5 rounded", clockAvailable ? "hover:text-zinc-100" : "text-zinc-700 cursor-default")}
          disabled={!clockAvailable}
          title={clockAvailable ? "Toggle clock / index axis" : "No story clock in this session"}
          onClick={() => setAxisMode((m) => (m === "clock" ? "index" : "clock"))}>
          {useClock ? "⏱ clock" : "# index"}
        </button>
        <button type="button" className="px-1.5 rounded hover:text-zinc-100"
          title="Toggle multi-head lanes / primary tracks"
          onClick={() => setLanesMode((m) => (m === "all" ? "primary" : "all"))}>
          {lanesMode === "all" ? "▤ heads" : "▦ tracks"}
        </button>
        {lanesMode === "primary" && (
          <span className="ml-auto flex items-center gap-3">
            <label className="flex items-center gap-1 cursor-pointer select-none" title="Connect co-speaking tracks">
              <input type="checkbox" checked={showLinks} onChange={(e) => setShowLinks(e.target.checked)} />
              links
            </label>
            <label className="flex items-center gap-1 cursor-pointer select-none" title="Draw cause → effect arcs">
              <input type="checkbox" checked={showCauses} onChange={(e) => setShowCauses(e.target.checked)} />
              causes
            </label>
          </span>
        )}
      </div>

      <div className="flex-1 min-h-0 flex">
        {/* Fixed gutter. */}
        <div className="shrink-0 bg-[#11151b] border-r border-white/10" style={{ width: LABEL_WIDTH }}>
          <div className="border-b border-white/10" style={{ height: RULER_HEIGHT }} />
          {hasBand && (
            <div className="flex items-center px-2 border-b border-white/10 bg-white/[0.03]" style={{ height: BAND_HEIGHT }}>
              <span className="text-[10px] uppercase tracking-wide text-zinc-500">Beats</span>
            </div>
          )}
          {rows.map((row) => {
            if (row.kind === "head") {
              const active = row.isPrimary;
              return (
                <div key={row.key} style={{ height: ROW_HEIGHT }}
                  className={clsx("relative flex items-center gap-2 px-2 cursor-pointer border-b border-white/5",
                    active ? "bg-blue-400/10" : "hover:bg-white/5")}
                  onClick={() => setPrimaryHead(row.headId)}
                  title={`Make ${row.headId} the primary head`}>
                  <span className="w-1 h-4 rounded-sm shrink-0" style={{ background: row.colour }} />
                  <span className="min-w-0">
                    <span className="block text-[11px] text-zinc-100 font-medium truncate">
                      {active && "★ "}{row.label}
                    </span>
                    <span className="block text-[9px] text-zinc-500 truncate">{row.sub}</span>
                  </span>
                </div>
              );
            }
            const ref = { kind: "track" as const, head: row.headId, track: row.track! };
            const focused = focusKey === refKey(ref);
            return (
              <div key={row.key} style={{ height: ROW_HEIGHT }}
                className={clsx("relative flex items-center gap-2 px-2 cursor-pointer border-b border-white/5", focused && "bg-blue-400/10")}
                onPointerEnter={() => setHover(ref)}
                onPointerLeave={() => setHover(null)}
                onClick={(e) => {
                  const r = e.currentTarget.getBoundingClientRect();
                  openDetail(ref, { sink: "panel", anchor: { x: r.left, y: r.top, width: r.width, height: r.height } });
                }}
                data-focusable>
                <span className="w-1 h-4 rounded-sm shrink-0" style={{ background: row.colour }} />
                <span className="min-w-0">
                  <span className="block text-[11px] text-zinc-100 font-medium truncate">{row.label}</span>
                  <span className="block text-[9px] text-zinc-500 truncate">{row.sub}</span>
                </span>
              </div>
            );
          })}
        </div>

        {/* Scrolling lane. */}
        <div ref={scrollRef} className="flex-1 min-w-0 overflow-x-auto overflow-y-hidden"
          onScroll={(e) => setView({ left: e.currentTarget.scrollLeft, width: e.currentTarget.clientWidth })}
          onWheel={(e) => {
            if (!(e.ctrlKey || e.metaKey)) return;
            e.preventDefault();
            setPpu((p) => Math.max(MIN_PPU, Math.min(MAX_PPU, e.deltaY < 0 ? p * 1.1 : p / 1.1)));
          }}>
          <div className="relative" style={{ width: laneWidth, height: bodyHeight }}>
            {/* Ruler. */}
            <div className="absolute top-0 left-0 border-b border-white/10" style={{ width: laneWidth, height: RULER_HEIGHT }}>
              {ticks.map((t) => (
                <div key={t.u} className="absolute top-0 h-full text-[9px] text-zinc-600" style={{ left: x(t.u) }}>
                  <div className="w-px h-full bg-white/10" />
                  <span className="absolute top-0.5 left-1 whitespace-nowrap">{t.label}</span>
                </div>
              ))}
            </div>

            {/* Beats spine band. */}
            {hasBand && (
              <div className="absolute left-0 border-b border-white/10 bg-white/[0.02]"
                style={{ top: RULER_HEIGHT, height: BAND_HEIGHT, width: laneWidth }}>
                {visibleBeats.map((b) => {
                  const ref = { kind: "envelope" as const, head: b.head, idx: b.idx };
                  const focused = focusKey === refKey(ref);
                  const w = Math.max(MIN_CLIP_PX, (b.endU - b.startU) * ppu - 1);
                  return (
                    <div key={`${b.head}:${b.idx}`} role="button" tabIndex={0}
                      className="absolute top-1 bottom-1 rounded-sm overflow-hidden cursor-pointer flex items-center"
                      style={{
                        left: x(b.startU),
                        width: w,
                        background: "#3b3f73",
                        outline: focused ? "1.5px solid #60a5fa" : "0.5px solid #11151b",
                        opacity: dimEnv(b.idx),
                      }}
                      title={`beat ${b.label}`}
                      onPointerEnter={() => setHover(ref)}
                      onPointerLeave={() => setHover(null)}
                      onClick={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        openDetail(ref, { sink: "popover", anchor: { x: r.left, y: r.top, width: r.width, height: r.height } });
                      }}
                      data-focusable>
                      {w > 28 && (
                        <span className="px-1 text-[9px] text-indigo-100 font-medium truncate">{b.label}</span>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {/* Row gridlines + fork markers. */}
            {rows.map((row, i) => (
              <div key={row.key} className="absolute left-0 border-b border-white/5"
                style={{ top: rowsTop + i * ROW_HEIGHT, height: ROW_HEIGHT, width: laneWidth }}>
                {row.kind === "head" && row.forkAt != null && (
                  <div className="absolute" style={{ left: x(posFor(row.headId, row.forkAt)) - 4, top: ROW_HEIGHT / 2 - 4 }}
                    title={`forked at #${row.forkAt}`}>
                    <div className="w-2 h-2 rotate-45 bg-purple-400" />
                  </div>
                )}
              </div>
            ))}

            {/* Interaction links (co-speaker) + cause arcs. */}
            {(showLinks || showCauses) && lanesMode === "primary" && (
              <svg className="absolute left-0 pointer-events-none" style={{ top: 0, width: laneWidth, height: bodyHeight }}>
                {showCauses && visible.map((c) => {
                  if (c.cause == null) return null;
                  const src = clipByKey.get(`${c.head}:${c.cause}`);
                  if (!src) return null;
                  return (
                    <line key={`a${c.idx}`} x1={x(src.startU) + 4} y1={rowCenter(src.row)}
                      x2={x(c.startU) + 2} y2={rowCenter(c.row)}
                      stroke="#ef5350" strokeWidth={0.8} opacity={0.4} />
                  );
                })}
                {showLinks && visible.map((c) => {
                  if (!c.speakers) return null;
                  const rowsOf = c.speakers
                    .map((s) => rowByLabel.get(s.toLowerCase()))
                    .filter((r): r is number => r != null);
                  if (rowsOf.length < 2) return null;
                  const lo = Math.min(...rowsOf);
                  const hi = Math.max(...rowsOf);
                  const cx = x(c.startU) + 1;
                  return (
                    <g key={`l${c.idx}`}>
                      <line x1={cx} y1={rowCenter(lo)} x2={cx} y2={rowCenter(hi)}
                        stroke="#34d399" strokeWidth={1} opacity={dimEnv(c.idx) * 0.7} />
                      {rowsOf.map((r) => (
                        <circle key={r} cx={cx} cy={rowCenter(r)} r={2.2} fill="#34d399" opacity={dimEnv(c.idx) * 0.9} />
                      ))}
                    </g>
                  );
                })}
              </svg>
            )}

            {/* Clips. */}
            {visible.map((c) => {
              const ref = { kind: "envelope" as const, head: c.head, idx: c.idx };
              const focused = focusKey === refKey(ref);
              const w = Math.max(MIN_CLIP_PX, c.spanU * ppu - 1);
              return (
                <div key={`${c.head}:${c.idx}`} role="button" tabIndex={0}
                  className="absolute rounded-sm overflow-hidden cursor-pointer flex items-center"
                  style={{
                    left: x(c.startU),
                    top: rowsTop + c.row * ROW_HEIGHT + 4,
                    width: w,
                    height: ROW_HEIGHT - 8,
                    background: c.colour,
                    opacity: dimEnv(c.idx),
                    outline: focused ? "1.5px solid #60a5fa" : "0.5px solid #11151b",
                  }}
                  title={`${c.head} #${c.idx} ${c.tag}${c.cause != null ? ` · cause #${c.cause}` : ""}`}
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
                        { label: `Snapshot @ #${c.idx}`, onSelect: () => snapshotPlay({ head: c.head, label: `at #${c.idx}` }) },
                        {
                          label: `Fork from #${c.idx}`,
                          onSelect: () => {
                            snapshotPlay({ head: c.head, label: `fork base @ #${c.idx}` });
                            forkPlay({ parent: c.head });
                          },
                        },
                        { label: "Open in detail panel", onSelect: () => openDetail(ref, { sink: "panel" }) },
                      ],
                      { x: e.clientX, y: e.clientY },
                    );
                  }}
                  data-focusable>
                  {w > 28 && c.label && (
                    <span className="px-1 text-[9px] text-black/80 font-medium truncate">{c.label}</span>
                  )}
                </div>
              );
            })}

            {/* Playhead. */}
            {playheadU != null && (
              <div className="absolute top-0 pointer-events-none" style={{ left: x(playheadU) + 1, height: bodyHeight }}>
                <div className="w-px h-full bg-rose-400/70" />
                <div className="absolute -left-1 w-2 h-2 rotate-45 bg-rose-400" />
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
          <button key={h.id} type="button" onClick={() => setPrimaryHead(h.id)}
            className={clsx("h-6 px-2 rounded flex items-center gap-1 border whitespace-nowrap",
              active ? "border-blue-400/40 bg-blue-400/10 text-blue-200" : "border-white/10 text-zinc-400 hover:text-zinc-200 hover:border-white/20")}
            title={h.parent ? `forked from ${h.parent}${h.forkedFrom ? ` @ ${h.forkedFrom}` : ""}` : "primary head"}>
            <span>{h.id}</span>
            {h.ended && <span className="text-zinc-500 text-[10px]">end</span>}
            {h.choices.length > 0 && <span className="text-emerald-400 text-[10px]">●</span>}
            {h.id !== "h0" && (
              <span role="button" tabIndex={0} aria-label={`Drop ${h.id}`}
                onClick={(e) => { e.stopPropagation(); dropHead(h.id); }}
                className="ml-1 text-zinc-600 hover:text-rose-400">×</span>
            )}
          </button>
        );
      })}
      <button type="button"
        onClick={() => {
          if (focus?.kind === "envelope") snapshotPlay({ head: primary, label: `fork @ #${focus.idx}` });
          forkPlay({ parent: primary });
        }}
        className="h-6 px-2 rounded border border-emerald-400/30 text-emerald-300 hover:bg-emerald-400/10 ml-1 whitespace-nowrap">
        + fork
      </button>
      <button type="button" onClick={() => snapshotPlay({ head: primary })}
        className="h-6 px-2 rounded border border-white/10 text-zinc-400 hover:text-zinc-200 hover:border-white/20 whitespace-nowrap">
        snapshot
      </button>
      <span className="ml-auto text-zinc-600">{play.heads.length} head{play.heads.length === 1 ? "" : "s"}</span>
    </header>
  );
}
