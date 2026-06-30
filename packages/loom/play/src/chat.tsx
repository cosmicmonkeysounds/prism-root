//! Composable chat primitives — the building blocks both the guest and the
//! performer apps assemble into their own surfaces. Nothing here knows about
//! roles or actions; callers feed in `Channel`s and `Decision`s and supply a
//! footer / moderation hook, so views compose without duplication.

import { useEffect, useRef, type ReactNode } from "react";
import { prettyName } from "./threads.ts";
import type { Action, Channel, ChatMessage, Decision } from "./types.ts";

// --- small shared atoms -----------------------------------------------------

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
  dm: "",
  guest: "",
  scanner: "📷",
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

export function DecisionTray({ decision }: { decision: Decision }) {
  return (
    <div className="tray">
      <div className="tray-title">{decision.title}</div>
      {decision.options.map((o, i) => (
        <button key={i} className={`choice ${o.tone ?? "primary"}`} onClick={o.onClick}>
          {o.label}
        </button>
      ))}
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
}: {
  msg: ChatMessage;
  /** Tag the bubble with its channel (used in the performer's flat view). */
  showChannel?: boolean;
  moderate?: ModerateHook;
}) {
  const cls = msg.kind === "line" ? "line" : msg.kind === "system" ? "system" : msg.kind === "signal" ? "signal" : "narration";
  return (
    <div className={`msg ${cls} ${msg.hidden ? "hidden" : ""}`}>
      {showChannel && <div className="msg-channel">{msg.title}</div>}
      {msg.kind === "line" ? (
        <>
          <div className="speaker">{prettyName(msg.from)}</div>
          <div className="bubble">{msg.text}</div>
        </>
      ) : (
        <div className="bubble plain">{msg.text}</div>
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
    </div>
  );
}

export function MessageList({
  messages,
  showChannel,
  moderate,
}: {
  messages: ChatMessage[];
  showChannel?: boolean;
  moderate?: ModerateHook;
}) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.length]);
  return (
    <div className="thread">
      {messages.map((m) => (
        <MessageBubble key={m.seq} msg={m} showChannel={showChannel} moderate={moderate} />
      ))}
      <div ref={end} />
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
}: {
  channel: Channel;
  onBack: () => void;
  footer?: ReactNode;
  moderate?: ModerateHook;
  showChannel?: boolean;
}) {
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
      <MessageList messages={channel.messages} showChannel={showChannel} moderate={moderate} />
      {(footer || channel.decision) && (
        <footer>
          {channel.decision && <DecisionTray decision={channel.decision} />}
          {footer}
        </footer>
      )}
    </div>
  );
}
