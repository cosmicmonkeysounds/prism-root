// The story edit journal — cross-surface undo/redo over workspace
// buffers: record/undo/redo roundtrip, the stale-buffer conflict guard
// (atomic across multi-file entries), and lifecycle (cap, project
// switch clears).

import { beforeEach, describe, expect, it } from 'vitest'
import { useEditJournal } from './edit-journal'
import { useWorkspace, type OpenFile } from './workspace'

const A = 'proj/a.loom'
const B = 'proj/b.loom'

function openFile(path: string, contents: string): OpenFile {
  return { path, backend: 'server', projectId: 'test-project', contents, dirty: false }
}

function seed(files: Record<string, string>) {
  const openFiles: Record<string, OpenFile> = {}
  for (const [path, contents] of Object.entries(files)) openFiles[path] = openFile(path, contents)
  useWorkspace.setState({
    projectId: 'test-project',
    openFiles,
    tabOrder: Object.keys(files),
    activePath: Object.keys(files)[0] ?? null,
  })
}

function contentsOf(path: string): string | undefined {
  return useWorkspace.getState().openFiles[path]?.contents
}

beforeEach(() => {
  seed({}) // fires the project-key subscriber once so tests start stable
  useEditJournal.setState({ undoStack: [], redoStack: [], notice: null })
})

describe('edit journal', () => {
  it('undo restores the before-text and redo re-applies', async () => {
    seed({ [A]: 'v2' }) // the post-edit buffer
    useEditJournal.getState().record('Connect a → b', [{ path: A, before: 'v1', after: 'v2' }])

    expect(await useEditJournal.getState().undo()).toBe('Connect a → b')
    expect(contentsOf(A)).toBe('v1')
    expect(useEditJournal.getState().undoStack).toHaveLength(0)
    expect(useEditJournal.getState().redoStack).toHaveLength(1)

    expect(await useEditJournal.getState().redo()).toBe('Connect a → b')
    expect(contentsOf(A)).toBe('v2')
    expect(useEditJournal.getState().undoStack).toHaveLength(1)
  })

  it('drops a stale entry instead of clobbering newer text', async () => {
    seed({ [A]: 'v3' }) // the buffer moved on since the edit
    useEditJournal.getState().record('Edit line', [{ path: A, before: 'v1', after: 'v2' }])

    expect(await useEditJournal.getState().undo()).toBeNull()
    expect(contentsOf(A)).toBe('v3') // untouched
    expect(useEditJournal.getState().undoStack).toHaveLength(0) // dropped
    expect(useEditJournal.getState().redoStack).toHaveLength(0)
    expect(useEditJournal.getState().notice?.text).toContain('changed since')
  })

  it('multi-file entries apply atomically — one conflict blocks all', async () => {
    seed({ [A]: 'a2', [B]: 'b-moved' }) // B no longer holds the expected text
    useEditJournal.getState().record('Rename x → y', [
      { path: A, before: 'a1', after: 'a2' },
      { path: B, before: 'b1', after: 'b2' },
    ])

    expect(await useEditJournal.getState().undo()).toBeNull()
    expect(contentsOf(A)).toBe('a2') // A untouched even though it matched
    expect(contentsOf(B)).toBe('b-moved')
  })

  it('filters no-op edits and skips empty entries', () => {
    useEditJournal.getState().record('nothing', [{ path: A, before: 'same', after: 'same' }])
    expect(useEditJournal.getState().undoStack).toHaveLength(0)
  })

  it('a new edit clears the redo stack', async () => {
    seed({ [A]: 'v2' })
    useEditJournal.getState().record('first', [{ path: A, before: 'v1', after: 'v2' }])
    await useEditJournal.getState().undo()
    expect(useEditJournal.getState().redoStack).toHaveLength(1)

    seed({ [A]: 'v9' })
    useEditJournal.getState().record('second', [{ path: A, before: 'v1', after: 'v9' }])
    expect(useEditJournal.getState().redoStack).toHaveLength(0)
  })

  it('caps the stack at 100 entries', () => {
    seed({ [A]: 'x' })
    for (let i = 0; i < 120; i++) {
      useEditJournal.getState().record(`edit ${i}`, [{ path: A, before: `${i}`, after: `${i + 1}` }])
    }
    const stack = useEditJournal.getState().undoStack
    expect(stack).toHaveLength(100)
    expect(stack[stack.length - 1]!.label).toBe('edit 119')
  })

  it('clears when the project identity changes', () => {
    seed({ [A]: 'v2' })
    useEditJournal.getState().record('edit', [{ path: A, before: 'v1', after: 'v2' }])
    expect(useEditJournal.getState().undoStack).toHaveLength(1)

    useWorkspace.setState({ projectId: 'another-project' })
    expect(useEditJournal.getState().undoStack).toHaveLength(0)
  })
})
