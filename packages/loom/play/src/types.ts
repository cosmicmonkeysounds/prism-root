//! Client view types. The message + channel shapes mirror the server's
//! authoritative `server/chat.ts`; the rest mirror the per-role snapshot
//! projections in `server/views.ts`.

// --- server-authoritative chat (mirror of server/chat.ts) -------------------

export type ChannelKind = "lobby" | "faction" | "dm";
export type MessageKind = "line" | "narration" | "signal" | "system";

/** One delivered message — the unit a conversation thread is built from. */
export interface ChatMessage {
  seq: number;
  channel: string;
  channelKind: ChannelKind;
  title: string;
  from: string;
  kind: MessageKind;
  text: string;
  ts: number;
  /** `"all"` or a list of guest ids. The client mostly ignores this. */
  audience: "all" | string[];
  hidden: boolean;
}

// --- per-role snapshots (mirror of server/views.ts) -------------------------

export interface GuestView {
  id: string;
  name: string;
  role: string | null;
  faction: string | null;
  score: number;
  location: string | null;
  captured: boolean;
  pendingChoice: string[] | null;
  /** Channel the pending decision docks under. */
  decisionChannel: string | null;
}

export interface PrimeGuest {
  id: string;
  name: string;
  faction: string | null;
  captured: boolean;
}

export interface PrimeView {
  character: string;
  faction: string | null;
  guests: PrimeGuest[];
}

export type Faction = "Mods" | "Chatters" | "TheAlgorithm";

// --- client-side view models (composed by the chat store) -------------------

export type Tone = "primary" | "danger" | "ghost";

/** A quick-reply button inside a decision tray / composer. */
export interface Action {
  label: string;
  onClick: () => void;
  tone?: Tone;
}

/** A decision that has been pulled to a thread, awaiting an answer. */
export interface Decision {
  title: string;
  options: Action[];
}

/** A conversation thread: the model both the list row and the open view use. */
export interface Channel {
  id: string;
  kind: ChannelKind | "guest" | "scanner";
  title: string;
  /** Optional one-line subtitle (faction, status …). */
  subtitle?: string;
  messages: ChatMessage[];
  /** New, unseen, not-mine messages — drives the badge. */
  unread: number;
  /** A required decision docked here (also forces the thread to the top). */
  decision: Decision | null;
  /** Story-clock time of the last message (for sorting + the row timestamp). */
  lastTs: number;
}
