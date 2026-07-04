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
}) {
  const cls = msg.kind === "line" ? "line" : msg.kind === "system" ? "system" : msg.kind === "signal" ? "signal" : "narration";
  return (
    <div className={`msg ${cls} ${tuck ? "tuck" : ""} ${msg.hidden ? "hidden" : ""}`}>
      {showChannel && !tuck && <div className="msg-channel">{msg.title}</div>}
      {msg.kind === "line" ? (
        <>
          {!tuck && (
            <span className="speaker" style={{ color: colorFor(msg.from) }}>
              {prettyName(msg.from)}
            </span>
          )}
          <span className="bubble">{msg.text}</span>
        </>
      ) : (
        <span className="bubble plain">{msg.text}</span>
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
}: {
  run: MessageRun;
  /** The full channel history, so each root can show its reply count. */
  allMessages: ChatMessage[];
  showChannel?: boolean;
  moderate?: ModerateHook;
  onOpenThread?: (rootSeq: number) => void;
}) {
  if (run.kind !== "line") {
    return <MessageBubble msg={run.messages[0]!} showChannel={showChannel} moderate={moderate} />;
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
  const runs = groupRuns(rootsOf(messages));
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
        />
      ))}
      <div ref={end} />
    </div>
  );
}

/** A text input + Send button (AOL skin). Used in channels + thread replies. */
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
  const send = () => {
    const t = text.trim();
    if (t === "") return;
    onSend(t);
    setText("");
  };
  return (
    <div className="composer">
      <input
        className="composer-input"
        value={text}
        placeholder={placeholder ?? "Say something…"}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && send()}
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
