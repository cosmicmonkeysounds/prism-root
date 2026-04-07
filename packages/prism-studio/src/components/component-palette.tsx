/**
 * Component Palette — lists available block types from the ObjectRegistry.
 *
 * Shows all component-category and section-category entity types that can
 * be added to the currently selected page/section. Clicking an item creates
 * a new child object under the selected parent. Items are also draggable
 * for drop-to-add in the explorer tree.
 */

import { useState, useCallback, useMemo } from "react";
import { useKernel, useSelection } from "../kernel/index.js";

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "8px",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    fontSize: 12,
  },
  header: {
    fontSize: 10,
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: 6,
    padding: "0 4px",
  },
  search: {
    width: "100%",
    padding: "4px 8px",
    fontSize: 11,
    background: "#1e1e1e",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    outline: "none",
    boxSizing: "border-box" as const,
    marginBottom: 8,
  },
  category: {
    fontSize: 9,
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#666",
    padding: "6px 4px 2px",
  },
  item: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: "5px 8px",
    borderRadius: 3,
    cursor: "grab",
    transition: "background 0.1s",
  },
  icon: {
    fontSize: 16,
    width: 20,
    textAlign: "center" as const,
    flexShrink: 0,
  },
  name: {
    flex: 1,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  addBtn: {
    background: "none",
    border: "none",
    color: "#888",
    cursor: "pointer",
    fontSize: 12,
    padding: "0 2px",
    flexShrink: 0,
    lineHeight: 1,
  },
  empty: {
    color: "#555",
    fontStyle: "italic" as const,
    padding: "12px 4px",
    textAlign: "center" as const,
  },
} as const;

interface PaletteItem {
  type: string;
  label: string;
  icon: string;
  color: string;
  category: string;
}

export function ComponentPalette() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const [search, setSearch] = useState("");

  // Get all component + section entity defs from registry
  const paletteItems = useMemo<PaletteItem[]>(() => {
    const defs = kernel.registry.allDefs();
    return defs
      .filter((d) => d.category === "component" || d.category === "section")
      .map((d) => ({
        type: d.type,
        label: d.label,
        icon: typeof d.icon === "string" ? d.icon : "\u25CB",
        color: d.color ?? "#888",
        category: d.category,
      }));
  }, [kernel.registry]);

  // Filter by search
  const filtered = useMemo(() => {
    if (!search.trim()) return paletteItems;
    const q = search.toLowerCase();
    return paletteItems.filter(
      (item) =>
        item.label.toLowerCase().includes(q) ||
        item.type.toLowerCase().includes(q),
    );
  }, [paletteItems, search]);

  // Group by category
  const grouped = useMemo(() => {
    const groups = new Map<string, PaletteItem[]>();
    for (const item of filtered) {
      const list = groups.get(item.category) ?? [];
      list.push(item);
      groups.set(item.category, list);
    }
    return groups;
  }, [filtered]);

  // Determine what the selected parent is (for add-child)
  const selectedObj = selectedId ? kernel.store.getObject(selectedId) : null;

  const handleAdd = useCallback(
    (itemType: string) => {
      if (!selectedObj) {
        kernel.notifications.add({ title: "Select a page or section first", kind: "warning" });
        return;
      }

      // Check containment
      if (!kernel.registry.canBeChildOf(itemType, selectedObj.type)) {
        // Try parent
        if (selectedObj.parentId) {
          const parent = kernel.store.getObject(selectedObj.parentId);
          if (parent && kernel.registry.canBeChildOf(itemType, parent.type)) {
            const siblings = kernel.store.listObjects({ parentId: parent.id }).length;
            const def = kernel.registry.get(itemType);
            kernel.createObject({
              type: itemType,
              name: `New ${def?.label ?? itemType}`,
              parentId: parent.id,
              position: siblings,
              status: "draft",
              tags: [],
              date: null,
              endDate: null,
              description: "",
              color: null,
              image: null,
              pinned: false,
              data: {},
            });
            kernel.notifications.add({ title: `Added ${def?.label ?? itemType}`, kind: "success" });
            return;
          }
        }
        kernel.notifications.add({ title: `Cannot add ${itemType} here`, kind: "warning" });
        return;
      }

      const siblings = kernel.store.listObjects({ parentId: selectedObj.id }).length;
      const def = kernel.registry.get(itemType);
      const newObj = kernel.createObject({
        type: itemType,
        name: `New ${def?.label ?? itemType}`,
        parentId: selectedObj.id,
        position: siblings,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });
      kernel.select(newObj.id);
      kernel.notifications.add({ title: `Added ${def?.label ?? itemType}`, kind: "success" });
    },
    [selectedObj, kernel],
  );

  const handleDragStart = useCallback(
    (e: React.DragEvent, itemType: string) => {
      e.dataTransfer.effectAllowed = "copy";
      e.dataTransfer.setData("application/x-prism-component", itemType);
    },
    [],
  );

  return (
    <div style={styles.container} data-testid="component-palette">
      <div style={styles.header}>Components</div>
      <input
        style={styles.search}
        type="text"
        placeholder="Search components..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        data-testid="palette-search"
      />

      {filtered.length === 0 ? (
        <div style={styles.empty}>No components found</div>
      ) : (
        [...grouped.entries()].map(([category, items]) => (
          <div key={category}>
            <div style={styles.category}>{category}</div>
            {items.map((item) => (
              <div
                key={item.type}
                data-testid={`palette-item-${item.type}`}
                draggable
                onDragStart={(e) => handleDragStart(e, item.type)}
                onClick={() => handleAdd(item.type)}
                style={styles.item}
                onMouseEnter={(e) => (e.currentTarget.style.background = "#2a2d2e")}
                onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
              >
                <span style={{ ...styles.icon, color: item.color }}>{item.icon}</span>
                <span style={styles.name}>{item.label}</span>
                <button
                  style={styles.addBtn}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleAdd(item.type);
                  }}
                  title={`Add ${item.label}`}
                  data-testid={`palette-add-${item.type}`}
                >
                  +
                </button>
              </div>
            ))}
          </div>
        ))
      )}
    </div>
  );
}
