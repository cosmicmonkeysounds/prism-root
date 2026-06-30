//! Shared client types — mirror the server's per-role view projections
//! (`server/views.ts`) plus the dialogue-timeline model the React app
//! composes from the live event stream.

export interface GuestView {
  id: string;
  name: string;
  role: string | null;
  faction: string | null;
  score: number;
  location: string | null;
  captured: boolean;
  pendingChoice: string[] | null;
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

/** One thing that flows into a participant's story timeline. */
export type Beat =
  | { id: number; kind: "narration"; text: string }
  | { id: number; kind: "line"; speaker: string; text: string }
  | { id: number; kind: "signal"; cue: string }
  | { id: number; kind: "system"; text: string };

export type Faction = "Mods" | "Chatters" | "TheAlgorithm";
