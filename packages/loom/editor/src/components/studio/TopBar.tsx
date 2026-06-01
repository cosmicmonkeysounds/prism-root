// Phase 5 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// §17.5): the global top bar. Workspace label + a transport (play /
// stop / fork / snapshot wired to the session), live relay status, and
// a presence strip. Replaces the static "Loom · IDE + Canvas" header.

import type { ReactNode } from 'react'
import clsx from 'clsx'
import { useSession } from '@/store/session'
import { useMode } from '@/store/mode'

export function TopBar() {
  const status = useSession((s) => s.status)
  const wsName = useSession((s) => s.active?.meta.name ?? null)
  const username = useSession((s) => s.username)
  return (
    <header className="h-9 px-3 flex items-center gap-3 border-b border-white/10 bg-zinc-950 text-sm shrink-0">
      <span className="font-semibold tracking-wide">Loom</span>
      <span className="text-zinc-500 text-xs truncate max-w-[220px]">
        {wsName ?? 'local workspace'}
      </span>
      <Transport />
      <div className="ml-auto flex items-center gap-3">
        <Presence />
        <ConnDot status={status} />
        {username && <span className="text-zinc-500 text-xs">{username}</span>}
        <ResetLayout />
      </div>
    </header>
  )
}

function Transport() {
  const active = useSession((s) => s.active)
  const play = useSession((s) => s.active?.play ?? null)
  const startPlay = useSession((s) => s.startPlay)
  const stopPlay = useSession((s) => s.stopPlay)
  const forkPlay = useSession((s) => s.forkPlay)
  const snapshotPlay = useSession((s) => s.snapshotPlay)
  const running = !!play

  return (
    <div className="flex items-center gap-1">
      {running ? (
        <TButton title="Stop play" onClick={stopPlay} accent="text-rose-300">
          ■
        </TButton>
      ) : (
        <TButton
          title={active ? 'Start play' : 'Open a workspace to play'}
          onClick={startPlay}
          disabled={!active}
          accent="text-emerald-300"
        >
          ▶
        </TButton>
      )}
      <TButton title="Fork head" onClick={() => forkPlay({ parent: play?.primary })} disabled={!running}>
        ⑂
      </TButton>
      <TButton title="Snapshot head" onClick={() => snapshotPlay({ head: play?.primary })} disabled={!running}>
        📌
      </TButton>
      {running && (
        <span className="text-zinc-600 text-xs ml-1">
          {play.heads.length} head{play.heads.length === 1 ? '' : 's'}
        </span>
      )}
    </div>
  )
}

function TButton({
  children,
  onClick,
  title,
  disabled,
  accent,
}: {
  children: ReactNode
  onClick: () => void
  title: string
  disabled?: boolean
  accent?: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={clsx(
        'h-6 w-6 grid place-items-center rounded text-xs',
        disabled ? 'text-zinc-700 cursor-default' : clsx('hover:bg-white/10', accent ?? 'text-zinc-300'),
      )}
    >
      {children}
    </button>
  )
}

function Presence() {
  const peers = useSession((s) => s.active?.peers ?? [])
  if (peers.length === 0) return null
  return (
    <div className="flex items-center -space-x-1" title={`${peers.length} collaborator${peers.length === 1 ? '' : 's'}`}>
      {peers.slice(0, 6).map((p) => (
        <span
          key={p.peerId}
          title={p.displayName ?? p.peerId}
          className="h-5 w-5 rounded-full grid place-items-center text-[9px] font-semibold text-black ring-1 ring-zinc-950"
          style={{ background: p.color ?? '#9ca3af' }}
        >
          {(p.displayName ?? p.peerId).slice(0, 2).toUpperCase()}
        </span>
      ))}
      {peers.length > 6 && <span className="text-zinc-500 text-[10px] pl-2">+{peers.length - 6}</span>}
    </div>
  )
}

function ConnDot({ status }: { status: string }) {
  const color =
    status === 'connected'
      ? 'bg-emerald-400'
      : status === 'error'
        ? 'bg-rose-400'
        : status === 'idle'
          ? 'bg-zinc-600'
          : 'bg-amber-400'
  return <span title={`relay: ${status}`} className={clsx('h-2 w-2 rounded-full', color)} />
}

function ResetLayout() {
  const mode = useMode((s) => s.mode)
  const reset = useMode((s) => s.reset)
  return (
    <button
      type="button"
      onClick={() => reset(mode)}
      title="Reset this mode's layout"
      className="text-zinc-600 hover:text-zinc-300 text-xs"
    >
      ⤢
    </button>
  )
}
