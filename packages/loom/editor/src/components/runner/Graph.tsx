// Phase 2 stub. The full entity Graph lands in Phase 3 (force-directed
// xyflow over the bundle's characters / cohorts / locations / beats),
// once the relay protocol carries the entity schema the simulator's
// `entities` command emits. Today the relay only ships the live ledger
// + world, so the graph would have nothing to draw.

export function GraphPanel() {
  return (
    <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
      <div>
        <div>Graph</div>
        <div className="text-zinc-600 mt-1 max-w-xs">
          Entity graph (characters · locations · cohorts · beats) lands
          with Phase 3 of the redesign — once the relay grows an
          entity-schema envelope.
        </div>
      </div>
    </div>
  );
}
