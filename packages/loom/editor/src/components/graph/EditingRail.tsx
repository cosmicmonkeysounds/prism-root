// Writing mode's left rail: Files ⇄ Story tabs.
//
// The file explorer and the project-wide Story Bin live side by side,
// so the merged Writing mode never strands you without either.

import { useState } from 'react'
import clsx from 'clsx'
import { Sidebar } from '@/components/files/Sidebar'
import { StoryBin } from './StoryBin'

type Tab = 'story' | 'files'

export function EditingRail() {
  const [tab, setTab] = useState<Tab>('story')
  return (
    <div className="h-full flex flex-col bg-zinc-950">
      <div className="flex shrink-0 border-b border-white/10">
        {(
          [
            ['story', 'Story'],
            ['files', 'Files'],
          ] as Array<[Tab, string]>
        ).map(([id, label]) => (
          <button
            key={id}
            className={clsx(
              'flex-1 px-2 py-1.5 text-[11px] font-medium',
              tab === id
                ? 'text-zinc-100 border-b-2 border-sky-400 -mb-px'
                : 'text-zinc-500 hover:text-zinc-300',
            )}
            onClick={() => setTab(id)}
            data-testid={`editing-rail-${id}`}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="flex-1 min-h-0">{tab === 'story' ? <StoryBin /> : <Sidebar />}</div>
    </div>
  )
}
