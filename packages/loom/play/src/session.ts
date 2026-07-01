//! Role sessions. Each opens the SSE stream for its participant (via
//! `useChatStream`), keeps the authoritative status snapshot, exposes the
//! role's actions, and composes the live message map into conversation
//! threads (`useThreads`). The guest and performer both build on the same
//! plumbing — only their channel-shaping + actions differ.

import { useCallback, useRef, useState } from "react";
import { api, useChatStream } from "./client.ts";
import { channelHead, groupByChannel, useThreads, type Threads } from "./threads.ts";
import type { Channel, ChatMessage, Decision, GuestView, PrimeView } from "./types.ts";

// --- persistence ------------------------------------------------------------

interface GuestId {
  id: string;
  name: string;
  /** Which event this guest joined — every call is scoped to `/e/:eventId`. */
  eventId: string;
}
interface PrimeAuth {
  token: string;
  character: string;
  admin: boolean;
  /** Which event this performer signed into. */
  eventId: string;
}
const load = <T,>(k: string): T | null => {
  try {
    return JSON.parse(localStorage.getItem(k) ?? "null") as T | null;
  } catch {
    return null;
  }
};
const save = (k: string, v: unknown): void => localStorage.setItem(k, JSON.stringify(v));
const drop = (k: string): void => localStorage.removeItem(k);
const GK = "loom.guest";
const PK = "loom.prime";

// --- guest ------------------------------------------------------------------

/** The one required decision (if any), and the channel it should dock under. */
function requiredDecision(
  status: GuestView | null,
  acts: { choose: (i: number) => void; join: (f: string) => void; escape: () => void },
): { channel: string; decision: Decision } | null {
  if (!status) return null;
  if (status.pendingChoice) {
    return {
      channel: status.decisionChannel ?? "lobby",
      decision: {
        title: "Your move…",
        options: status.pendingChoice.map((opt, i) => ({ label: opt, onClick: () => acts.choose(i) })),
      },
    };
  }
  if (!status.faction && !status.captured) {
    return {
      channel: "lobby",
      decision: {
        title: "Choose your side",
        options: [
          { label: "🛡️ Side with the Mods", onClick: () => acts.join("Mods") },
          { label: "💬 Side with the Chatters", onClick: () => acts.join("Chatters"), tone: "ghost" },
        ],
      },
    };
  }
  if (status.captured) {
    return {
      channel: "lobby",
      decision: {
        title: "You're trapped in the Internet",
        options: [{ label: "🏃 Make a break for it", onClick: () => acts.escape(), tone: "danger" }],
      },
    };
  }
  return null;
}

/** Group the guest's messages into threads, docking the pending decision, and
 *  merge in authored channels (SPACE/CHANNEL) the guest can see — including
 *  empty rooms — from the server snapshot. */
function buildGuestChannels(
  messages: Map<number, ChatMessage>,
  dock: { channel: string; decision: Decision } | null,
  view: GuestView | null,
): Channel[] {
  const groups = groupByChannel(messages);
  if (!groups.has("lobby")) groups.set("lobby", []); // the lobby is always present
  if (dock && !groups.has(dock.channel)) groups.set(dock.channel, []);
  const spaceTitles = new Map((view?.spaces ?? []).map((s) => [s.id, s.title]));
  const snap = new Map((view?.channels ?? []).map((c) => [c.id, c]));
  for (const id of snap.keys()) if (!groups.has(id)) groups.set(id, []); // empty authored rooms
  return [...groups.entries()].map(([id, msgs]) => {
    const lastTs = msgs.length ? msgs[msgs.length - 1]!.ts : 0;
    const decision = dock && dock.channel === id ? dock.decision : null;
    const s = snap.get(id);
    if (s !== undefined) {
      // An authored channel — title/kind/space are server-authoritative.
      return {
        id,
        kind: s.kind as Channel["kind"],
        title: s.title,
        spaceId: s.spaceId,
        spaceTitle: spaceTitles.get(s.spaceId),
        member: s.member,
        canPost: s.canPost,
        threadable: s.threadable,
        messages: msgs,
        unread: 0,
        decision,
        lastTs,
      };
    }
    // A derived channel — read title/kind from the id (`dm:RECRUITER` → "Recruiter").
    const head = channelHead(id);
    return {
      id,
      kind: head.kind,
      title: head.title,
      spaceId: "internet",
      order: head.kind === "lobby" ? 0 : head.kind === "faction" ? 1 : 2,
      messages: msgs,
      unread: 0,
      decision,
      lastTs,
    };
  });
}

export interface GuestSession {
  me: GuestId | null;
  status: GuestView | null;
  connected: boolean;
  threads: Threads;
  register: (name: string, code: string) => Promise<void>;
  join: (faction: string) => Promise<unknown>;
  defect: (to: string) => Promise<unknown>;
  choose: (index: number) => Promise<unknown>;
  escape: () => Promise<unknown>;
  /** Hybrid chat: type into a channel (a reply carries the root's seq). */
  say: (channel: string, text: string, parentSeq?: number) => Promise<unknown>;
  /** Access control: pull another participant into a membership-gated channel. */
  inviteToChannel: (person: string, channel: string) => Promise<unknown>;
  /** Leave a membership-gated channel. */
  leaveChannel: (channel: string) => Promise<unknown>;
  leave: () => void;
}

export function useGuestSession(): GuestSession {
  const [me, setMe] = useState<GuestId | null>(() => {
    const stored = load<GuestId>(GK);
    if (stored && !stored.eventId) stored.eventId = "default"; // migrate pre-multi-event sessions
    return stored;
  });
  const [status, setStatus] = useState<GuestView | null>(null);
  const base = me ? `/e/${encodeURIComponent(me.eventId)}` : "";
  const url = me ? `${base}/events?role=guest&id=${encodeURIComponent(me.id)}` : null;
  const { messages, connected } = useChatStream(url, { onSnapshot: (v) => setStatus(v as GuestView) });

  // Every action is scoped to the event the guest joined (`/e/:eventId/...`).
  const post = useCallback(<T,>(path: string, body?: unknown) => api<T>(`${base}${path}`, body), [base]);

  const join = useCallback((faction: string) => post("/api/guest/join", { id: me!.id, faction }), [post, me]);
  const defect = useCallback((to: string) => post("/api/guest/defect", { id: me!.id, to }), [post, me]);
  const choose = useCallback((index: number) => post("/api/guest/choose", { id: me!.id, index }), [post, me]);
  const escape = useCallback(() => post("/api/guest/escape", { id: me!.id }), [post, me]);
  const say = useCallback(
    (channel: string, text: string, parentSeq?: number) =>
      post("/api/guest/say", { id: me!.id, channel, text, parentSeq }),
    [post, me],
  );
  const register = useCallback(async (name: string, code: string) => {
    // Resolve the short event code to its event, then register there.
    const { eventId } = await api<{ eventId: string; role: string }>("/api/resolve-code", { code });
    const r = await api<{ id: string; name: string }>(
      `/e/${encodeURIComponent(eventId)}/api/guest/register`,
      { name, passcode: code },
    );
    const m = { id: r.id, name: r.name, eventId };
    save(GK, m);
    setMe(m);
  }, []);
  const leave = useCallback(() => {
    drop(GK);
    setMe(null);
    setStatus(null);
  }, []);

  const inviteToChannel = useCallback(
    (person: string, channel: string) => post("/api/guest/channel/invite", { id: me!.id, person, channel }),
    [post, me],
  );
  const leaveChannel = useCallback(
    (channel: string) => post("/api/guest/channel/leave", { id: me!.id, channel }),
    [post, me],
  );

  const dock = requiredDecision(status, { choose: (i) => void choose(i), join: (f) => void join(f), escape: () => void escape() });
  const threads = useThreads(buildGuestChannels(messages, dock, status));

  return { me, status, connected, threads, register, join, defect, choose, escape, say, inviteToChannel, leaveChannel, leave };
}

// --- performer (character) --------------------------------------------------

/** Build the performer's surfaces: a scanner, the broadcast feed, one
 *  conversation per guest (targeted messages, for moderation). */
function buildPrimeChannels(messages: Map<number, ChatMessage>, view: PrimeView | null): Channel[] {
  const all = [...messages.values()];
  const scanner: Channel = {
    id: "__scanner",
    kind: "scanner",
    title: "Scanner",
    subtitle: "scan a guest's pass",
    spaceId: "booth", // the booth tools group apart from the room + guests
    order: 0,
    messages: [],
    unread: 0,
    decision: null,
    lastTs: Number.MAX_SAFE_INTEGER, // pinned to the top
  };
  const feedMsgs = all.filter((m) => m.audience === "all").sort((a, b) => a.seq - b.seq);
  const feed: Channel = {
    id: "__feed",
    kind: "lobby",
    title: "The Internet",
    subtitle: "everyone · broadcast feed",
    spaceId: "internet",
    order: 0,
    messages: feedMsgs,
    unread: 0,
    decision: null,
    lastTs: feedMsgs.length ? feedMsgs[feedMsgs.length - 1]!.ts : 0,
  };
  const guests: Channel[] = (view?.guests ?? []).map((g) => {
    const msgs = all
      .filter((m) => Array.isArray(m.audience) && m.audience.includes(g.id))
      .sort((a, b) => a.seq - b.seq);
    return {
      id: `guest:${g.id}`,
      kind: "guest" as const,
      title: g.name,
      subtitle: `${g.faction ?? "unaligned"}${g.captured ? " · 🔒 captured" : ""}`,
      spaceId: "guests", // one section of per-guest conversations
      order: 0,
      messages: msgs,
      unread: 0,
      decision: null,
      lastTs: msgs.length ? msgs[msgs.length - 1]!.ts : 0,
    };
  });
  // Authored channels (SPACE/CHANNEL) the performer runs — grouped by space.
  const spaceTitles = new Map((view?.spaces ?? []).map((s) => [s.id, s.title]));
  const rooms: Channel[] = (view?.channels ?? []).map((c) => {
    const msgs = all.filter((m) => m.channel === c.id).sort((a, b) => a.seq - b.seq);
    return {
      id: c.id,
      kind: c.kind as Channel["kind"],
      title: c.title,
      spaceId: c.spaceId,
      spaceTitle: spaceTitles.get(c.spaceId),
      member: c.member,
      canPost: c.canPost,
      threadable: c.threadable,
      messages: msgs,
      unread: 0,
      decision: null,
      lastTs: msgs.length ? msgs[msgs.length - 1]!.ts : 0,
    };
  });
  return [scanner, feed, ...rooms, ...guests];
}

export interface PrimeSession {
  auth: PrimeAuth | null;
  view: PrimeView | null;
  responses: Array<{ id: number; text: string }>;
  connected: boolean;
  threads: Threads;
  login: (character: string, passcode: string) => Promise<void>;
  scan: (target: string) => Promise<unknown>;
  /** Hybrid chat: type into a channel as this character (reply via parentSeq). */
  say: (channel: string, text: string, parentSeq?: number) => Promise<unknown>;
  /** Access control: pull a guest into a membership-gated channel. */
  inviteToChannel: (person: string, channel: string) => Promise<unknown>;
  /** Leave a membership-gated channel. */
  leaveChannel: (channel: string) => Promise<unknown>;
  becomeAdmin: (passcode: string) => Promise<void>;
  moderate: (id: string, action: string, name?: string) => Promise<unknown>;
  setHidden: (seq: number, hidden: boolean) => Promise<unknown>;
  leave: () => void;
}

export function usePrimeSession(): PrimeSession {
  const [auth, setAuth] = useState<PrimeAuth | null>(() => {
    const stored = load<PrimeAuth>(PK);
    if (stored && !stored.eventId) stored.eventId = "default"; // migrate pre-multi-event sessions
    return stored;
  });
  const [view, setView] = useState<PrimeView | null>(null);
  const [responses, setResponses] = useState<Array<{ id: number; text: string }>>([]);
  const rid = useRef(0);
  const base = auth ? `/e/${encodeURIComponent(auth.eventId)}` : "";
  const url = auth ? `${base}/events?role=prime&id=${encodeURIComponent(auth.character)}` : null;
  const { messages, connected } = useChatStream(url, {
    onSnapshot: (v) => setView(v as PrimeView),
    onResponse: (text) => setResponses((r) => [{ id: rid.current++, text }, ...r]),
  });

  // Every action is scoped to the event this performer signed into.
  const post = useCallback(
    <T,>(path: string, body?: unknown, token?: string) => api<T>(`${base}${path}`, body, token),
    [base],
  );

  const login = useCallback(async (character: string, passcode: string) => {
    // Resolve the performer/mod code to its event, then sign in there.
    const { eventId } = await api<{ eventId: string; role: string }>("/api/resolve-code", { code: passcode });
    const r = await api<{ token: string; character: string; admin: boolean }>(
      `/e/${encodeURIComponent(eventId)}/api/prime/login`,
      { character, passcode },
    );
    const a = { token: r.token, character: r.character, admin: !!r.admin, eventId };
    save(PK, a);
    setAuth(a);
  }, []);
  const scan = useCallback((target: string) => post("/api/scan", { target }, auth!.token), [post, auth]);
  const say = useCallback(
    (channel: string, text: string, parentSeq?: number) =>
      post("/api/prime/say", { channel, text, parentSeq }, auth!.token),
    [post, auth],
  );
  const inviteToChannel = useCallback(
    (person: string, channel: string) => post("/api/prime/channel/invite", { person, channel }, auth!.token),
    [post, auth],
  );
  const leaveChannel = useCallback(
    (channel: string) => post("/api/prime/channel/leave", { channel }, auth!.token),
    [post, auth],
  );
  const becomeAdmin = useCallback(
    async (passcode: string) => {
      const r = await api<{ token: string }>(`${base}/api/mod/login`, { passcode }, auth!.token);
      const a = { token: r.token, character: auth!.character, admin: true, eventId: auth!.eventId };
      save(PK, a);
      setAuth(a);
    },
    [base, auth],
  );
  const moderate = useCallback(
    (id: string, action: string, name?: string) => post("/api/mod/act", { id, action, name }, auth!.token),
    [post, auth],
  );
  const setHidden = useCallback(
    (seq: number, hidden: boolean) => post("/api/mod/message", { seq, hidden }, auth!.token),
    [post, auth],
  );
  const leave = useCallback(() => {
    drop(PK);
    setAuth(null);
    setView(null);
    setResponses([]);
  }, []);

  const threads = useThreads(buildPrimeChannels(messages, view));
  return { auth, view, responses, connected, threads, login, scan, say, inviteToChannel, leaveChannel, becomeAdmin, moderate, setHidden, leave };
}
