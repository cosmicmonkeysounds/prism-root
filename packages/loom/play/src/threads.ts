//! Channel plumbing shared by the guest + performer apps: grouping a flat
//! message map into threads, deriving channel headers from ids, and the
//! unread / active-thread bookkeeping that makes the list feel like a
//! Discord / Telegram inbox (badges, decision-pulls, last-seen marks).

import { useEffect, useRef, useState } from "react";
import type { Channel, ChannelKind, ChatMessage, MessageKind } from "./types.ts";

/** `MODERATOR_PRIME` / `Recruiter` → a friendly contact name. */
export function prettyName(raw: string): string {
  return raw
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((w) => w[0]!.toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

/** Title + kind for a channel id, used for threads with no messages yet. */
export function channelHead(id: string): { kind: ChannelKind; title: string } {
  if (id === "lobby") return { kind: "lobby", title: "The Internet" };
  if (id.startsWith("faction:")) return { kind: "faction", title: `#${id.slice(8).toLowerCase()}` };
  if (id.startsWith("dm:")) return { kind: "dm", title: prettyName(id.slice(3)) };
  return { kind: "dm", title: id };
}

// --- spaces (the Discord-style sidebar sections) ----------------------------

/** Sidebar space metadata. `internet` holds the room; the booth groups the
 *  performer's tools + per-guest threads. (Authoring adds more later.) */
// Built-in sections. Authored spaces (any other id) sort between `internet`
// and the booth, titled from the channel's `spaceTitle`.
export const SPACES: Record<string, { title: string; order: number }> = {
  internet: { title: "The Internet", order: 0 },
  booth: { title: "Booth", order: 10 },
  guests: { title: "Guests", order: 11 },
};
export function spaceTitle(id: string): string {
  return SPACES[id]?.title ?? id;
}
function spaceOrder(id: string): number {
  return SPACES[id]?.order ?? 1; // authored spaces: after internet, before booth
}

export interface SpaceGroup {
  id: string;
  title: string;
  channels: Channel[];
}

/**
 * Fold an already-sorted channel list into ordered space sections, preserving
 * the within-list order (decisions-first / most-recent) inside each space.
 * An authored section's title rides on its channels' `spaceTitle`.
 */
export function groupBySpace(list: Channel[]): SpaceGroup[] {
  const byId = new Map<string, Channel[]>();
  for (const c of list) {
    const arr = byId.get(c.spaceId) ?? [];
    arr.push(c);
    byId.set(c.spaceId, arr);
  }
  return [...byId.entries()]
    .map(([id, channels]) => ({ id, title: channels[0]?.spaceTitle ?? spaceTitle(id), channels }))
    .sort((a, b) => spaceOrder(a.id) - spaceOrder(b.id));
}

// --- sender-run grouping (the Slack/Discord consecutive-sender banner) -------

export interface MessageRun {
  from: string;
  kind: MessageKind;
  messages: ChatMessage[];
}

/** Consecutive `line`s from one sender within this window share a banner. */
export const RUN_WINDOW_MS = 5 * 60 * 1000; // story-clock ms (deterministic)

/**
 * Coalesce consecutive same-sender messages into runs. A new run starts when
 * the sender changes, the kind isn't `line` (narration / system / signal each
 * stand alone), or the story-clock gap exceeds `RUN_WINDOW_MS`.
 */
export function groupRuns(messages: ChatMessage[]): MessageRun[] {
  const runs: MessageRun[] = [];
  for (const m of messages) {
    const last = runs[runs.length - 1];
    const prev = last?.messages[last.messages.length - 1];
    const sameRun =
      last !== undefined &&
      prev !== undefined &&
      m.kind === "line" &&
      last.kind === "line" &&
      last.from === m.from &&
      m.ts - prev.ts < RUN_WINDOW_MS;
    if (sameRun) last!.messages.push(m);
    else runs.push({ from: m.from, kind: m.kind, messages: [m] });
  }
  return runs;
}

// --- threads (Slack-style replies under a root message) ---------------------

/** Top-level messages of a channel (replies are tucked into their thread). */
export function rootsOf(messages: ChatMessage[]): ChatMessage[] {
  return messages.filter((m) => m.parentSeq == null);
}
/** Replies hanging under a root message, seq-ordered. */
export function repliesFor(messages: ChatMessage[], rootSeq: number): ChatMessage[] {
  return messages.filter((m) => m.parentSeq === rootSeq).sort((a, b) => a.seq - b.seq);
}
/** How many replies a root message has. */
export function replyCountFor(messages: ChatMessage[], rootSeq: number): number {
  let n = 0;
  for (const m of messages) if (m.parentSeq === rootSeq) n += 1;
  return n;
}

/** Group a message map into per-channel, seq-ordered buckets. */
export function groupByChannel(messages: Map<number, ChatMessage>): Map<string, ChatMessage[]> {
  const out = new Map<string, ChatMessage[]>();
  for (const m of [...messages.values()].sort((a, b) => a.seq - b.seq)) {
    const bucket = out.get(m.channel) ?? [];
    bucket.push(m);
    out.set(m.channel, bucket);
  }
  return out;
}

/** The highest seq across a channel's messages (−1 when empty). */
function maxSeq(c: Channel): number {
  return c.messages.length ? c.messages[c.messages.length - 1]!.seq : -1;
}

export interface Threads {
  /** Channels, decisions + unread first, then most-recent — ready to render. */
  list: Channel[];
  /** The same channels folded into ordered Discord-style space sections. */
  spaces: SpaceGroup[];
  activeId: string | null;
  active: Channel | null;
  open: (id: string) => void;
  back: () => void;
  /** The root `seq` of the open Slack-style thread panel, if any. */
  activeThreadRoot: number | null;
  openThread: (rootSeq: number) => void;
  closeThread: () => void;
}

/**
 * Layer unread counts + open/close navigation over a freshly-built channel
 * set. A thread is "unread" when it has messages past the last-seen mark or
 * a pending decision; pending decisions also pin it to the top so an
 * unanswered choice always pulls focus.
 */
export function useThreads(channels: Channel[]): Threads {
  const [readMarks, setReadMarks] = useState<Map<string, number>>(new Map());
  const [activeId, setActiveId] = useState<string | null>(null);
  const [activeThreadRoot, setActiveThreadRoot] = useState<number | null>(null);

  const active = channels.find((c) => c.id === activeId) ?? null;
  const activeMax = active ? maxSeq(active) : -1;

  // Reading a thread (or new messages landing while it's open) advances its
  // last-seen mark, clearing the badge.
  useEffect(() => {
    if (activeId === null) return;
    setReadMarks((prev) => {
      if (prev.get(activeId) === activeMax) return prev;
      const next = new Map(prev);
      next.set(activeId, activeMax);
      return next;
    });
  }, [activeId, activeMax]);

  const list = channels
    .map((c) => {
      const seen = readMarks.get(c.id) ?? -1;
      const fresh = c.messages.filter((m) => m.seq > seen).length;
      return { ...c, unread: c.decision ? Math.max(1, fresh) : fresh };
    })
    .sort((a, b) => {
      if (!!a.decision !== !!b.decision) return a.decision ? -1 : 1;
      return b.lastTs - a.lastTs;
    });

  return {
    list,
    spaces: groupBySpace(list),
    activeId,
    active: list.find((c) => c.id === activeId) ?? active,
    open: (id) => {
      setActiveId(id);
      setActiveThreadRoot(null); // opening a channel closes any thread panel
    },
    back: () => {
      setActiveId(null);
      setActiveThreadRoot(null);
    },
    activeThreadRoot,
    openThread: (rootSeq) => setActiveThreadRoot(rootSeq),
    closeThread: () => setActiveThreadRoot(null),
  };
}
