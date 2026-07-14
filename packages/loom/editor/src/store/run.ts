//! Run mode's source switch. The one cockpit rehearses OR moderates:
//! `Sim` drives the local in-browser `@loom/core` simulator
//! (`store/sim.ts`), `Live` drives the launched event over the mod SSE
//! (`store/operate.ts`). The stage, rail, and inspector tray all follow
//! this switch, mounting the matching `CockpitContext` provider.

import { create } from 'zustand'
import { useWorkspace } from '@/store/workspace'

export const RunSource = {
  Sim: 'sim',
  Live: 'live',
} as const

export type RunSource = (typeof RunSource)[keyof typeof RunSource]

type RunState = {
  source: RunSource
  setSource(source: RunSource): void
}

export const useRun = create<RunState>((set) => ({
  source: RunSource.Sim,
  setSource: (source) => set({ source }),
}))

/** Whether Run mode is actually on the live backend right now: the
 *  Live source is selected AND a server project is open (local folders
 *  have no event plane, so they always resolve to the simulator). */
export function useLiveRun(): boolean {
  const source = useRun((s) => s.source)
  const projectId = useWorkspace((s) => s.projectId)
  return source === RunSource.Live && projectId !== null
}
