//! Client session hooks. Each app instance is one node in the
//! distributed story: it opens an SSE stream for its participant and
//! composes the incoming events into a flowing (branching) dialogue
//! timeline plus an authoritative status snapshot.

import { useCallback, useEffect, useRef, useState } from "react";
import type { Beat, GuestView, PrimeView } from "./types.ts";

/** `Omit` that distributes over a union, preserving each variant's shape. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
type BeatInput = DistributiveOmit<Beat, "id">;

export async function api<T = Record<string, unknown>>(
  path: string,
  body?: unknown,
  token?: string,
): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json", ...(token ? { "x-loom-token": token } : {}) },
    body: JSON.stringify(body ?? {}),
  });
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  if (!res.ok) throw new Error((data["error"] as string) || `HTTP ${res.status}`);
  return data as T;
}

/** Friendly in-world copy for a broadcast cue. */
export function cueText(cue: string): string {
  const map: Record<string, string> = {
    doomed: "📡 The Algorithm has marked you.",
    freedom: "✨ Freedom! You're back in the game.",
    welcome: "👋 Welcome to the internet.",
    peer_ping: "📲 Someone scanned your pass.",
    lockdown_siren: "🚨 Lockdown — the Algorithm tightens its grip.",
    jailed: "🔒 The door of the Internet slams shut behind you.",
  };
  return map[cue] ?? `📣 ${cue}`;
}

const md = (e: Event): Record<string, unknown> => JSON.parse((e as MessageEvent).data);

// --- persistence ---------------------------------------------------------
interface GuestId {
  id: string;
  name: string;
}
interface PrimeAuth {
  token: string;
  character: string;
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

// --- guest ---------------------------------------------------------------

export interface GuestSession {
  me: GuestId | null;
  status: GuestView | null;
  story: Beat[];
  connected: boolean;
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
  const [story, setStory] = useState<Beat[]>([]);
  const [connected, setConnected] = useState(false);
  const beatId = useRef(0);
  const prev = useRef<GuestView | null>(null);

  const addBeat = useCallback((b: BeatInput) => {
    setStory((s) => [...s, { ...b, id: beatId.current++ } as Beat]);
  }, []);

  useEffect(() => {
    if (!me) return;
    const es = new EventSource(`/events?role=guest&id=${encodeURIComponent(me.id)}`);
    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);
    es.addEventListener("snapshot", (e) => {
      const v = md(e) as unknown as GuestView;
      const p = prev.current;
      if (p) {
        if (!p.captured && v.captured) {
          addBeat({ kind: "system", text: "⛓️ You've been dragged into the Internet." });
        } else if (p.captured && !v.captured) {
          addBeat({ kind: "system", text: "🏃 You broke free and slipped back to the party." });
        }
        if (p.faction !== v.faction && v.faction) {
          addBeat({ kind: "system", text: `You threw in with the ${v.faction}.` });
        }
      }
      prev.current = v;
      setStatus(v);
    });
    es.addEventListener("line", (e) => {
      const d = md(e);
      addBeat({ kind: "line", speaker: String(d["speaker"]), text: String(d["text"]) });
    });
    es.addEventListener("notify", (e) => addBeat({ kind: "signal", cue: String(md(e)["cue"]) }));
    es.addEventListener("ambient", (e) => addBeat({ kind: "narration", text: String(md(e)["text"]) }));
    es.addEventListener("reveal", (e) =>
      addBeat({ kind: "system", text: `⚠️ The ${String(md(e)["faction"])} has been exposed!` }),
    );
    return () => es.close();
  }, [me, addBeat]);

  const register = useCallback(async (name: string, code: string) => {
    const r = await api<{ id: string; name: string }>("/api/guest/register", { name, passcode: code });
    const m = { id: r.id, name: r.name };
    save(GK, m);
    setMe(m);
    setStory([
      {
        id: beatId.current++,
        kind: "narration",
        text: "You're in. The internet hums around you — Mods keeping order, Chatters stirring chaos. Pick a side.",
      },
    ]);
  }, []);

  const join = useCallback((faction: string) => api("/api/guest/join", { id: me!.id, faction }), [me]);
  const defect = useCallback((to: string) => api("/api/guest/defect", { id: me!.id, to }), [me]);
  const choose = useCallback((index: number) => api("/api/guest/choose", { id: me!.id, index }), [me]);
  const escape = useCallback(() => api("/api/guest/escape", { id: me!.id }), [me]);
  const leave = useCallback(() => {
    drop(GK);
    setMe(null);
    setStatus(null);
    setStory([]);
    prev.current = null;
  }, []);

  return { me, status, story, connected, register, join, defect, choose, escape, leave };
}

// --- performer (character) ----------------------------------------------

export interface PrimeSession {
  auth: PrimeAuth | null;
  view: PrimeView | null;
  responses: Array<{ id: number; text: string }>;
  connected: boolean;
  login: (character: string, passcode: string) => Promise<void>;
  scan: (target: string) => Promise<unknown>;
  leave: () => void;
}

export function usePrimeSession(): PrimeSession {
  const [auth, setAuth] = useState<PrimeAuth | null>(() => load<PrimeAuth>(PK));
  const [view, setView] = useState<PrimeView | null>(null);
  const [responses, setResponses] = useState<Array<{ id: number; text: string }>>([]);
  const [connected, setConnected] = useState(false);
  const rid = useRef(0);

  useEffect(() => {
    if (!auth) return;
    const es = new EventSource(`/events?role=prime&id=${encodeURIComponent(auth.character)}`);
    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);
    es.addEventListener("snapshot", (e) => setView(md(e) as unknown as PrimeView));
    es.addEventListener("response", (e) =>
      setResponses((r) => [{ id: rid.current++, text: String(md(e)["text"]) }, ...r]),
    );
    return () => es.close();
  }, [auth]);

  const login = useCallback(async (character: string, passcode: string) => {
    const r = await api<{ token: string; character: string }>("/api/prime/login", {
      character,
      passcode,
    });
    const a = { token: r.token, character: r.character };
    save(PK, a);
    setAuth(a);
  }, []);
  const scan = useCallback((target: string) => api("/api/prime/scan", { target }, auth!.token), [auth]);
  const leave = useCallback(() => {
    drop(PK);
    setAuth(null);
    setView(null);
    setResponses([]);
  }, []);

  return { auth, view, responses, connected, login, scan, leave };
}
