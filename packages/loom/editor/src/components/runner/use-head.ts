// Phase 4 of the Loom IDE redesign: shared selector for the active
// head's state. Every Runner panel that used to read fields off
// `active.play.{transcript,meta,world,…}` goes through this instead,
// so the head-keyed shape (`active.play.heads[primary]`) is opaque to
// the panels.
//
// Returning the head object directly means components depend on the
// reference equality of one head — switching primary heads or
// receiving a new server snapshot both produce new references, which
// is exactly when the panel needs to re-render.

import { useSession } from '@/store/session'
import { primaryHead, type PlayHeadState } from '@/lib/sync'

export function useActiveHead(): PlayHeadState | null {
  return useSession((s) => primaryHead(s.active?.play ?? null))
}

export function usePrimaryHeadId(): string {
  return useSession((s) => s.active?.play?.primary ?? 'h0')
}
