//! Sim mode's cockpit store — the LOCAL simulator. Builds a real
//! `@loom/core` `Sim` from the indexed `.loom` project and drives it
//! entirely in the browser: no server, no event, no account. It
//! implements the shared `CockpitState` contract (see `store/cockpit.ts`)
//! so the Run cockpit's chat / roster / world / director / inspector
//! surfaces work against it unchanged, and it feeds the story canvas's
//! `RuntimeOverlay` the same way Run's mod SSE feed does — the writer
//! watches the *real runtime graph* light up as the story reacts.
//!
//! Message composition + snapshot projection reuse the event server's
//! pure modules (`@loom/core/chat`, `@loom/core/views`) verbatim, so a
//! simulated run reads exactly like the live event would.

import { create } from 'zustand'
import { Sim, SimEventType, namedEvents, type SimEvent } from '@loom/core/sim'
import { ChatStore, composeGuestMessages } from '@loom/core/chat'
import { modView } from '@loom/core/views'
import type { StatField } from '@/lib/api'
import { lspWorkspaceSync, pathForUri } from '@/lib/lsp-client'
import { useGraph } from '@/store/graph'
import { useWorkspace } from '@/store/workspace'
import {
  CockpitPhase,
  CockpitTab,
  OPERATOR_LENS,
  type CockpitState,
  type Selection,
} from '@/store/cockpit'

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/** Simulator lifecycle. */
export const SimStatus = {
  Idle: 'idle',
  Running: 'running',
  Paused: 'paused',
} as const

export type SimStatus = (typeof SimStatus)[keyof typeof SimStatus]

const PHASE_FOR: Record<SimStatus, CockpitPhase> = {
  [SimStatus.Idle]: CockpitPhase.Idle,
  [SimStatus.Running]: CockpitPhase.Open,
  [SimStatus.Paused]: CockpitPhase.Paused,
}

/** The key unbound (no-participant) choice menus queue under. */
export const GLOBAL_CHOICE_KEY = '__global'

/** One ledger row for the Log tab. */
export interface SimLogEntry {
  seq: number
  /** Story-clock ms when the event landed. */
  ts: number
  event: SimEvent
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface SimState extends CockpitState {
  status: SimStatus
  /** Persona ids the writer controls (guests created from the cockpit). */
  personas: string[]
  /** The raw sim ledger, for the Log tab. */
  log: SimLogEntry[]
  /** LSP index generation the running sim was compiled at (stale check). */
  compiledAt: number
  /** The model's `entry:` beat, when it resolves. */
  entryBeat: string | null

  start(): void
  pause(): void
  resume(): void
  /** Stop + rebuild from the CURRENT sources (picks up edits). */
  reset(): void
  stop(): void
  addPersona(name?: string): void
}

// The live engine objects are module-level (non-reactive) — the store
// holds only their projections, refreshed after every command.
let sim: Sim | null = null
let chat: ChatStore | null = null
let ticker: number | null = null
let personaSeq = 0

const TICK_MS = 1000

export const useSim = create<SimState>((set, get) => {
  /** Re-project every snapshot surface out of the live sim. */
  const snapshot = (): void => {
    if (sim === null || chat === null) return
    const view = modView(sim, PHASE_FOR[get().status], get().scenario)
    const choices: Record<string, string[]> = {}
    for (const id of [...get().personas, GLOBAL_CHOICE_KEY]) {
      const pending = sim.pendingChoiceFor(id)
      if (pending !== null) choices[id] = pending
    }
    set({
      roster: view.roster,
      factions: view.factions,
      locations: view.locations,
      cast: view.cast,
      channels: view.channels,
      spaces: view.spaces,
      beats: view.beats,
      ledgerLen: view.ledgerLen,
      messages: [...chat.all()],
      choices,
    })
  }

  /** Compose + project a freshly-drained batch (ledger rows from `from`). */
  const ingest = (from: number): void => {
    if (sim === null || chat === null) return
    const events = sim.log.since(from)
    if (events.length > 0) {
      chat.append(composeGuestMessages(sim, events))
      const ts = sim.elapsed()
      const rows = events.map((event, i) => ({ seq: from + i, ts, event }))
      set((s) => ({ log: [...s.log, ...rows] }))
      // The runtime overlay lights the story map exactly like Run mode.
      for (const e of events) {
        if (e.type === SimEventType.BeatEntered) useGraph.getState().runtimeEnter(e.beat)
      }
    }
    snapshot()
  }

  /** Run one command against the live sim and ingest what it produced. */
  const run = (fn: (s: Sim) => void): void => {
    if (sim === null) return
    const from = sim.log.len()
    try {
      fn(sim)
      set({ error: null })
    } catch (e) {
      set({ error: (e as Error).message })
    }
    ingest(from)
  }

  // The autonomous clock only runs in a browser; headless tests drive
  // the sim through the command surface instead.
  const stopTicker = (): void => {
    if (ticker !== null) {
      window.clearInterval(ticker)
      ticker = null
    }
  }

  const startTicker = (): void => {
    if (typeof window === 'undefined') return
    stopTicker()
    ticker = window.setInterval(() => {
      if (sim !== null && get().status === SimStatus.Running) {
        run((s) => void s.tick(TICK_MS))
      }
    }, TICK_MS)
  }

  return {
    // ---- CockpitState ----
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
    activeTab: CockpitTab.Sim,
    activeChannel: 'lobby',
    selection: null,
    choices: {},
    perspective: OPERATOR_LENS,

    setTab: (activeTab) => set({ activeTab }),
    selectChannel: (activeChannel) => set({ activeChannel }),
    select: (selection: Selection) => set({ selection }),
    setPerspective: (perspective) => set({ perspective }),

    // The operator/mod command surface, mirrored onto the local sim with
    // the exact semantics of the server's `/api/mod/*` routes.
    say: async (channel, text, as, parentSeq) => {
      const t = text.trim()
      if (t.length === 0) return
      run((s) => {
        const speaker = as !== undefined && as !== '' ? as : 'Operator'
        let ch = channel || 'lobby'
        let audience: 'all' | string[] | undefined
        if (ch.startsWith('guest:')) {
          const gid = ch.slice('guest:'.length)
          if (!s.persons.has(gid)) return
          ch = `dm:${speaker}`
          audience = [gid]
        }
        const parent = s.threadableOf(ch) && parentSeq != null ? parentSeq : null
        s.say(speaker, ch, t, parent, audience)
      })
    },
    hideMessage: async (seq, hidden) => {
      if (chat === null) return
      chat.setHidden(seq, hidden)
      snapshot()
    },
    capture: async (id) => run((s) => void s.capture(id)),
    release: async (id) => run((s) => void s.escape(id)),
    broadcast: async (scope, cue) => {
      // Like the server: a mod broadcast is composed straight to chat
      // (it is not a journaled sim event).
      if (sim === null || chat === null) return
      const synthetic: SimEvent = {
        type: SimEventType.Broadcast,
        cue,
        audience: sim.audienceFor(scope),
        scope,
      }
      chat.append(composeGuestMessages(sim, [synthetic]))
      snapshot()
    },
    setStat: async (id, field: StatField, value) => {
      run((s) => {
        switch (field) {
          case 'score':
            s.setScore(id, Number(value ?? 0))
            break
          case 'faction':
            if (typeof value === 'string' && value !== '') s.defect(id, value)
            break
          case 'location':
            if (typeof value === 'string' && value !== '') s.arrive(id, value)
            break
          case 'captured':
            if (value === true || value === 'true') s.capture(id)
            else s.escape(id)
            break
        }
      })
    },
    fireBeat: async (name, subject) => run((s) => void s.fireBeat(name, subject)),
    fireSignal: async (name, subject) => run((s) => void s.signal(name, subject)),
    scanAs: async (as, target) => run((s) => void s.scan(as, target)),
    reveal: async (faction) => run((s) => void s.reveal(faction)),
    choose: async (person, index) => run((s) => void s.choose(person, index)),

    // ---- Sim lifecycle ----
    status: SimStatus.Idle,
    personas: [],
    log: [],
    compiledAt: -1,
    entryBeat: null,

    start: () => {
      const ws = lspWorkspaceSync()
      const sources = [...ws.docs].map(([uri, doc]) => ({
        path: pathForUri(uri),
        source: doc.text,
      }))
      if (sources.length === 0) {
        set({ error: 'No .loom files indexed yet.' })
        return
      }
      sim = Sim.fromSources(...sources)
      chat = new ChatStore()
      personaSeq = 0
      useGraph.getState().runtimeReset()
      const entry = sim.model.entry !== null && sim.model.beats.has(sim.model.entry)
        ? sim.model.entry
        : null
      set({
        status: SimStatus.Running,
        phase: CockpitPhase.Open,
        connected: true,
        live: true,
        scenario: useWorkspace.getState().root?.name ?? 'workspace',
        events: namedEvents(sim.model),
        personas: [],
        log: [],
        messages: [],
        choices: {},
        selection: null,
        perspective: OPERATOR_LENS,
        error: null,
        compiledAt: ws.generation,
        entryBeat: entry,
      })
      // A first persona so scans / DMs / per-guest beats have a subject,
      // then the entry beat (when authored) so the story starts playing.
      // Each command ingests its own ledger slice.
      get().addPersona('Writer')
      if (entry !== null) run((s) => void s.fireBeat(entry))
      snapshot()
      startTicker()
    },

    pause: () => {
      if (get().status !== SimStatus.Running) return
      set({ status: SimStatus.Paused, phase: CockpitPhase.Paused })
    },

    resume: () => {
      if (get().status !== SimStatus.Paused) return
      set({ status: SimStatus.Running, phase: CockpitPhase.Open })
    },

    reset: () => {
      get().stop()
      get().start()
    },

    stop: () => {
      stopTicker()
      sim = null
      chat = null
      useGraph.getState().runtimeReset()
      set({
        status: SimStatus.Idle,
        phase: CockpitPhase.Idle,
        connected: false,
        live: false,
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
        personas: [],
        log: [],
        choices: {},
        selection: null,
        perspective: OPERATOR_LENS,
        error: null,
        entryBeat: null,
      })
    },

    addPersona: (name) => {
      if (sim === null) return
      personaSeq += 1
      const id = `p${personaSeq}`
      const label = name !== undefined && name.trim() !== '' ? name.trim() : `Guest ${personaSeq}`
      set((s) => ({ personas: [...s.personas, id] }))
      run((s) => void s.createPerson(id, label))
    },
  }
})
