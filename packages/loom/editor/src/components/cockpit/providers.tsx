//! Cockpit providers — bind the shared cockpit components to a backend.
//! `OperateCockpit` supplies the live-event store (mod SSE + `/api/mod/*`);
//! `SimCockpit` supplies the local in-browser simulator. Both stores are
//! module singletons, so the provider value is referentially stable.

import type { ReactNode } from 'react'
import { CockpitContext } from '@/store/cockpit'
import { useOperate } from '@/store/operate'
import { useSim } from '@/store/sim'
import { useLiveRun } from '@/store/run'

export function OperateCockpit({ children }: { children: ReactNode }) {
  return <CockpitContext.Provider value={useOperate}>{children}</CockpitContext.Provider>
}

export function SimCockpit({ children }: { children: ReactNode }) {
  return <CockpitContext.Provider value={useSim}>{children}</CockpitContext.Provider>
}

/** Run mode's provider — follows the Sim ⇄ Live source switch, so the
 *  stage, rooms rail, and inspector tray all peer into one backend. */
export function RunCockpit({ children }: { children: ReactNode }) {
  const live = useLiveRun()
  return <CockpitContext.Provider value={live ? useOperate : useSim}>{children}</CockpitContext.Provider>
}
