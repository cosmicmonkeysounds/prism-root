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
import { namedEvents } from '@loom/core/sim'
import { eventsApi, modApi, type EventInfo } from '@/lib/api'
import { lspWorkspaceSync } from '@/lib/lsp-client'
import { useGraph } from '@/store/graph'
import {
  CockpitTab,
  OPERATOR_LENS,
  type CastSummary,
  type ChannelSummary,
  type CockpitMessage,
  type CockpitState,
  type FactionSummary,
  type LocationSummary,
  type RosterRow,
  type Selection,
  type SpaceSummary,
} from '@/store/cockpit'

// The shared cockpit shapes (roster rows, messages, faction/location
// summaries, selection) live in `store/cockpit.ts` — Sim mode renders
// the identical surfaces off its local simulator.

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
  choices: Record<string, string[]>
  ledgerLen: number
}

interface OperateState extends CockpitState {
  projectId: string | null
  event: EventInfo | null
  busy: boolean

  init: (projectId: string) => Promise<void>
  teardown: () => void
  launch: (mode: 'live' | 'preview') => Promise<void>
  pause: () => Promise<void>
  resume: () => Promise<void>
  end: () => Promise<void>
  reset: () => Promise<void>
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
        choices: snap.choices ?? {},
        ledgerLen: snap.ledgerLen ?? 0,
      })
    })
    source.addEventListener('history', (e) => {
      const msgs = JSON.parse((e as MessageEvent).data) as CockpitMessage[]
      set({ messages: msgs })
    })
    const upsert = (m: CockpitMessage) =>
      set((s) => {
        const rest = s.messages.filter((x) => x.seq !== m.seq)
        return { messages: [...rest, m].sort((a, b) => a.seq - b.seq) }
      })
    source.addEventListener('message', (e) => upsert(JSON.parse((e as MessageEvent).data) as CockpitMessage))
    source.addEventListener('messageModerated', (e) => {
      const d = JSON.parse((e as MessageEvent).data) as CockpitMessage
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
    events: [],
    ledgerLen: 0,
    messages: [],
    connected: false,
    live: false,
    busy: false,
    error: null,
    activeTab: CockpitTab.Event,
    activeChannel: 'lobby',
    selection: null,
    choices: {},
    perspective: OPERATOR_LENS,

    init: async (projectId) => {
      if (get().projectId === projectId && get().event) return // already live for this project
      set({
        projectId,
        error: null,
        messages: [],
        selection: null,
        perspective: OPERATOR_LENS,
        activeTab: CockpitTab.Event,
        activeChannel: 'lobby',
        // Named events come from the locally-indexed model (best-effort —
        // the running event's code is a snapshot of the same project).
        events: localNamedEvents(),
        ...EMPTY_SNAPSHOT,
      })
      try {
        const event = await eventsApi.status(projectId)
        set({ event, live: event !== null, phase: event?.status ?? 'idle' })
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
        live: false,
        phase: 'idle',
        scenario: null,
        messages: [],
        selection: null,
        perspective: OPERATOR_LENS,
        activeTab: CockpitTab.Event,
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
        set({ event, live: true, phase: event.status, events: localNamedEvents() })
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
          live: false,
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
    // The mod snapshot carries pending choices, and this answers one on a
    // guest's behalf — journaled exactly like the guest's own tap.
    choose: (person, index) => withEvent((ev) => modApi.choose(ev, person, index)),

    setTab: (activeTab) => set({ activeTab }),
    selectChannel: (activeChannel) => set({ activeChannel }),
    select: (selection: Selection) => set({ selection }),
    setPerspective: (perspective) => set({ perspective }),
  }
})

/** Authored named events from the locally-indexed model (may be empty
 *  before the first successful compile). */
function localNamedEvents(): string[] {
  try {
    const model = lspWorkspaceSync().model()
    return model !== null ? namedEvents(model) : []
  } catch {
    return []
  }
}
