/**
 * Object Explorer — tree view of objects in the CollectionStore.
 *
 * Shows a hierarchical tree based on parentId relationships.
 * Clicking an object selects it (updates kernel atoms).
 * Supports creating new objects via context buttons.
 */

import { useState, useCallback, useSyncExternalStore } from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { useKernel, useSelection } from "../kernel/index.js";

// ── Tree Node ───────────────────────────────────────────────────────────────

function TreeNode({
  obj,
  children,
  depth,
  kernel,
  selectedId,
  expanded,
  onToggle,
}: {
  obj: GraphObject;
  children: GraphObject[];
  depth: number;
  kernel: ReturnType<typeof useKernel>;
  selectedId: ObjectId | null;
  expanded: Set<string>;
  onToggle: (id: string) => void;
}) {
  const isSelected = selectedId === obj.id;
  const hasChildren = children.length > 0;
  const isExpanded = expanded.has(obj.id);
  const def = kernel.registry.get(obj.type);
  const icon = def?.icon ?? "\u25CB";

  return (
    <div>
      <div
        data-testid={`explorer-node-${obj.id}`}
        onClick={() => kernel.select(obj.id)}
        style={{
          display: "flex",
          alignItems: "center",
          padding: "3px 8px",
          paddingLeft: 8 + depth * 16,
          cursor: "pointer",
          background: isSelected ? "#094771" : "transparent",
          color: isSelected ? "#fff" : "#ccc",
          fontSize: 12,
          gap: 4,
          userSelect: "none",
        }}
        onMouseEnter={(e) => {
          if (!isSelected) e.currentTarget.style.background = "#2a2d2e";
        }}
        onMouseLeave={(e) => {
          if (!isSelected) e.currentTarget.style.background = "transparent";
        }}
      >
        {hasChildren ? (
          <span
            onClick={(e) => {
              e.stopPropagation();
              onToggle(obj.id);
            }}
            style={{
              width: 16,
              textAlign: "center",
              fontSize: 10,
              color: "#888",
              cursor: "pointer",
              flexShrink: 0,
            }}
          >
            {isExpanded ? "\u25BC" : "\u25B6"}
          </span>
        ) : (
          <span style={{ width: 16, flexShrink: 0 }} />
        )}
        <span style={{ flexShrink: 0 }}>{icon}</span>
        <span
          style={{
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {obj.name}
        </span>
        <span style={{ marginLeft: "auto", fontSize: 9, color: "#666", flexShrink: 0 }}>
          {obj.type}
        </span>
      </div>
      {isExpanded &&
        children.map((child) => (
          <TreeNodeWrapper
            key={child.id}
            obj={child}
            depth={depth + 1}
            kernel={kernel}
            selectedId={selectedId}
            expanded={expanded}
            onToggle={onToggle}
          />
        ))}
    </div>
  );
}

function TreeNodeWrapper({
  obj,
  depth,
  kernel,
  selectedId,
  expanded,
  onToggle,
}: {
  obj: GraphObject;
  depth: number;
  kernel: ReturnType<typeof useKernel>;
  selectedId: ObjectId | null;
  expanded: Set<string>;
  onToggle: (id: string) => void;
}) {
  const allObjects = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects(),
  );

  const children = allObjects
    .filter((o) => o.parentId === obj.id && !o.deletedAt)
    .sort((a, b) => a.position - b.position);

  return (
    <TreeNode
      obj={obj}
      children={children}
      depth={depth}
      kernel={kernel}
      selectedId={selectedId}
      expanded={expanded}
      onToggle={onToggle}
    />
  );
}

// ── Explorer ────────────────────────────────────────────────────────────────

export function ObjectExplorer() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    // Expand all roots by default
    const roots = kernel.store
      .allObjects()
      .filter((o) => !o.parentId && !o.deletedAt);
    return new Set(roots.map((r) => r.id));
  });

  const rootObjects = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () =>
      kernel.store
        .allObjects()
        .filter((o) => !o.parentId && !o.deletedAt)
        .sort((a, b) => a.position - b.position),
  );

  const handleToggle = useCallback((id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleCreatePage = useCallback(() => {
    const count = kernel.store.listObjects({ types: ["page"] }).length;
    kernel.createObject({
      type: "page",
      name: `New Page ${count + 1}`,
      parentId: null,
      position: count,
      status: "draft",
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { title: `New Page ${count + 1}`, slug: "", layout: "single", published: false },
    });
    kernel.notifications.add({
      title: "Page created",
      kind: "success",
    });
  }, [kernel]);

  return (
    <div
      data-testid="object-explorer"
      style={{ flex: 1, overflow: "auto", paddingTop: 4 }}
    >
      {rootObjects.map((obj) => (
        <TreeNodeWrapper
          key={obj.id}
          obj={obj}
          depth={0}
          kernel={kernel}
          selectedId={selectedId}
          expanded={expanded}
          onToggle={handleToggle}
        />
      ))}

      <div style={{ padding: "8px 12px", borderTop: "1px solid #333", marginTop: 4 }}>
        <button
          data-testid="create-page-btn"
          onClick={handleCreatePage}
          style={{
            width: "100%",
            padding: "4px 8px",
            fontSize: 11,
            background: "#333",
            border: "1px solid #444",
            borderRadius: 3,
            color: "#ccc",
            cursor: "pointer",
          }}
        >
          + New Page
        </button>
      </div>
    </div>
  );
}
