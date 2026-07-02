//! The Operate mode's shared event-control core.
//!
//! One store drives the whole run/admin cockpit: the Event sidebar (launch /
//! codes / lifecycle + the rooms navigator), the Operate stage (chat per room ·
//! roster · world · director tools) and the Inspector tray (per-guest stat
//! editing). The "run panel" and the "admin tools" are the same capability, so
//! they share this state. Launch/lifecycle go through the control plane
//! (`/api/projects/:id/event`); the live view + moderation ride the per-event
//! mod SSE + `/e/:eventId/api/mod/*`, authorized by the author's session.

import { create } from 'zustand'
import { eventsApi, modApi, type EventInfo, type StatField } from '@/lib/api'
import { useGraph } from '@/store/graph'

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

export interface OperateMessage {
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

interface ModSnapshot {
  phase: string
  scenario: string | null
  roster: RosterRow[]
  factions: FactionSummary[]
  locations: LocationSummary[]
  characters: string[]
  cast: CastSummary[]
  channels: ChannelSummary[]
  spaces: SpaceSummary[]
  beats: string[]
  ledgerLen: number
}

/** What's selected across the cockpit → drives the Inspector tray. */
export type Selection =
  | { kind: 'guest'; id: string }
  | { kind: 'character'; id: string }
  | { kind: 'faction'; id: string }
  | { kind: 'location'; id: string }
  | null

/** The Run cockpit's center pages. */
export type RunTab = 'event' | 'chat' | 'roster' | 'world' | 'director' | 'story'

type OperateState = {
  projectId: string | null
  event: EventInfo | null
  phase: string
  scenario: string | null
  roster: RosterRow[]
  factions: FactionSummary[]
  locations: LocationSummary[]
  cast: CastSummary[]
  channels: ChannelSummary[]
  spaces: SpaceSummary[]
  beats: string[]
  ledgerLen: number
  messages: OperateMessage[]
  connected: boolean
  busy: boolean
  error: string | null

  /** The active center page. */
  activeTab: RunTab
  /** The room the Chat tab is peering into (channel id), null → lobby. */
  activeChannel: string
  /** The entity (guest / character / faction / location) the Inspector is bound to. */
  selection: Selection

  init: (projectId: string) => Promise<void>
  teardown: () => void
  launch: (mode: 'live' | 'preview') => Promise<void>
  pause: () => Promise<void>
  resume: () => Promise<void>
  end: () => Promise<void>
  reset: () => Promise<void>

  // live moderation
  capture: (id: string) => Promise<void>
  release: (id: string) => Promise<void>
  hideMessage: (seq: number, hidden: boolean) => Promise<void>
  broadcast: (scope: string, cue: string) => Promise<void>
  say: (channel: string, text: string, as?: string, parentSeq?: number | null) => Promise<void>
  setStat: (id: string, field: StatField, value: string | number | boolean) => Promise<void>
  fireBeat: (name: string, subject?: string) => Promise<void>
  fireSignal: (name: string, subject?: string) => Promise<void>
  scanAs: (as: string, target: string) => Promise<void>
  reveal: (faction: string) => Promise<void>

  // UI selection
  setTab: (tab: RunTab) => void
  selectChannel: (channel: string) => void
  select: (selection: Selection) => void
}

// One live mod stream at a time (Operate mode is a single surface).
let es: EventSource | null = null

/** The snapshot-driven fields, reset to empty on init/teardown/end. */
const EMPTY_SNAPSHOT = {
  roster: [] as RosterRow[],
  factions: [] as FactionSummary[],
  locations: [] as LocationSummary[],
  cast: [] as CastSummary[],
  channels: [] as ChannelSummary[],
  spaces: [] as SpaceSummary[],
  beats: [] as string[],
  ledgerLen: 0,
}

export const useOperate = create<OperateState>((set, get) => {
  const disconnect = () => {
    if (es) {
      es.close()
      es = null
    }
    set({ connected: false })
  }

  const withEvent = async (fn: (id: string) => Promise<unknown>) => {
    const ev = get().event
    if (!ev) return
    try {
      await fn(ev.id)
    } catch (e) {
      set({ error: (e as Error).message })
    }
  }

  const connect = (eventId: string) => {
    disconnect()
    useGraph.getState().runtimeReset() // fresh event → fresh overlay
    const source = new EventSource(`/e/${encodeURIComponent(eventId)}/events?role=mod`)
    source.onopen = () => set({ connected: true })
    source.onerror = () => set({ connected: false })
    source.addEventListener('snapshot', (e) => {
      const snap = JSON.parse((e as MessageEvent).data) as Partial<ModSnapshot>
      set({
        phase: snap.phase ?? get().phase,
        scenario: snap.scenario ?? null,
        roster: snap.roster ?? [],
        factions: snap.factions ?? [],
        locations: snap.locations ?? [],
        cast: snap.cast ?? [],
        channels: snap.channels ?? [],
        spaces: snap.spaces ?? [],
        beats: snap.beats ?? [],
        ledgerLen: snap.ledgerLen ?? 0,
      })
    })
    source.addEventListener('history', (e) => {
      const msgs = JSON.parse((e as MessageEvent).data) as OperateMessage[]
      set({ messages: msgs })
    })
    const upsert = (m: OperateMessage) =>
      set((s) => {
        const rest = s.messages.filter((x) => x.seq !== m.seq)
        return { messages: [...rest, m].sort((a, b) => a.seq - b.seq) }
      })
    source.addEventListener('message', (e) => upsert(JSON.parse((e as MessageEvent).data) as OperateMessage))
    source.addEventListener('messageModerated', (e) => {
      const d = JSON.parse((e as MessageEvent).data) as OperateMessage
      if ('text' in d) upsert(d)
    })
    // Raw sim feed (mods only): drives the story-graph runtime overlay —
    // the same contract a future in-editor simulator will feed locally.
    source.addEventListener('sim', (e) => {
      const ev = JSON.parse((e as MessageEvent).data) as { type?: string; beat?: string }
      if (ev.type === 'beatEntered' && typeof ev.beat === 'string') {
        useGraph.getState().runtimeEnter(ev.beat)
      }
    })
    es = source
  }

  return {
    projectId: null,
    event: null,
    phase: 'idle',
    scenario: null,
    roster: [],
    factions: [],
    locations: [],
    cast: [],
    channels: [],
    spaces: [],
    beats: [],
    ledgerLen: 0,
    messages: [],
    connected: false,
    busy: false,
    error: null,
    activeTab: 'event',
    activeChannel: 'lobby',
    selection: null,

    init: async (projectId) => {
      if (get().projectId === projectId && get().event) return // already live for this project
      set({ projectId, error: null, messages: [], selection: null, activeTab: 'event', activeChannel: 'lobby', ...EMPTY_SNAPSHOT })
      try {
        const event = await eventsApi.status(projectId)
        set({ event, phase: event?.status ?? 'idle' })
        if (event) connect(event.id)
      } catch (e) {
        set({ error: (e as Error).message })
      }
    },

    teardown: () => {
      disconnect()
      useGraph.getState().runtimeReset()
      set({
        projectId: null,
        event: null,
        phase: 'idle',
        scenario: null,
        messages: [],
        selection: null,
        activeTab: 'event',
        activeChannel: 'lobby',
        ...EMPTY_SNAPSHOT,
      })
    },

    launch: async (mode) => {
      const projectId = get().projectId
      if (!projectId) return
      set({ busy: true, error: null })
      try {
        const event = await eventsApi.launch(projectId, mode)
        set({ event, phase: event.status })
        connect(event.id)
      } catch (e) {
        set({ error: (e as Error).message })
      } finally {
        set({ busy: false })
      }
    },

    pause: async () => {
      const projectId = get().projectId
      if (!projectId) return
      set({ busy: true })
      try {
        const event = await eventsApi.pause(projectId)
        set({ event, phase: event.status })
      } finally {
        set({ busy: false })
      }
    },

    resume: async () => {
      const projectId = get().projectId
      if (!projectId) return
      set({ busy: true })
      try {
        const event = await eventsApi.resume(projectId)
        set({ event, phase: event.status })
        if (!es && event) connect(event.id)
      } finally {
        set({ busy: false })
      }
    },

    end: async () => {
      const projectId = get().projectId
      if (!projectId) return
      set({ busy: true })
      try {
        await eventsApi.end(projectId)
        disconnect()
        set({
          event: null,
          phase: 'idle',
          scenario: null,
          messages: [],
          selection: null,
          activeChannel: 'lobby',
          ...EMPTY_SNAPSHOT,
        })
      } finally {
        set({ busy: false })
      }
    },

    reset: async () => {
      await withEvent((id) => modApi.reset(id))
    },

    capture: (id) => withEvent((ev) => modApi.act(ev, id, 'capture')),
    release: (id) => withEvent((ev) => modApi.act(ev, id, 'release')),
    hideMessage: (seq, hidden) => withEvent((ev) => modApi.hideMessage(ev, seq, hidden)),
    broadcast: (scope, cue) => withEvent((ev) => modApi.broadcast(ev, scope, cue)),
    say: (channel, text, as, parentSeq) => withEvent((ev) => modApi.say(ev, channel, text, as, parentSeq)),
    setStat: (id, field, value) => withEvent((ev) => modApi.setStat(ev, id, field, value)),
    fireBeat: (name, subject) => withEvent((ev) => modApi.fireBeat(ev, name, subject)),
    fireSignal: (name, subject) => withEvent((ev) => modApi.fireSignal(ev, name, subject)),
    scanAs: (as, target) => withEvent((ev) => modApi.scanAs(ev, as, target)),
    reveal: (faction) => withEvent((ev) => modApi.reveal(ev, faction)),

    setTab: (activeTab) => set({ activeTab }),
    selectChannel: (activeChannel) => set({ activeChannel }),
    select: (selection) => set({ selection }),
  }
})
