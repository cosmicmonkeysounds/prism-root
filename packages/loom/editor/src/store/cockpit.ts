//! The cockpit contract — the shared state + action surface behind Run
//! mode's two sources. Both are the same instrument panel (rooms rail ·
//! chat/roster/world/story/director stage · entity inspector); only the
//! backend differs: Live drives the launched event over the mod SSE +
//! `/e/:eventId/api/mod/*`, Sim drives an in-browser `@loom/core` `Sim`.
//! Shared components read through `useCockpit`, which resolves to
//! whichever store the enclosing `CockpitContext.Provider` supplies
//! (`RunCockpit` follows the source switch; Deploy mounts the live one).
//!
//! Every closed vocabulary here is an enum (const-object form — the
//! workspace compiles with `erasableSyntaxOnly`, which forbids runtime
//! TS `enum` syntax): tabs, selection kinds, message kinds, phases.

import { createContext, useContext } from 'react'
import { create, useStore } from 'zustand'
import type { StatField } from '@/lib/api'

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/** The cockpit's center pages. `Sim`/`Log` exist only on the local-sim
 *  source; the live event's lifecycle page lives in Deploy mode. */
export const CockpitTab = {
  Sim: 'sim',
  Chat: 'chat',
  Roster: 'roster',
  World: 'world',
  Story: 'story',
  Director: 'director',
  Log: 'log',
} as const

export type CockpitTab = (typeof CockpitTab)[keyof typeof CockpitTab]

/** What the Inspector tray is bound to. */
export const SelectionKind = {
  Guest: 'guest',
  Character: 'character',
  Faction: 'faction',
  Location: 'location',
} as const

export type SelectionKind = (typeof SelectionKind)[keyof typeof SelectionKind]

export type Selection = { kind: SelectionKind; id: string } | null

/** Lifecycle phase, shared vocabulary with the event server's `RuntimePhase`. */
export const CockpitPhase = {
  Idle: 'idle',
  Open: 'open',
  Paused: 'paused',
} as const

export type CockpitPhase = (typeof CockpitPhase)[keyof typeof CockpitPhase]

/** The god-view lens — the cockpit's default `perspective`. Any other value
 *  is a guest/persona id (their room set + feed) or a character id (the
 *  performer view). */
export const OPERATOR_LENS = 'operator'

// ---------------------------------------------------------------------------
// Shared data shapes (the server's view projections / their local twins)
// ---------------------------------------------------------------------------

export interface RosterRow {
  id: string
  name: string
  role: string
  faction: string | null
  trueFaction: string | null
  location: string | null
  captured: boolean
  score: number
}

export interface CockpitMessage {
  seq: number
  channel: string
  channelKind?: string
  title?: string
  from: string
  text: string
  kind: string
  ts: number
  hidden?: boolean
  audience: 'all' | string[]
  parentSeq?: number | null
  /** The beat a scripted line was spoken in — links a message to the map. */
  beat?: string | null
}

export interface FactionSummary {
  id: string
  hidden: boolean
  revealed: boolean
  ethos?: string | null
  rival?: string | null
  members: string[]
}

export interface LocationSummary {
  id: string
  label: string | null
  prison: boolean
  occupants: string[]
}

export interface ChannelSummary {
  id: string
  kind: string
  title: string
  spaceId: string
}

export interface CastSummary {
  id: string
  faction: string | null
}

export interface SpaceSummary {
  id: string
  title: string
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

export interface CockpitState {
  phase: string
  scenario: string | null
  /** Backend link is up (SSE connected / local sim instantiated). */
  connected: boolean
  /** A session exists (event launched / sim started). */
  live: boolean
  roster: RosterRow[]
  factions: FactionSummary[]
  locations: LocationSummary[]
  cast: CastSummary[]
  channels: ChannelSummary[]
  spaces: SpaceSummary[]
  /** Every named beat (the "fire beat" picker). */
  beats: string[]
  /** Model-enumerated authored events (`on lockdown`, …) for the director. */
  events: string[]
  ledgerLen: number
  messages: CockpitMessage[]
  error: string | null

  activeTab: CockpitTab
  /** The room the Chat page is peering into (channel id / room key). */
  activeChannel: string
  selection: Selection
  /** Outstanding choices per person id (`__global` for unbound menus).
   *  Sim reads them off the local engine; Run gets them in the mod
   *  snapshot (`ModView.choices`). */
  choices: Record<string, string[]>
  /** The identity lens the cockpit views + speaks through: `OPERATOR_LENS`
   *  (god view), a guest/persona id, or a character id. Filters the rooms
   *  rail + feed and becomes the composer's default voice. */
  perspective: string

  setTab(tab: CockpitTab): void
  selectChannel(channel: string): void
  select(selection: Selection): void
  setPerspective(id: string): void

  say(channel: string, text: string, as?: string, parentSeq?: number | null): Promise<void>
  hideMessage(seq: number, hidden: boolean): Promise<void>
  capture(id: string): Promise<void>
  release(id: string): Promise<void>
  broadcast(scope: string, cue: string): Promise<void>
  setStat(id: string, field: StatField, value: string | number | boolean): Promise<void>
  fireBeat(name: string, subject?: string): Promise<void>
  fireSignal(name: string, subject?: string): Promise<void>
  scanAs(as: string, target: string): Promise<void>
  reveal(faction: string): Promise<void>
  /** Answer a pending choice — Sim resumes the local engine's saved
   *  continuation; Run journals it via `/api/mod/choose` on the guest's
   *  behalf (identical to their own tap). */
  choose(person: string, index: number): Promise<void>
}

/**
 * The read surface `useCockpit` needs. Deliberately the *readonly*
 * store API so a store whose full state is a superset of
 * `CockpitState` (`useOperate`, `useSim`) is assignable as-is —
 * `setState`'s contravariant partial would forbid that.
 */
export type CockpitStore = {
  getState(): CockpitState
  getInitialState(): CockpitState
  subscribe(listener: (state: CockpitState, prevState: CockpitState) => void): () => void
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const noop = async (): Promise<void> => {}

/**
 * The inert default cockpit — every surface empty, every action a no-op.
 * Lets cockpit-aware components (e.g. the story canvas's "Fire beat")
 * mount outside a provider (Writing mode's canvas) without special-casing.
 */
export const nullCockpit: CockpitStore = create<CockpitState>((set) => ({
  phase: CockpitPhase.Idle,
  scenario: null,
  connected: false,
  live: false,
  roster: [],
  factions: [],
  locations: [],
  cast: [],
  channels: [],
  spaces: [],
  beats: [],
  events: [],
  ledgerLen: 0,
  messages: [],
  error: null,
  activeTab: CockpitTab.Chat,
  activeChannel: 'lobby',
  selection: null,
  choices: {},
  perspective: OPERATOR_LENS,
  setTab: (activeTab) => set({ activeTab }),
  selectChannel: (activeChannel) => set({ activeChannel }),
  select: (selection) => set({ selection }),
  setPerspective: (perspective) => set({ perspective }),
  say: noop,
  hideMessage: noop,
  capture: noop,
  release: noop,
  broadcast: noop,
  setStat: noop,
  fireBeat: noop,
  fireSignal: noop,
  scanAs: noop,
  reveal: noop,
  choose: noop,
}))

export const CockpitContext = createContext<CockpitStore>(nullCockpit)

/** Read a slice of whichever cockpit store the enclosing provider supplies. */
export function useCockpit<T>(selector: (s: CockpitState) => T): T {
  const store = useContext(CockpitContext)
  return useStore(store, selector)
}
