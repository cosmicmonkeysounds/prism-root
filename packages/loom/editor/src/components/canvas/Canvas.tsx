import { useCallback } from 'react'
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  type NodeChange,
  type EdgeChange,
  type Connection,
} from '@xyflow/react'
import { useWorkspace } from '@/store/workspace'

export function Canvas() {
  const nodes = useWorkspace((s) => s.nodes)
  const edges = useWorkspace((s) => s.edges)
  const setNodes = useWorkspace((s) => s.setNodes)
  const setEdges = useWorkspace((s) => s.setEdges)
  const setActive = useWorkspace((s) => s.setActive)
  const openFiles = useWorkspace((s) => s.openFiles)

  const onNodesChange = useCallback(
    (changes: NodeChange[]) => setNodes((nds) => applyNodeChanges(changes, nds)),
    [setNodes],
  )
  const onEdgesChange = useCallback(
    (changes: EdgeChange[]) => setEdges((eds) => applyEdgeChanges(changes, eds)),
    [setEdges],
  )
  const onConnect = useCallback(
    (c: Connection) => setEdges((eds) => addEdge(c, eds)),
    [setEdges],
  )

  if (nodes.length === 0) {
    return (
      <div className="h-full grid place-items-center text-zinc-600 text-xs">
        Open a file to populate the canvas.
      </div>
    )
  }

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onConnect={onConnect}
      onNodeClick={(_, n) => {
        if (openFiles[n.id]) setActive(n.id)
      }}
      fitView
      colorMode="dark"
      proOptions={{ hideAttribution: true }}
    >
      <Background gap={16} size={1} color="#27272a" />
      <Controls className="!bg-zinc-900 !border-white/10" />
      <MiniMap pannable className="!bg-zinc-900" maskColor="rgba(0,0,0,0.5)" />
    </ReactFlow>
  )
}
