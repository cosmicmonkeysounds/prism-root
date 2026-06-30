//! Channel plumbing shared by the guest + performer apps: grouping a flat
//! message map into threads, deriving channel headers from ids, and the
//! unread / active-thread bookkeeping that makes the list feel like a
//! Discord / Telegram inbox (badges, decision-pulls, last-seen marks).

import { useEffect, useRef, useState } from "react";
import type { Channel, ChannelKind, ChatMessage } from "./types.ts";

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
  activeId: string | null;
  active: Channel | null;
  open: (id: string) => void;
  back: () => void;
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
    activeId,
    active: list.find((c) => c.id === activeId) ?? active,
    open: (id) => setActiveId(id),
    back: () => setActiveId(null),
  };
}
