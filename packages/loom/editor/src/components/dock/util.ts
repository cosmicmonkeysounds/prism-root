// Dock helpers split out of DockShell.tsx so the file can remain a
// pure component module (react-refresh requires that). Re-exported
// for any non-component consumer (e.g. the workspace-preset store and
// the StatusBar preset switcher).

import type { AddPanelPositionOptions, DockviewApi } from 'dockview-react'
import type { PanelId } from './panel-registry'

/** Live dockview API once the shell mounts — set by `DockShell` on
 *  `onReady`. Reads return `null` before the dock is mounted. */
let activeDockApi: DockviewApi | null = null

export function getActiveDockApi(): DockviewApi | null {
  return activeDockApi
}

export function setActiveDockApi(api: DockviewApi | null): void {
  activeDockApi = api
}

/**
 * Where to put a panel when it's added to an otherwise-populated
 * dock. Returning `undefined` lets dockview pick (used for the very
 * first panel added).
 */
export function positionFor(
  api: DockviewApi,
  id: PanelId,
): AddPanelPositionOptions | undefined {
  const editor = api.getPanel('editor')
  const canvas = api.getPanel('canvas')
  const files = api.getPanel('files')
  const remote = api.getPanel('remote')

  if (id === 'files') {
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'search') {
    if (files) return { referencePanel: files.id, direction: 'within' }
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'cloud') {
    if (files) return { referencePanel: files.id, direction: 'within' }
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'editor') {
    if (canvas) return { referencePanel: canvas.id, direction: 'above' }
    if (files) return { referencePanel: files.id, direction: 'right' }
    return undefined
  }
  if (id === 'remote') {
    if (editor) return { referencePanel: editor.id, direction: 'within' }
    if (canvas) return { referencePanel: canvas.id, direction: 'above' }
    return undefined
  }
  if (id === 'play') {
    if (remote) return { referencePanel: remote.id, direction: 'right' }
    if (editor) return { referencePanel: editor.id, direction: 'right' }
    return undefined
  }
  if (id === 'transcript') {
    if (editor) return { referencePanel: editor.id, direction: 'right' }
    return undefined
  }
  if (id === 'choices') {
    const transcript = api.getPanel('transcript')
    if (transcript) return { referencePanel: transcript.id, direction: 'below' }
    if (editor) return { referencePanel: editor.id, direction: 'right' }
    return undefined
  }
  if (id === 'timeline') {
    if (canvas) return { referencePanel: canvas.id, direction: 'within' }
    if (editor) return { referencePanel: editor.id, direction: 'below' }
    return undefined
  }
  if (id === 'outline' || id === 'references') {
    if (files) return { referencePanel: files.id, direction: 'within' }
    if (editor) return { referencePanel: editor.id, direction: 'left' }
    return undefined
  }
  if (id === 'booth' || id === 'cast') {
    const timeline = api.getPanel('timeline')
    if (timeline) return { referencePanel: timeline.id, direction: 'right' }
    if (editor) return { referencePanel: editor.id, direction: 'right' }
    return undefined
  }
  if (
    id === 'ledger' ||
    id === 'world' ||
    id === 'inspector' ||
    id === 'graph' ||
    id === 'detail'
  ) {
    const timeline = api.getPanel('timeline')
    if (timeline) return { referencePanel: timeline.id, direction: 'within' }
    if (canvas) return { referencePanel: canvas.id, direction: 'within' }
    if (editor) return { referencePanel: editor.id, direction: 'below' }
    return undefined
  }
  // canvas
  if (remote) return { referencePanel: remote.id, direction: 'below' }
  if (editor) return { referencePanel: editor.id, direction: 'below' }
  if (files) return { referencePanel: files.id, direction: 'right' }
  return undefined
}
