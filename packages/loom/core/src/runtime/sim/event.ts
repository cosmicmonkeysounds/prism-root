//! Ecosystem-simulation events — the append-only record of everything
//! that happens in the world. External input (account creation, QR
//! scans, faction joins) and internal effects (captures, broadcasts,
//! dialogue) all land here in source order.

/** One thing that happened in the world. */
export type SimEvent =
  | { type: "accountCreated"; person: string; name: string; role: string }
  | { type: "joined"; person: string; faction: string }
  | { type: "defected"; person: string; from: string | null; to: string }
  | { type: "betrayed"; person: string; displayed: string | null; secret: string }
  | { type: "factionRevealed"; faction: string }
  | { type: "scanned"; scanner: string; person: string }
  | { type: "captured"; person: string; location: string; by: string | null }
  | { type: "released"; person: string; location: string }
  | { type: "escaped"; person: string }
  | { type: "arrived"; person: string; location: string; from: string | null }
  | { type: "cast"; person: string; role: string }
  | { type: "promoted"; person: string; role: string }
  | { type: "worldSet"; path: string; value: string }
  | { type: "relationshipChanged"; subject: string; relation: string; object: string; value: number }
  | { type: "broadcast"; cue: string; audience: string[]; scope: string }
  | { type: "dialogue"; speaker: string; text: string; audience: string[] }
  // A participant typed a message into a channel. `from` is the display name,
  // `audience` is resolved at send time ("all" or guest ids), `parentSeq` links
  // a reply to its root message (null for a top-level message).
  | { type: "chat"; from: string; channel: string; text: string; audience: "all" | string[]; parentSeq: number | null }
  // A participant was invited into (or left) an authored channel — membership
  // changes for private/group/dm rooms drive who can see + post.
  | { type: "channelInvited"; channel: string; person: string; by: string }
  | { type: "channelLeft"; channel: string; person: string }
  | { type: "action"; text: string }
  | { type: "directive"; verb: string; args: string }
  | { type: "beatEntered"; beat: string }
  | { type: "choicePrompted"; person: string | null; promptId: string; options: string[] }
  | { type: "respond"; to: string; text: string }
  | { type: "signal"; name: string; subject: string | null }
  | { type: "ambient"; source: string; text: string }
  | { type: "tick"; elapsedMs: number }
  | { type: "diagnostic"; message: string };

/** Append-only event log with the queries the rule engine + app need. */
export class SimLog {
  private events: SimEvent[] = [];

  push(event: SimEvent): number {
    this.events.push(event);
    return this.events.length - 1;
  }

  all(): readonly SimEvent[] {
    return this.events;
  }

  len(): number {
    return this.events.length;
  }

  /** Envelopes appended at or after `from` (the app polls this slice). */
  since(from: number): SimEvent[] {
    return this.events.slice(from);
  }

  /** Count of events satisfying `pred`. */
  count(pred: (e: SimEvent) => boolean): number {
    let n = 0;
    for (const e of this.events) if (pred(e)) n += 1;
    return n;
  }

  /** Most recent event satisfying `pred`, or null. */
  last(pred: (e: SimEvent) => boolean): SimEvent | null {
    for (let i = this.events.length - 1; i >= 0; i--) {
      if (pred(this.events[i]!)) return this.events[i]!;
    }
    return null;
  }
}
