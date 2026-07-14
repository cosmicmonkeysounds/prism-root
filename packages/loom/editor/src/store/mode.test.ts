// The v3 modal topology: three modes (Writing / Run / Deploy), the
// Writing center split persisted per mode, and migration of the
// pre-v3 persisted mode ids (editing→writing, sim/operate→run).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// The store reads localStorage at module-import time, so each case
// seeds a fake store first and dynamically imports a fresh module.
class FakeStorage {
  private map = new Map<string, string>()
  getItem(k: string): string | null {
    return this.map.get(k) ?? null
  }
  setItem(k: string, v: string): void {
    this.map.set(k, v)
  }
  removeItem(k: string): void {
    this.map.delete(k)
  }
}

let storage: FakeStorage

beforeEach(() => {
  storage = new FakeStorage()
  vi.stubGlobal('localStorage', storage)
  vi.resetModules()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

async function freshMode() {
  return await import('@/store/mode')
}

describe('mode store (v3 topology)', () => {
  it('exposes exactly Writing / Run / Deploy on ⌘1..⌘3', async () => {
    const { MODES } = await freshMode()
    expect(MODES.map((m) => m.id)).toEqual(['writing', 'run', 'deploy'])
    expect(MODES.map((m) => m.hint)).toEqual(['⌘1', '⌘2', '⌘3'])
    // The BeatStrip timeline dock rides the merged Writing mode.
    expect(MODES.find((m) => m.id === 'writing')?.hasTimeline).toBe(true)
  })

  it('defaults to writing with the editor⇄graph split present', async () => {
    const { useMode } = await freshMode()
    const s = useMode.getState()
    expect(s.mode).toBe('writing')
    expect(s.ui.writing.split).toHaveLength(2)
    expect(s.ui.writing.split.every((n) => n >= 280)).toBe(true)
  })

  it.each([
    ['editing', 'writing'],
    ['sim', 'run'],
    ['operate', 'run'],
  ])('migrates persisted legacy mode %s → %s', async (legacy, expected) => {
    storage.setItem('loom.studio', JSON.stringify({ mode: legacy }))
    const { useMode } = await freshMode()
    expect(useMode.getState().mode).toBe(expected)
  })

  it('drops unknown persisted modes back to writing', async () => {
    storage.setItem('loom.studio', JSON.stringify({ mode: 'bogus' }))
    const { useMode } = await freshMode()
    expect(useMode.getState().mode).toBe('writing')
  })

  it('sanitizes stale ui blobs missing the split field', async () => {
    storage.setItem(
      'loom.studio',
      JSON.stringify({
        mode: 'writing',
        ui: { writing: { cols: [10, 10, 10], rows: [10, 10] } },
      }),
    )
    const { useMode } = await freshMode()
    const ui = useMode.getState().ui.writing
    expect(ui.cols[0]).toBeGreaterThanOrEqual(160) // clamped
    expect(ui.split.every((n) => n >= 280)).toBe(true) // defaulted
    expect(ui.graphOpen).toBe(true) // defaulted
  })

  it('persists the graph-pane toggle (⌘\\) round-trip', async () => {
    const { useMode } = await freshMode()
    useMode.getState().setUi('writing', { graphOpen: false })
    const raw = JSON.parse(storage.getItem('loom.studio') ?? '{}') as {
      ui: { writing: { graphOpen: boolean } }
    }
    expect(raw.ui.writing.graphOpen).toBe(false)
  })

  it('persists split edits round-trip', async () => {
    const { useMode } = await freshMode()
    useMode.getState().setUi('writing', { split: [400, 800] })
    const raw = JSON.parse(storage.getItem('loom.studio') ?? '{}') as {
      ui: { writing: { split: [number, number] } }
    }
    expect(raw.ui.writing.split).toEqual([400, 800])
  })
})
