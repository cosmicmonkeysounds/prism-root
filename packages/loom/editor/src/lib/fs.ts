/**
 * Thin wrapper over the File System Access API.
 * Spec: https://wicg.github.io/file-system-access/
 */

export type FsEntry = {
  name: string
  path: string
  kind: 'file' | 'directory'
  handle: FileSystemFileHandle | FileSystemDirectoryHandle
  children?: FsEntry[]
}

export function isFsAccessSupported(): boolean {
  return typeof window !== 'undefined' && 'showDirectoryPicker' in window
}

export async function pickDirectory(): Promise<FileSystemDirectoryHandle> {
  return await (window as unknown as {
    showDirectoryPicker: (opts: { mode: 'read' | 'readwrite' }) => Promise<FileSystemDirectoryHandle>
  }).showDirectoryPicker({ mode: 'readwrite' })
}

const IGNORED = new Set(['node_modules', '.git', '.DS_Store', 'dist', '.next', '.turbo'])

export async function readDirectoryTree(
  dir: FileSystemDirectoryHandle,
  basePath = '',
): Promise<FsEntry> {
  const children: FsEntry[] = []
  const iter = (dir as unknown as {
    entries: () => AsyncIterableIterator<[string, FileSystemHandle]>
  }).entries()
  for await (const [name, handle] of iter) {
    if (IGNORED.has(name)) continue
    const path = basePath ? `${basePath}/${name}` : name
    if (handle.kind === 'directory') {
      children.push(await readDirectoryTree(handle as FileSystemDirectoryHandle, path))
    } else {
      children.push({ name, path, kind: 'file', handle: handle as FileSystemFileHandle })
    }
  }
  children.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === 'directory' ? -1 : 1
    return a.name.localeCompare(b.name)
  })
  return {
    name: dir.name,
    path: basePath || dir.name,
    kind: 'directory',
    handle: dir,
    children,
  }
}

export async function readFileText(handle: FileSystemFileHandle): Promise<string> {
  const file = await handle.getFile()
  return await file.text()
}

export async function writeFileText(
  handle: FileSystemFileHandle,
  contents: string,
): Promise<void> {
  const writable = await handle.createWritable()
  await writable.write(contents)
  await writable.close()
}

export async function createFile(
  dir: FileSystemDirectoryHandle,
  name: string,
): Promise<FileSystemFileHandle> {
  return await dir.getFileHandle(name, { create: true })
}

export async function createDirectory(
  dir: FileSystemDirectoryHandle,
  name: string,
): Promise<FileSystemDirectoryHandle> {
  return await dir.getDirectoryHandle(name, { create: true })
}

export async function removeEntry(
  dir: FileSystemDirectoryHandle,
  name: string,
  recursive = false,
): Promise<void> {
  await (dir as unknown as {
    removeEntry: (name: string, opts?: { recursive?: boolean }) => Promise<void>
  }).removeEntry(name, { recursive })
}

type MoveCapable = {
  move?: (newName: string) => Promise<void>
}

export async function renameHandle(
  handle: FileSystemFileHandle | FileSystemDirectoryHandle,
  newName: string,
): Promise<boolean> {
  const h = handle as unknown as MoveCapable
  if (typeof h.move !== 'function') return false
  await h.move(newName)
  return true
}

type PermState = 'granted' | 'denied' | 'prompt'

type PermissionCapable = {
  queryPermission?: (opts: { mode: 'read' | 'readwrite' }) => Promise<PermState>
  requestPermission?: (opts: { mode: 'read' | 'readwrite' }) => Promise<PermState>
}

export async function queryHandlePermission(
  handle: FileSystemHandle,
  mode: 'read' | 'readwrite' = 'readwrite',
): Promise<PermState> {
  const h = handle as unknown as PermissionCapable
  if (!h.queryPermission) return 'prompt'
  return await h.queryPermission({ mode })
}

export async function requestHandlePermission(
  handle: FileSystemHandle,
  mode: 'read' | 'readwrite' = 'readwrite',
): Promise<PermState> {
  const h = handle as unknown as PermissionCapable
  if (!h.requestPermission) return 'denied'
  return await h.requestPermission({ mode })
}

/**
 * FileSystemObserver: native push-based change notifications.
 * Shipping in Chromium 129+ behind `chrome://flags/#file-system-observer` historically,
 * unflagged in recent releases. Returns null if unavailable so callers can degrade gracefully.
 *
 * Spec draft: https://github.com/WICG/file-system-access/blob/main/proposals/FileSystemObserver.md
 */
export type FsChangeRecord = {
  type: 'appeared' | 'disappeared' | 'modified' | 'moved' | 'unknown' | 'errored'
  relativePathComponents: string[]
  changedHandle?: FileSystemHandle
}

type ObserverCtor = new (
  cb: (records: FsChangeRecord[]) => void,
) => {
  observe: (handle: FileSystemHandle, opts?: { recursive?: boolean }) => Promise<void>
  unobserve: (handle: FileSystemHandle) => void
  disconnect: () => void
}

export function isFsObserverSupported(): boolean {
  return typeof window !== 'undefined' && 'FileSystemObserver' in window
}

export function createFsObserver(
  cb: (records: FsChangeRecord[]) => void,
): InstanceType<ObserverCtor> | null {
  if (!isFsObserverSupported()) return null
  const Ctor = (window as unknown as { FileSystemObserver: ObserverCtor }).FileSystemObserver
  return new Ctor(cb)
}
