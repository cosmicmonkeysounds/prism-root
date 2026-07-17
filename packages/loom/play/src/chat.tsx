//! Composable chat primitives — the building blocks both the guest and the
//! performer apps assemble into their own surfaces. Nothing here knows about
//! roles or actions; callers feed in `Channel`s and `Decision`s and supply a
//! footer / moderation hook, so views compose without duplication.

import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  groupRuns,
  prettyName,
  repliesFor,
  replyCountFor,
  rootsOf,
  type MessageRun,
  type SpaceGroup,
} from "./threads.ts";
import type { Action, Channel, ChatMessage, Decision } from "./types.ts";

// --- small shared atoms -----------------------------------------------------

/**
 * A deterministic screen-name colour, the way every AOL chatter picked a
 * font colour and kept it. Hash the name into a small web-safe palette so the
 * same person is always the same colour across the room.
 */
const SN_COLORS = [
  "#c00000", "#0000c0", "#008000", "#800080", "#c05000",
  "#008080", "#a00050", "#505000", "#0050a0", "#a02000",
];
export function colorFor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return SN_COLORS[h % SN_COLORS.length]!;
}

/**
 * Reveal `text` one character at a time — the classic RPG dialogue crawl.
 * Only runs when `enabled` (we type just the newest line, not the backlog);
 * honours `prefers-reduced-motion` by showing the full line at once.
 */
function useTypewriter(text: string, enabled: boolean): { shown: string; typing: boolean } {
  const [count, setCount] = useState(enabled ? 0 : text.length);
  useEffect(() => {
    const reduce =
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!enabled || reduce || text.length === 0) {
      setCount(text.length);
      return;
    }
    setCount(0);
    let i = 0;
    const id = window.setInterval(() => {
      i += 1;
      setCount(i);
      if (i >= text.length) window.clearInterval(id);
    }, 18);
    return () => window.clearInterval(id);
  }, [text, enabled]);
  return { shown: text.slice(0, count), typing: count < text.length };
}

export function FactionPill({ faction }: { faction: string | null }) {
  const f = faction ?? "none";
  return <span className={`pill ${f}`}>{faction ?? "unaligned"}</span>;
}

export function ConnDot({ connected, label }: { connected: boolean; label: string }) {
  return (
    <span className="conn">
      <span className={`dot ${connected ? "on" : ""}`} /> {label}
    </span>
  );
}

const GLYPH: Record<Channel["kind"], string> = {
  lobby: "🌐",
  faction: "#",
  location: "📍",
  dm: "",
  guest: "",
  scanner: "📷",
  group: "👥",
  open: "🔓",
  private: "🔒",
};

export function Avatar({ channel }: { channel: Pick<Channel, "kind" | "title"> }) {
  const glyph = GLYPH[channel.kind] || channel.title.replace(/[#\s]/g, "").slice(0, 1).toUpperCase();
  return <div className={`avatar ${channel.kind}`}>{glyph}</div>;
}

export function Badge({ count }: { count: number }) {
  if (count <= 0) return null;
  return <span className="badge">{count > 9 ? "9+" : count}</span>;
}

// --- decision tray (quick-reply buttons) ------------------------------------

/**
 * The decision box — a video-game dialogue chooser. Options are numbered and
 * driven like an RPG menu: ↑/↓ (or the number keys) move a ▶ selection caret,
 * Enter confirms. Hover / focus still work for touch + mouse. The keyboard
 * handler steps aside while a text field is focused so typing never triggers a
 * choice.
 */
export function DecisionTray({ decision }: { decision: Decision }) {
  const [cursor, setCursor] = useState(0);
  const n = decision.options.length;
  // A fresh prompt resets the caret to the top option.
  useEffect(() => setCursor(0), [decision.title, n]);
  const pick = (i: number) => decision.options[i]?.onClick();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return; // don't hijack the composer
      if (e.key === "ArrowDown" || e.key === "ArrowRight") {
        e.preventDefault();
        setCursor((c) => (c + 1) % n);
      } else if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
        e.preventDefault();
        setCursor((c) => (c - 1 + n) % n);
      } else if (e.key === "Enter") {
        e.preventDefault();
        pick(cursor);
      } else if (/^[1-9]$/.test(e.key)) {
        const i = Number(e.key) - 1;
        if (i < n) {
          e.preventDefault();
          pick(i);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cursor, n, decision]);
  return (
    <div className="tray dialogue" role="menu" aria-label={decision.title}>
      <div className="tray-title">{decision.title}</div>
      <div className="dlg-options">
        {decision.options.map((o, i) => (
          <button
            key={i}
            role="menuitem"
            className={`dlg-choice ${o.tone ?? "primary"} ${i === cursor ? "on" : ""}`}
            onMouseEnter={() => setCursor(i)}
            onFocus={() => setCursor(i)}
            onClick={o.onClick}
          >
            <span className="dlg-caret" aria-hidden>
              ▶
            </span>
            <span className="dlg-key" aria-hidden>
              {i + 1}
            </span>
            <span className="dlg-label">{o.label}</span>
          </button>
        ))}
      </div>
      {n > 1 && <div className="dlg-hint">↑↓ select · enter confirm · 1–{n} quick-pick</div>}
    </div>
  );
}

/**
 * A modal picker: choose a participant to pull into a channel. Shared by the
 * guest and performer invite affordances.
 */
export function InviteSheet({
  title,
  people,
  onPick,
  onClose,
}: {
  title: string;
  people: Array<{ id: string; name: string }>;
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <h2>{title}</h2>
        {people.length === 0 && <div className="muted">No one else here yet.</div>}
        {people.map((p) => (
          <button
            key={p.id}
            className="choice ghost"
            onClick={() => {
              onPick(p.id);
              onClose();
            }}
          >
            {p.name}
          </button>
        ))}
        <button className="link" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

/** A row of inline action buttons (used in composers / profile sheets). */
export function ActionRow({ actions }: { actions: Action[] }) {
  return (
    <div className="actionrow">
      {actions.map((a, i) => (
        <button key={i} className={`choice ${a.tone ?? "ghost"}`} onClick={a.onClick}>
          {a.label}
        </button>
      ))}
    </div>
  );
}

// --- channel list -----------------------------------------------------------

function lastPreview(c: Channel): string {
  const last = c.messages[c.messages.length - 1];
  if (c.decision) return c.decision.title;
  if (!last) return "—";
  return last.kind === "line" ? `${last.from ? `${last.from}: ` : ""}${last.text}` : last.text;
}

export function ChannelRow({ channel, onOpen }: { channel: Channel; onOpen: (id: string) => void }) {
  return (
    <button className="chrow" onClick={() => onOpen(channel.id)}>
      <Avatar channel={channel} />
      <div className="chrow-body">
        <div className="chrow-top">
          <span className="chrow-title">{channel.title}</span>
          {channel.decision && <span className="decision-dot">decision</span>}
        </div>
        <div className="chrow-sub">{channel.subtitle ?? lastPreview(channel)}</div>
      </div>
      <Badge count={channel.unread} />
    </button>
  );
}

export function ChannelList({
  channels,
  onOpen,
  header,
  empty,
}: {
  channels: Channel[];
  onOpen: (id: string) => void;
  header?: ReactNode;
  empty?: ReactNode;
}) {
  return (
    <div className="screen">
      {header}
      <div className="chlist">
        {channels.length === 0 && <div className="empty">{empty ?? "No conversations yet."}</div>}
        {channels.map((c) => (
          <ChannelRow key={c.id} channel={c} onOpen={onOpen} />
        ))}
      </div>
    </div>
  );
}

// --- one open thread --------------------------------------------------------

export interface ModerateHook {
  /** Flip a message's hidden flag (admin only). Absent → no controls. */
  setHidden: (seq: number, hidden: boolean) => void;
}

export function MessageBubble({
  msg,
  showChannel,
  moderate,
  tuck,
  replies,
  onOpenThread,
  live,
}: {
  msg: ChatMessage;
  /** Tag the bubble with its channel (used in the performer's flat view). */
  showChannel?: boolean;
  moderate?: ModerateHook;
  /** A same-sender continuation line: hide the screen-name banner. */
  tuck?: boolean;
  /** Reply count, when the caller wants a thread affordance on this line. */
  replies?: number;
  onOpenThread?: (rootSeq: number) => void;
  /** The freshest line in the room — reveal it with the typewriter crawl. */
  live?: boolean;
}) {
  const cls = msg.kind === "line" ? "line" : msg.kind === "system" ? "system" : msg.kind === "signal" ? "signal" : "narration";
  // Only crawl spoken/narrated story text — system + signal notices pop in.
  const crawlable = msg.kind === "line" || msg.kind === "narration";
  const { shown, typing } = useTypewriter(msg.text, !!live && crawlable);
  return (
    <div className={`msg ${cls} ${tuck ? "tuck" : ""} ${msg.hidden ? "hidden" : ""} ${typing ? "typing" : ""}`}>
      {showChannel && !tuck && <div className="msg-channel">{msg.title}</div>}
      {msg.kind === "line" ? (
        <>
          {!tuck && (
            <span className="speaker" style={{ color: colorFor(msg.from) }}>
              {prettyName(msg.from)}
            </span>
          )}
          <span className="bubble">{shown}</span>
        </>
      ) : (
        <span className="bubble plain">{crawlable ? shown : msg.text}</span>
      )}
      {moderate && (
        <button
          className="mod-toggle"
          title={msg.hidden ? "Restore for guests" : "Hide from guests"}
          onClick={() => moderate.setHidden(msg.seq, !msg.hidden)}
        >
          {msg.hidden ? "🙈 hidden — restore" : "hide"}
        </button>
      )}
      {onOpenThread && msg.kind === "line" && (
        <button
          className={`replies ${replies ? "has" : ""}`}
          onClick={() => onOpenThread(msg.seq)}
          title={replies ? `${replies} ${replies === 1 ? "reply" : "replies"}` : "Reply in thread"}
          aria-label={replies ? `${replies} replies` : "Reply in thread"}
        >
          <span className="reply-ico">💬</span>
          {replies && replies > 0 ? <span className="reply-n">{replies}</span> : null}
        </button>
      )}
    </div>
  );
}

/**
 * One Slack/Discord sender-run: a banner (the colored screen name) shown once,
 * with each subsequent same-sender line tucked under it. Non-`line` runs
 * (narration / system / signal) are single standalone messages.
 */
export function MessageGroup({
  run,
  allMessages,
  showChannel,
  moderate,
  onOpenThread,
  liveSeq,
}: {
  run: MessageRun;
  /** The full channel history, so each root can show its reply count. */
  allMessages: ChatMessage[];
  showChannel?: boolean;
  moderate?: ModerateHook;
  onOpenThread?: (rootSeq: number) => void;
  /** Seq of the freshest line in the room — gets the typewriter crawl. */
  liveSeq?: number;
}) {
  if (run.kind !== "line") {
    return (
      <MessageBubble
        msg={run.messages[0]!}
        showChannel={showChannel}
        moderate={moderate}
        live={run.messages[0]!.seq === liveSeq}
      />
    );
  }
  return (
    <div className="run">
      {run.messages.map((m, i) => (
        <MessageBubble
          key={m.seq}
          msg={m}
          tuck={i > 0}
          showChannel={showChannel}
          moderate={moderate}
          onOpenThread={onOpenThread}
          replies={onOpenThread ? replyCountFor(allMessages, m.seq) : undefined}
          live={m.seq === liveSeq}
        />
      ))}
    </div>
  );
}

export function MessageList({
  messages,
  showChannel,
  moderate,
  onOpenThread,
}: {
  messages: ChatMessage[];
  showChannel?: boolean;
  moderate?: ModerateHook;
  onOpenThread?: (rootSeq: number) => void;
}) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.length]);
  // Only top-level messages live in the channel; replies are tucked into their
  // thread panel. Consecutive same-sender lines coalesce into one banner.
  const roots = rootsOf(messages);
  const runs = groupRuns(roots);
  // The last root is the freshest line — it crawls in like game dialogue.
  const liveSeq = roots.length ? roots[roots.length - 1]!.seq : undefined;
  return (
    <div className="thread">
      {runs.map((run) => (
        <MessageGroup
          key={run.messages[0]!.seq}
          run={run}
          allMessages={messages}
          showChannel={showChannel}
          moderate={moderate}
          onOpenThread={onOpenThread}
          liveSeq={liveSeq}
        />
      ))}
      <div ref={end} />
    </div>
  );
}

/**
 * A roomy auto-growing text box + Send button (AOL skin). Used in channels +
 * thread replies. Grows with what you type (up to a cap) so you can always see
 * the whole message — Enter sends, Shift+Enter drops a newline.
 */
export function Composer({
  onSend,
  placeholder,
  disabled,
}: {
  onSend: (text: string) => void;
  placeholder?: string;
  disabled?: boolean;
}) {
  const [text, setText] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);
  // Reflow the textarea to fit its content (bounded by the CSS max-height).
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);
  const send = () => {
    const t = text.trim();
    if (t === "") return;
    onSend(t);
    setText("");
  };
  return (
    <div className="composer">
      <textarea
        ref={ref}
        className="composer-input"
        rows={1}
        value={text}
        placeholder={placeholder ?? "Say something…"}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
      />
      <button className="choice composer-send" onClick={send} disabled={disabled}>
        Send
      </button>
    </div>
  );
}

/**
 * One open conversation: a back-button header, the message stream, and a
 * caller-supplied footer (a decision tray, a scanner, moderation actions…).
 */
export function ChannelView({
  channel,
  onBack,
  footer,
  moderate,
  showChannel,
  onSend,
  onOpenThread,
}: {
  channel: Channel;
  onBack: () => void;
  footer?: ReactNode;
  moderate?: ModerateHook;
  showChannel?: boolean;
  /** Hybrid chat: when present, a composer docks below the footer. */
  onSend?: (text: string, parentSeq?: number) => void;
  /** When present, line messages expose a Slack-style "reply" affordance. */
  onOpenThread?: (rootSeq: number) => void;
}) {
  // Channel-type rules: a read-only channel hides the composer; a
  // non-threadable channel hides the reply affordance. (Derived channels omit
  // both flags → open + threadable.)
  const canPost = channel.canPost !== false;
  const threadable = channel.threadable !== false;
  return (
    <div className="screen">
      <header className="thread-head">
        <button className="back" onClick={onBack}>
          ‹
        </button>
        <Avatar channel={channel} />
        <div className="thread-id">
          <strong>{channel.title}</strong>
          {channel.subtitle && <span className="muted">{channel.subtitle}</span>}
        </div>
      </header>
      <MessageList
        messages={channel.messages}
        showChannel={showChannel}
        moderate={moderate}
        onOpenThread={threadable ? onOpenThread : undefined}
      />
      {(footer || channel.decision || (onSend && canPost)) && (
        <footer>
          {channel.decision && <DecisionTray decision={channel.decision} />}
          {footer}
          {onSend && canPost && <Composer onSend={(t) => onSend(t)} placeholder={`Message ${channel.title}…`} />}
        </footer>
      )}
    </div>
  );
}

/**
 * A Slack-style thread panel: the root message pinned at the top, its replies
 * below, and a reply composer. `rootSeq` is the message the thread hangs from.
 */
export function MessageThread({
  channel,
  rootSeq,
  onClose,
  moderate,
  onSend,
}: {
  channel: Channel;
  rootSeq: number;
  onClose: () => void;
  moderate?: ModerateHook;
  onSend?: (text: string, parentSeq?: number) => void;
}) {
  const end = useRef<HTMLDivElement>(null);
  const root = channel.messages.find((m) => m.seq === rootSeq);
  const replies = repliesFor(channel.messages, rootSeq);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [replies.length]);
  return (
    <div className="screen">
      <header className="thread-head">
        <button className="back" onClick={onClose}>
          ‹
        </button>
        <div className="avatar dm">💬</div>
        <div className="thread-id">
          <strong>Thread</strong>
          <span className="muted">{channel.title}</span>
        </div>
      </header>
      <div className="thread">
        {root && <MessageBubble msg={root} moderate={moderate} />}
        <div className="thread-divider">
          {replies.length} {replies.length === 1 ? "reply" : "replies"}
        </div>
        {groupRuns(replies).map((run) => (
          <MessageGroup key={run.messages[0]!.seq} run={run} allMessages={channel.messages} moderate={moderate} />
        ))}
        <div ref={end} />
      </div>
      {onSend && channel.canPost !== false && (
        <footer>
          <Composer onSend={(t) => onSend(t, rootSeq)} placeholder="Reply…" />
        </footer>
      )}
    </div>
  );
}

/**
 * The Discord-style channel sidebar: channels folded into ordered space
 * sections, each with a header. A thin wrapper over `ChannelRow`.
 */
export function SpaceList({
  spaces,
  onOpen,
  header,
  empty,
}: {
  spaces: SpaceGroup[];
  onOpen: (id: string) => void;
  header?: ReactNode;
  empty?: ReactNode;
}) {
  const total = spaces.reduce((n, s) => n + s.channels.length, 0);
  return (
    <div className="screen">
      {header}
      <div className="chlist">
        {total === 0 && <div className="empty">{empty ?? "No conversations yet."}</div>}
        {spaces.map((s) => (
          <div key={s.id} className="space-section">
            <div className="space-title">{s.title}</div>
            {s.channels.map((c) => (
              <ChannelRow key={c.id} channel={c} onOpen={onOpen} />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
