//! Operate mode — center stage: the live admin surface. Roster with
//! moderation, an operator broadcast composer, and the message feed with
//! per-message hide toggles. Owns the mod-stream lifecycle for the mode.

import { useEffect, useState } from 'react'
import { useWorkspace } from '@/store/workspace'
import { useOperate } from '@/store/operate'

function Roster() {
  const { roster, capture, release } = useOperate()
  if (roster.length === 0) return <div className="p-4 text-sm text-zinc-600">No participants yet.</div>
  return (
    <table className="w-full text-sm">
      <thead className="text-left text-[10px] uppercase tracking-wide text-zinc-500">
        <tr className="border-b border-zinc-800">
          <th className="px-3 py-2">Guest</th>
          <th className="px-3 py-2">Faction</th>
          <th className="px-3 py-2">Location</th>
          <th className="px-3 py-2">Score</th>
          <th className="px-3 py-2"></th>
        </tr>
      </thead>
      <tbody>
        {roster.map((r) => (
          <tr key={r.id} className="border-b border-zinc-900 hover:bg-zinc-900/60">
            <td className="px-3 py-2">
              {r.name} {r.captured && <span className="ml-1 text-xs text-red-400">🔒</span>}
            </td>
            <td className="px-3 py-2 text-zinc-400">
              {r.trueFaction ?? r.faction ?? '—'}
            </td>
            <td className="px-3 py-2 text-zinc-400">{r.location ?? '—'}</td>
            <td className="px-3 py-2 text-zinc-400">{r.score}</td>
            <td className="px-3 py-2 text-right">
              {r.captured ? (
                <button onClick={() => void release(r.id)} className="rounded px-2 py-1 text-xs text-emerald-400 hover:bg-zinc-800">
                  release
                </button>
              ) : (
                <button onClick={() => void capture(r.id)} className="rounded px-2 py-1 text-xs text-amber-400 hover:bg-zinc-800">
                  capture
                </button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function BroadcastBar() {
  const broadcast = useOperate((s) => s.broadcast)
  const [scope, setScope] = useState('all')
  const [cue, setCue] = useState('')
  const send = () => {
    if (cue.trim()) {
      void broadcast(scope.trim() || 'all', cue.trim())
      setCue('')
    }
  }
  return (
    <div className="flex items-center gap-2 border-t border-zinc-800 p-2">
      <input
        className="w-28 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs outline-none focus:border-indigo-500"
        value={scope}
        onChange={(e) => setScope(e.target.value)}
        title="Scope, e.g. all, faction(Mods), location(lobby)"
      />
      <input
        className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm outline-none focus:border-indigo-500"
        placeholder="Broadcast a cue to the room…"
        value={cue}
        onChange={(e) => setCue(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && send()}
      />
      <button onClick={send} className="rounded bg-indigo-600 px-3 py-1 text-sm text-white hover:bg-indigo-500">
        Send
      </button>
    </div>
  )
}

function Feed() {
  const { messages, hideMessage } = useOperate()
  return (
    <div className="flex-1 min-h-0 overflow-auto p-2">
      {messages.length === 0 && <div className="p-2 text-sm text-zinc-600">No messages yet.</div>}
      <ul className="flex flex-col gap-1">
        {messages.map((m) => (
          <li key={m.seq} className={`group flex items-start gap-2 rounded px-2 py-1 text-sm ${m.hidden ? 'opacity-40' : ''}`}>
            <span className="w-24 shrink-0 truncate text-xs text-zinc-500">{m.channel}</span>
            <span className="shrink-0 font-medium text-zinc-300">{m.from || '·'}</span>
            <span className="flex-1 text-zinc-200">{m.text}</span>
            <button
              onClick={() => void hideMessage(m.seq, !m.hidden)}
              className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-zinc-500 opacity-0 hover:bg-zinc-800 hover:text-zinc-200 group-hover:opacity-100"
            >
              {m.hidden ? 'show' : 'hide'}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function OperateStage() {
  const projectId = useWorkspace((s) => s.projectId)
  const { init, teardown, event, connected } = useOperate()

  useEffect(() => {
    if (!projectId) return
    void init(projectId)
    return () => teardown()
  }, [projectId, init, teardown])

  if (!projectId) {
    return (
      <div className="grid h-full w-full place-items-center bg-zinc-950 p-8 text-center text-sm text-zinc-500">
        Open a server project from the launchpad to run events.
        <br />
        (Local folders can be authored, but events run on server projects.)
      </div>
    )
  }

  return (
    <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
      <div className="flex items-center justify-between border-b border-zinc-800 px-3 py-2">
        <div className="text-sm font-semibold">Run &amp; moderate</div>
        <span className="text-xs text-zinc-500">
          {event ? (connected ? '● live' : '○ reconnecting…') : 'no active event'}
        </span>
      </div>
      <div className="max-h-[45%] overflow-auto border-b border-zinc-800">
        <Roster />
      </div>
      <div className="text-[10px] uppercase tracking-wide text-zinc-600 px-3 pt-2">Feed</div>
      <Feed />
      <BroadcastBar />
    </div>
  )
}
