//! Thin transport layer: a JSON `fetch` POST helper and the SSE message
//! reducer that turns the server's `history` / `message` / `messageModerated`
//! stream into a live, ordered, de-duplicated message map.

import { useEffect, useRef, useState } from "react";
import type { ChatMessage } from "./types.ts";

/** POST JSON to the server, attaching a capability token when present. */
export async function api<T = Record<string, unknown>>(path: string, body?: unknown, token?: string): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json", ...(token ? { "x-loom-token": token } : {}) },
    body: JSON.stringify(body ?? {}),
  });
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  if (!res.ok) throw new Error((data["error"] as string) || `HTTP ${res.status}`);
  return data as T;
}

const json = (e: Event): unknown => JSON.parse((e as MessageEvent).data);

export interface ChatStream {
  /** Messages keyed by seq (stable + de-duplicating across reconnects). */
  messages: Map<number, ChatMessage>;
  connected: boolean;
}

/**
 * Subscribe to an SSE endpoint and maintain the message map. `onSnapshot`
 * receives role-specific snapshots (`snapshot` event); `onResponse` receives
 * performer scan readouts (`response`). Both are optional so guests and
 * performers share this one wiring.
 */
export function useChatStream(
  url: string | null,
  handlers: { onSnapshot?: (v: unknown) => void; onResponse?: (text: string) => void },
): ChatStream {
  const [messages, setMessages] = useState<Map<number, ChatMessage>>(new Map());
  const [connected, setConnected] = useState(false);
  // Keep the latest handlers without re-opening the stream on every render.
  const h = useRef(handlers);
  h.current = handlers;

  useEffect(() => {
    if (!url) return;
    setMessages(new Map());
    const es = new EventSource(url);
    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);

    const upsert = (m: ChatMessage) =>
      setMessages((prev) => {
        const next = new Map(prev);
        next.set(m.seq, m);
        return next;
      });

    es.addEventListener("snapshot", (e) => h.current.onSnapshot?.(json(e)));
    es.addEventListener("response", (e) => h.current.onResponse?.(String((json(e) as Record<string, unknown>)["text"])));
    es.addEventListener("history", (e) =>
      setMessages(() => {
        const m = new Map<number, ChatMessage>();
        for (const msg of json(e) as ChatMessage[]) m.set(msg.seq, msg);
        return m;
      }),
    );
    es.addEventListener("message", (e) => upsert(json(e) as ChatMessage));
    // An ephemeral message aged out — drop it from the view.
    es.addEventListener("messageExpired", (e) => {
      const { seq } = json(e) as { seq: number };
      setMessages((prev) => {
        if (!prev.has(seq)) return prev;
        const next = new Map(prev);
        next.delete(seq);
        return next;
      });
    });
    es.addEventListener("messageModerated", (e) => {
      const d = json(e) as ChatMessage | { seq: number; hidden: boolean };
      if ("text" in d) {
        upsert(d); // admin payload: full message + flag
      } else if (d.hidden) {
        // Guest view of a hide: drop the message we can no longer see.
        setMessages((prev) => {
          if (!prev.has(d.seq)) return prev;
          const next = new Map(prev);
          next.delete(d.seq);
          return next;
        });
      }
    });
    return () => es.close();
  }, [url]);

  return { messages, connected };
}
