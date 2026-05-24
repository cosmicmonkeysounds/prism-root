import { useWorkspace } from '@/store/workspace'

export function Breadcrumbs() {
  const activePath = useWorkspace((s) => s.activePath)
  if (!activePath) return null
  const parts = activePath.split('/')
  return (
    <div className="px-3 py-1 text-[11px] text-zinc-500 border-b border-white/5 bg-zinc-950/50 flex items-center gap-1 overflow-x-auto whitespace-nowrap">
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          {i > 0 && <span className="text-zinc-700">/</span>}
          <span className={i === parts.length - 1 ? 'text-zinc-300' : ''}>{part}</span>
        </span>
      ))}
    </div>
  )
}
