//! Selecting an entity (guest / character / faction / location) and making sure
//! the Inspector tray is visible — shared by the Event page, Roster, and World.

import { useOperate, type Selection } from '@/store/operate'
import { useMode } from '@/store/mode'

export function useInspect() {
  const select = useOperate((s) => s.select)
  return (sel: Selection) => {
    select(sel)
    useMode.getState().setUi('operate', { trayOpen: true })
  }
}
