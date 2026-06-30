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
}
interface PrimeAuth {
  token: string;
  character: string;
  admin: boolean;
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

/** Group the guest's messages into threads, docking the pending decision. */
function buildGuestChannels(
  messages: Map<number, ChatMessage>,
  dock: { channel: string; decision: Decision } | null,
): Channel[] {
  const groups = groupByChannel(messages);
  if (!groups.has("lobby")) groups.set("lobby", []); // the lobby is always present
  if (dock && !groups.has(dock.channel)) groups.set(dock.channel, []);
  return [...groups.entries()].map(([id, msgs]) => {
    // Derive title/kind from the id so contact names read nicely
    // (`dm:RECRUITER` → "Recruiter") and stay stable for empty threads.
    const head = channelHead(id);
    return {
      id,
      kind: head.kind,
      title: head.title,
      messages: msgs,
      unread: 0,
      decision: dock && dock.channel === id ? dock.decision : null,
      lastTs: msgs.length ? msgs[msgs.length - 1]!.ts : 0,
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
  leave: () => void;
}

export function useGuestSession(): GuestSession {
  const [me, setMe] = useState<GuestId | null>(() => load<GuestId>(GK));
  const [status, setStatus] = useState<GuestView | null>(null);
  const url = me ? `/events?role=guest&id=${encodeURIComponent(me.id)}` : null;
  const { messages, connected } = useChatStream(url, { onSnapshot: (v) => setStatus(v as GuestView) });

  const join = useCallback((faction: string) => api("/api/guest/join", { id: me!.id, faction }), [me]);
  const defect = useCallback((to: string) => api("/api/guest/defect", { id: me!.id, to }), [me]);
  const choose = useCallback((index: number) => api("/api/guest/choose", { id: me!.id, index }), [me]);
  const escape = useCallback(() => api("/api/guest/escape", { id: me!.id }), [me]);
  const register = useCallback(async (name: string, code: string) => {
    const r = await api<{ id: string; name: string }>("/api/guest/register", { name, passcode: code });
    const m = { id: r.id, name: r.name };
    save(GK, m);
    setMe(m);
  }, []);
  const leave = useCallback(() => {
    drop(GK);
    setMe(null);
    setStatus(null);
  }, []);

  const dock = requiredDecision(status, { choose: (i) => void choose(i), join: (f) => void join(f), escape: () => void escape() });
  const threads = useThreads(buildGuestChannels(messages, dock));

  return { me, status, connected, threads, register, join, defect, choose, escape, leave };
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
      messages: msgs,
      unread: 0,
      decision: null,
      lastTs: msgs.length ? msgs[msgs.length - 1]!.ts : 0,
    };
  });
  return [scanner, feed, ...guests];
}

export interface PrimeSession {
  auth: PrimeAuth | null;
  view: PrimeView | null;
  responses: Array<{ id: number; text: string }>;
  connected: boolean;
  threads: Threads;
  login: (character: string, passcode: string) => Promise<void>;
  scan: (target: string) => Promise<unknown>;
  becomeAdmin: (passcode: string) => Promise<void>;
  moderate: (id: string, action: string, name?: string) => Promise<unknown>;
  setHidden: (seq: number, hidden: boolean) => Promise<unknown>;
  leave: () => void;
}

export function usePrimeSession(): PrimeSession {
  const [auth, setAuth] = useState<PrimeAuth | null>(() => load<PrimeAuth>(PK));
  const [view, setView] = useState<PrimeView | null>(null);
  const [responses, setResponses] = useState<Array<{ id: number; text: string }>>([]);
  const rid = useRef(0);
  const url = auth ? `/events?role=prime&id=${encodeURIComponent(auth.character)}` : null;
  const { messages, connected } = useChatStream(url, {
    onSnapshot: (v) => setView(v as PrimeView),
    onResponse: (text) => setResponses((r) => [{ id: rid.current++, text }, ...r]),
  });

  const login = useCallback(async (character: string, passcode: string) => {
    const r = await api<{ token: string; character: string; admin: boolean }>("/api/prime/login", { character, passcode });
    const a = { token: r.token, character: r.character, admin: !!r.admin };
    save(PK, a);
    setAuth(a);
  }, []);
  const scan = useCallback((target: string) => api("/api/scan", { target }, auth!.token), [auth]);
  const becomeAdmin = useCallback(
    async (passcode: string) => {
      const r = await api<{ token: string }>("/api/mod/login", { passcode }, auth!.token);
      const a = { token: r.token, character: auth!.character, admin: true };
      save(PK, a);
      setAuth(a);
    },
    [auth],
  );
  const moderate = useCallback(
    (id: string, action: string, name?: string) => api("/api/mod/act", { id, action, name }, auth!.token),
    [auth],
  );
  const setHidden = useCallback(
    (seq: number, hidden: boolean) => api("/api/mod/message", { seq, hidden }, auth!.token),
    [auth],
  );
  const leave = useCallback(() => {
    drop(PK);
    setAuth(null);
    setView(null);
    setResponses([]);
  }, []);

  const threads = useThreads(buildPrimeChannels(messages, view));
  return { auth, view, responses, connected, threads, login, scan, becomeAdmin, moderate, setHidden, leave };
}
