//! Client view types. The message + channel shapes mirror the server's
//! authoritative `server/chat.ts`; the rest mirror the per-role snapshot
//! projections in `server/views.ts`.

// --- server-authoritative chat (mirror of server/chat.ts) -------------------

export type ChannelKind = "lobby" | "faction" | "dm" | "group" | "open" | "private" | "location";
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
  /** Slack thread link: root message's `seq`, or `null` for a top-level line. */
  parentSeq: number | null;
  hidden: boolean;
  /** Beat a scripted line was spoken in, when known (mirror of server chat). */
  beat?: string | null;
}

// --- per-role snapshots (mirror of server/views.ts) -------------------------

/** An authored channel a participant can see (mirror of server views.ts). */
export interface ChannelSnapshot {
  id: string;
  kind: string; // open | private | faction | group | dm
  title: string;
  spaceId: string;
  member: boolean;
  /** May the viewer post here? Drives whether a composer shows. */
  canPost: boolean;
  /** Can messages here open threads? Drives the reply affordance. */
  threadable: boolean;
}

export interface SpaceSnapshot {
  id: string;
  title: string;
}

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
  channels: ChannelSnapshot[];
  spaces: SpaceSnapshot[];
  /** Other participants (id + name), for the invite picker. */
  roster: Array<{ id: string; name: string }>;
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
  channels: ChannelSnapshot[];
  spaces: SpaceSnapshot[];
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
  /** The Discord-style space this channel groups under in the sidebar. */
  spaceId: string;
  /** Section title for the space (from the authored SPACE, when present). */
  spaceTitle?: string;
  /** Stable sidebar sort key within a space (lobby 0, faction 1, …). */
  order?: number;
  /** True for an authored membership-gated channel the viewer belongs to. */
  member?: boolean;
  /** May the viewer post here? (authored channels; derived channels omit → open). */
  canPost?: boolean;
  /** Can messages here open threads? (authored channels; derived → threadable). */
  threadable?: boolean;
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
