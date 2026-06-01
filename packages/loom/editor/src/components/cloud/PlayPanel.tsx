// Phase 7 — co-playing UI. Originally a monolithic transcript +
// choices pane; Phase 2 of the IDE redesign split those into the
// dock's standalone Transcript / Choices panels.
//
// This composite version stays available under the `play` panel id
// so saved presets and the ⌘⇧P shortcut still land on a working
// "all-in-one runner" view, but it now just stacks the same
// components — no duplicated transcript-rendering or event-tag
// switch logic.

import { TranscriptPanel } from "@/components/runner/Transcript";
import { ChoicesPanel } from "@/components/runner/Choices";

export function PlayPanel() {
  return (
    <div className="h-full w-full flex flex-col bg-zinc-950">
      <div className="flex-1 min-h-0">
        <TranscriptPanel />
      </div>
      <div className="border-t border-white/10 max-h-[40%] overflow-auto">
        <ChoicesPanel />
      </div>
    </div>
  );
}
