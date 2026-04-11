/**
 * Zustand store managing the spatial node graph backed by Loro CRDT.
 *
 * Nodes have x/y positions, type, and data.
 * Edges have source/target, wire type (hard ref / weak ref), and optional label.
 * All state lives in Loro — this store is a reactive projection.
 */

import { createStore } from "zustand/vanilla";
import { LoroDoc, LoroMap, LoroList } from "loro-crdt";

/** Wire type for edges in the object graph. */
export type WireType = "hard" | "weak";

/** A node in the spatial graph. */
export type GraphNode = {
  id: string;
  type: string;
  x: number;
  y: number;
  width: number;
  height: number;
  data: Record<string, unknown>;
};

/** An edge in the spatial graph. */
export type GraphEdge = {
  id: string;
  source: string;
  target: string;
  wireType: WireType;
  label?: string;
};

export type GraphStoreState = {
  nodes: GraphNode[];
  edges: GraphEdge[];
};

export type GraphStoreActions = {
  /** Add a node to the graph. */
  addNode: (node: GraphNode) => void;
  /** Update a node's position. */
  moveNode: (id: string, x: number, y: number) => void;
  /** Update a node's data. */
  updateNodeData: (id: string, data: Record<string, unknown>) => void;
  /** Remove a node and its connected edges. */
  removeNode: (id: string) => void;
  /** Add an edge between two nodes. */
  addEdge: (edge: GraphEdge) => void;
  /** Remove an edge. */
  removeEdge: (id: string) => void;
  /** Sync state from the Loro document. */
  syncFromLoro: () => void;
};

export type GraphStore = GraphStoreState & GraphStoreActions;

const NODES_KEY = "graph_nodes";
const EDGES_KEY = "graph_edges";

/**
 * Create a graph store backed by a Loro document.
 * All mutations go through Loro; the store reacts to Loro changes.
 */
export function createGraphStore(doc: LoroDoc) {
  const root = doc.getMap("root");

  function getNodesList(): LoroList {
    let list = root.get(NODES_KEY);
    if (!list || !(list instanceof LoroList)) {
      root.setContainer(NODES_KEY, new LoroList());
      doc.commit();
      list = root.get(NODES_KEY);
    }
    return list as LoroList;
  }

  function getEdgesList(): LoroList {
    let list = root.get(EDGES_KEY);
    if (!list || !(list instanceof LoroList)) {
      root.setContainer(EDGES_KEY, new LoroList());
      doc.commit();
      list = root.get(EDGES_KEY);
    }
    return list as LoroList;
  }

  function readNodes(): GraphNode[] {
    try {
      const list = getNodesList();
      const json = list.toJSON() as unknown[];
      return json.map((item) => item as GraphNode);
    } catch {
      return [];
    }
  }

  function readEdges(): GraphEdge[] {
    try {
      const list = getEdgesList();
      const json = list.toJSON() as unknown[];
      return json.map((item) => item as GraphEdge);
    } catch {
      return [];
    }
  }

  function findNodeIndex(id: string): number {
    const nodes = readNodes();
    return nodes.findIndex((n) => n.id === id);
  }

  function findEdgeIndex(id: string): number {
    const edges = readEdges();
    return edges.findIndex((e) => e.id === id);
  }

  const store = createStore<GraphStore>((set) => ({
    nodes: [],
    edges: [],

    addNode(node: GraphNode) {
      const list = getNodesList();
      const map = new LoroMap();
      map.set("id", node.id);
      map.set("type", node.type);
      map.set("x", node.x);
      map.set("y", node.y);
      map.set("width", node.width);
      map.set("height", node.height);
      map.set("data", JSON.stringify(node.data));
      list.pushContainer(map);
      doc.commit();
      set({ nodes: readNodes() });
    },

    moveNode(id: string, x: number, y: number) {
      const idx = findNodeIndex(id);
      if (idx === -1) return;
      const list = getNodesList();
      const nodeContainer = list.get(idx);
      if (nodeContainer && nodeContainer instanceof LoroMap) {
        nodeContainer.set("x", x);
        nodeContainer.set("y", y);
        doc.commit();
        set({ nodes: readNodes() });
      }
    },

    updateNodeData(id: string, data: Record<string, unknown>) {
      const idx = findNodeIndex(id);
      if (idx === -1) return;
      const list = getNodesList();
      const nodeContainer = list.get(idx);
      if (nodeContainer && nodeContainer instanceof LoroMap) {
        nodeContainer.set("data", JSON.stringify(data));
        doc.commit();
        set({ nodes: readNodes() });
      }
    },

    removeNode(id: string) {
      const idx = findNodeIndex(id);
      if (idx === -1) return;
      const list = getNodesList();
      list.delete(idx, 1);

      // Remove connected edges
      const edges = readEdges();
      const edgesList = getEdgesList();
      for (let i = edges.length - 1; i >= 0; i--) {
        const edge = edges[i];
        if (edge && (edge.source === id || edge.target === id)) {
          edgesList.delete(i, 1);
        }
      }

      doc.commit();
      set({ nodes: readNodes(), edges: readEdges() });
    },

    addEdge(edge: GraphEdge) {
      const list = getEdgesList();
      const map = new LoroMap();
      map.set("id", edge.id);
      map.set("source", edge.source);
      map.set("target", edge.target);
      map.set("wireType", edge.wireType);
      if (edge.label) {
        map.set("label", edge.label);
      }
      list.pushContainer(map);
      doc.commit();
      set({ edges: readEdges() });
    },

    removeEdge(id: string) {
      const idx = findEdgeIndex(id);
      if (idx === -1) return;
      const list = getEdgesList();
      list.delete(idx, 1);
      doc.commit();
      set({ edges: readEdges() });
    },

    syncFromLoro() {
      set({ nodes: readNodes(), edges: readEdges() });
    },
  }));

  // Subscribe to Loro changes for external updates (peer sync)
  doc.subscribe(() => {
    store.getState().syncFromLoro();
  });

  // Initial hydration
  store.getState().syncFromLoro();

  return store;
}
