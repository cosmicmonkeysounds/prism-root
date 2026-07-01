//! The Operate mode's shared event-control core.
//!
//! One store drives both the Event sidebar (launch / codes / lifecycle) and
//! the Operate stage (live roster · moderation · feed · broadcast) — the
//! "run panel" and the "admin tools" are the same capability, so they share
//! this state. Launch/lifecycle go through the control plane
//! (`/api/projects/:id/event`); the live view + moderation ride the per-event
//! mod SSE + `/e/:eventId/api/mod/*`, authorized by the author's session.

import { create } from 'zustand'
import { eventsApi, modApi, type EventInfo } from '@/lib/api'

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
  from: string
  text: string
  kind: string
  ts: number
  hidden?: boolean
  audience: 'all' | string[]
  parentSeq?: number | null
}

interface ModSnapshot {
  phase: string
  roster: RosterRow[]
  factions: { id: string; hidden: boolean; revealed: boolean; members: string[] }[]
}

type OperateState = {
  projectId: string | null
  event: EventInfo | null
  phase: string
  roster: RosterRow[]
  factions: ModSnapshot['factions']
  messages: OperateMessage[]
  connected: boolean
  busy: boolean
  error: string | null

  init: (projectId: string) => Promise<void>
  teardown: () => void
  launch: (mode: 'live' | 'preview') => Promise<void>
  pause: () => Promise<void>
  resume: () => Promise<void>
  end: () => Promise<void>
  capture: (id: string) => Promise<void>
  release: (id: string) => Promise<void>
  hideMessage: (seq: number, hidden: boolean) => Promise<void>
  broadcast: (scope: string, cue: string) => Promise<void>
}

// One live mod stream at a time (Operate mode is a single surface).
let es: EventSource | null = null

export const useOperate = create<OperateState>((set, get) => {
  const disconnect = () => {
    if (es) {
      es.close()
      es = null
    }
    set({ connected: false })
  }

  const connect = (eventId: string) => {
    disconnect()
    const source = new EventSource(`/e/${encodeURIComponent(eventId)}/events?role=mod`)
    source.onopen = () => set({ connected: true })
    source.onerror = () => set({ connected: false })
    source.addEventListener('snapshot', (e) => {
      const snap = JSON.parse((e as MessageEvent).data) as ModSnapshot
      set({ phase: snap.phase, roster: snap.roster ?? [], factions: snap.factions ?? [] })
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
    es = source
  }

  return {
    projectId: null,
    event: null,
    phase: 'idle',
    roster: [],
    factions: [],
    messages: [],
    connected: false,
    busy: false,
    error: null,

    init: async (projectId) => {
      if (get().projectId === projectId && get().event) return // already live for this project
      set({ projectId, error: null, messages: [], roster: [] })
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
      set({ projectId: null, event: null, phase: 'idle', roster: [], factions: [], messages: [] })
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
        set({ event: null, phase: 'idle', roster: [], factions: [], messages: [] })
      } finally {
        set({ busy: false })
      }
    },

    capture: async (id) => {
      const ev = get().event
      if (ev) await modApi.act(ev.id, id, 'capture')
    },
    release: async (id) => {
      const ev = get().event
      if (ev) await modApi.act(ev.id, id, 'release')
    },
    hideMessage: async (seq, hidden) => {
      const ev = get().event
      if (ev) await modApi.hideMessage(ev.id, seq, hidden)
    },
    broadcast: async (scope, cue) => {
      const ev = get().event
      if (ev) await modApi.broadcast(ev.id, scope, cue)
    },
  }
})
