//! Selecting an entity (guest / character / faction / location) and making
//! sure the Inspector tray is visible — shared by the Event page, Roster,
//! World, and the Sim setup tab. Works in whichever mode hosts the cockpit.

import { useCockpit, type Selection } from '@/store/cockpit'
import { useMode } from '@/store/mode'

export function useInspect() {
  const select = useCockpit((s) => s.select)
  return (sel: Selection) => {
    select(sel)
    const mode = useMode.getState().mode
    useMode.getState().setUi(mode, { trayOpen: true })
  }
}
