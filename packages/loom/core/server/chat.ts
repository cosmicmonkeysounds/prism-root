//! Server-authoritative chat model — the single source of truth for what
//! every participant sees, structured as **channels** (conversation
//! threads) the way Discord / Telegram do it.
//!
//! The participant app used to compose its story client-side from a
//! granular SSE feed (`line` / `notify` / `ambient` / `reveal`). That made
//! history impossible (a re-login started blank) and put narrative copy in
//! two places. Here we instead derive every visible message from the sim's
//! `SimEvent` stream **once, on the server**, route it to a channel, and
//! keep it in an append-only `ChatStore`. Because the `Sim` is fully
//! deterministic (see store.ts), replaying the journal re-derives the exact
//! same messages with the exact same `seq`, so chat history survives a
//! restart for free — only moderation (`hidden`) flags need persisting.
//!
//! Channels:
//!   - **lobby**   `"lobby"`          — "The Internet": ambient story, global
//!                                       broadcasts, and your personal beats.
//!   - **faction** `"faction:<Id>"`   — a faction broadcast, members only.
//!   - **dm**      `"dm:<Character>"`  — a character speaking to you.

import type { Sim, SimEvent } from "../src/runtime/sim/index.ts";

export type ChannelKind = "lobby" | "faction" | "dm";

/** Who receives a message: `"all"` guests, or a fixed set of guest ids. */
export type Audience = "all" | string[];

export type MessageKind = "line" | "narration" | "signal" | "system";

/** One delivered message, the unit a participant's thread is built from. */
export interface ChatMessage {
  /** Monotonic, dense, and stable across a deterministic replay. */
  seq: number;
  /** Channel id (`"lobby"` | `"faction:Mods"` | `"dm:Moderator_Prime"`). */
  channel: string;
  channelKind: ChannelKind;
  /** Display title for the channel this message lands in. */
  title: string;
  /** Speaker/character for `line`, else `""` (ambient / system). */
  from: string;
  kind: MessageKind;
  text: string;
  /** Story-clock time (ms) — deterministic, unlike wall clock. */
  ts: number;
  audience: Audience;
  /** Hidden by a moderator. Withheld from guests, greyed for admins. */
  hidden: boolean;
}

/** A message before the store assigns it a `seq` + resolves `hidden`. */
export type DraftMessage = Omit<ChatMessage, "seq" | "hidden">;

// --- friendly in-world copy for broadcast cues ------------------------------

const CUES: Record<string, string> = {
  doomed: "📡 The Algorithm has marked you.",
  freedom: "✨ Freedom! You're back in the game.",
  welcome: "👋 Welcome to the internet.",
  peer_ping: "📲 Someone scanned your pass.",
  lockdown_siren: "🚨 Lockdown — the Algorithm tightens its grip.",
  jailed: "🔒 The door of the Internet slams shut behind you.",
};

/** In-world copy for a broadcast cue (falls back to a generic megaphone). */
export function cueText(cue: string): string {
  return CUES[cue] ?? `📣 ${cue}`;
}

// --- channel descriptors ----------------------------------------------------

type ChannelHead = Pick<ChatMessage, "channel" | "channelKind" | "title">;

export const LOBBY: ChannelHead = { channel: "lobby", channelKind: "lobby", title: "The Internet" };

function factionChannel(faction: string): ChannelHead {
  return { channel: `faction:${faction}`, channelKind: "faction", title: `#${faction.toLowerCase()}` };
}
function dmChannel(speaker: string): ChannelHead {
  return { channel: `dm:${speaker}`, channelKind: "dm", title: speaker };
}

/** Faction names referenced by a broadcast scope like `faction(Mods) | …`. */
function factionsInScope(scope: string): string[] {
  return [...scope.matchAll(/faction\(([^)]+)\)/g)].map((m) => m[1]!.trim());
}

/** A scope addressed to the whole room (vs. a participant / faction slice). */
function globalScope(scope: string): boolean {
  const s = scope.trim().toLowerCase();
  return s === "" || ["everyone", "all", "internet", "world", "room", "party"].includes(s);
}

// --- composition ------------------------------------------------------------

/**
 * Route a freshly-emitted batch of sim events into guest-facing messages.
 * Pure apart from reading the sim's current clock + faction membership, both
 * of which are deterministic functions of the replayed command history.
 */
export function composeGuestMessages(sim: Sim, events: readonly SimEvent[]): DraftMessage[] {
  const out: DraftMessage[] = [];
  const ts = sim.elapsed();
  const sys = (text: string, audience: Audience): DraftMessage =>
    ({ ...LOBBY, from: "", kind: "system", text, ts, audience });

  for (const e of events) {
    switch (e.type) {
      case "dialogue":
        // A character addressing you → that character's DM thread.
        out.push({ ...dmChannel(e.speaker), from: e.speaker, kind: "line", text: e.text, ts, audience: [...e.audience] });
        break;
      case "broadcast": {
        const text = cueText(e.cue);
        const factions = factionsInScope(e.scope);
        if (factions.length > 0) {
          // Mirror into each targeted faction channel (members only).
          for (const f of factions) {
            out.push({ ...factionChannel(f), from: "", kind: "signal", text, ts, audience: sim.factionMembers(f) });
          }
        } else {
          out.push({ ...LOBBY, from: "", kind: "signal", text, ts, audience: globalScope(e.scope) ? "all" : [...e.audience] });
        }
        break;
      }
      case "ambient":
        out.push({ ...LOBBY, from: "", kind: "narration", text: e.text, ts, audience: "all" });
        break;
      case "captured":
        out.push(sys("⛓️ You've been dragged into the Internet.", [e.person]));
        break;
      case "escaped":
        out.push(sys("🏃 You broke free and slipped back to the party.", [e.person]));
        break;
      case "released":
        out.push(sys("🔓 The door swings open — you're released.", [e.person]));
        break;
      case "joined":
        out.push(sys(`You threw in with the ${e.faction}.`, [e.person]));
        break;
      case "defected":
        out.push(sys(`You betrayed the ${e.from ?? "unaligned"} and defected to the ${e.to}.`, [e.person]));
        break;
      case "factionRevealed":
        out.push(sys(`⚠️ The ${e.faction} has been exposed!`, "all"));
        break;
      default:
        break;
    }
  }
  return out;
}

/**
 * The channel a pending decision belongs to: the DM of the character whose
 * line most recently prompted this person in `events`, else the lobby. Lets
 * a narrative `<choice>` dock under the speaker who asked it, while world
 * decisions (choose a side, escape) stay in the lobby.
 */
export function decisionChannelFor(events: readonly SimEvent[], person: string): string {
  let speaker: string | null = null;
  for (const e of events) {
    if (e.type === "dialogue" && e.audience.includes(person)) speaker = e.speaker;
    if (e.type === "choicePrompted" && e.person === person) break;
  }
  return speaker ? `dm:${speaker}` : LOBBY.channel;
}

/** Is a message visible to a given guest (audience match)? */
export function visibleTo(m: ChatMessage, guestId: string): boolean {
  return m.audience === "all" || m.audience.includes(guestId);
}

// --- store ------------------------------------------------------------------

/**
 * Append-only log of every composed message, plus the moderator-hidden set.
 * Rebuilt verbatim on restart by replaying the journal through
 * `composeGuestMessages` (see server.ts `restore`); only `hiddenSeqs` is
 * persisted out-of-band because moderation isn't a sim event.
 */
export class ChatStore {
  private msgs: ChatMessage[] = [];
  private hidden = new Set<number>();

  /** Assign seqs and append. Returns the newly-stored messages. */
  append(drafts: DraftMessage[]): ChatMessage[] {
    const added: ChatMessage[] = [];
    for (const d of drafts) {
      const seq = this.msgs.length;
      const m: ChatMessage = { ...d, seq, hidden: this.hidden.has(seq) };
      this.msgs.push(m);
      added.push(m);
    }
    return added;
  }

  get length(): number {
    return this.msgs.length;
  }
  all(): readonly ChatMessage[] {
    return this.msgs;
  }
  get(seq: number): ChatMessage | undefined {
    return this.msgs[seq];
  }

  /** Hide / show a message. Returns the updated message, or null if absent. */
  setHidden(seq: number, hidden: boolean): ChatMessage | null {
    const m = this.msgs[seq];
    if (m === undefined) return null;
    m.hidden = hidden;
    if (hidden) this.hidden.add(seq);
    else this.hidden.delete(seq);
    return m;
  }

  hiddenSeqs(): number[] {
    return [...this.hidden].sort((a, b) => a - b);
  }

  /** Re-apply persisted moderation after a rebuild populates `msgs`. */
  loadHidden(seqs: Iterable<number>): void {
    this.hidden = new Set(seqs);
    for (const s of this.hidden) {
      const m = this.msgs[s];
      if (m !== undefined) m.hidden = true;
    }
  }

  /** Start a fresh timeline (new scenario / reset). */
  clear(): void {
    this.msgs = [];
    this.hidden = new Set();
  }

  /**
   * The thread history a viewer should receive. Guests see only messages
   * addressed to them, with hidden ones withheld. An admin sees the same
   * guest's messages *including* hidden ones (flagged) so they can moderate.
   */
  historyFor(guestId: string, admin: boolean): ChatMessage[] {
    return this.msgs.filter((m) => visibleTo(m, guestId) && (admin || !m.hidden));
  }
}
