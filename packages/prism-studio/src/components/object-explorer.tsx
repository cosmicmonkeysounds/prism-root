/**
 * Object Explorer — tree view of objects in the CollectionStore.
 *
 * Shows a hierarchical tree based on parentId relationships.
 * Clicking an object selects it (updates kernel atoms).
 * Supports creating new objects via context buttons.
 */

import { useState, useCallback, useMemo, useSyncExternalStore, useEffect, useRef } from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import type { SearchHit } from "@prism/core/search";
import { useKernel, useSelection } from "../kernel/index.js";

const reorderBtnStyle = {
  background: "none",
  border: "none",
  color: "#888",
  cursor: "pointer",
  fontSize: 10,
  padding: "0 2px",
  lineHeight: 1,
} as const;

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
        {isSelected && (
          <span style={{ marginLeft: "auto", display: "flex", gap: 2, flexShrink: 0 }}>
            <button
              data-testid={`move-up-${obj.id}`}
              onClick={(e) => {
                e.stopPropagation();
                kernel.updateObject(obj.id, { position: obj.position - 1 });
                // Also swap with sibling above
                const siblings = kernel.store
                  .allObjects()
                  .filter((o) => o.parentId === obj.parentId && !o.deletedAt && o.id !== obj.id)
                  .sort((a, b) => a.position - b.position);
                const above = siblings.find((s) => s.position === obj.position - 1 || (s.position >= obj.position - 1 && s.position < obj.position));
                if (above) kernel.updateObject(above.id, { position: obj.position });
              }}
              disabled={obj.position <= 0}
              style={{ ...reorderBtnStyle, opacity: obj.position > 0 ? 1 : 0.3 }}
              title="Move up"
            >
              {"\u2191"}
            </button>
            <button
              data-testid={`move-down-${obj.id}`}
              onClick={(e) => {
                e.stopPropagation();
                const siblings = kernel.store
                  .allObjects()
                  .filter((o) => o.parentId === obj.parentId && !o.deletedAt);
                kernel.updateObject(obj.id, { position: obj.position + 1 });
                const below = siblings
                  .sort((a, b) => a.position - b.position)
                  .find((s) => s.id !== obj.id && s.position >= obj.position && s.position <= obj.position + 1);
                if (below) kernel.updateObject(below.id, { position: obj.position });
              }}
              style={reorderBtnStyle}
              title="Move down"
            >
              {"\u2193"}
            </button>
          </span>
        )}
        {!isSelected && (
          <span style={{ marginLeft: "auto", fontSize: 9, color: "#666", flexShrink: 0 }}>
            {obj.type}
          </span>
        )}
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
  const cacheRef = useRef<{ children: GraphObject[]; version: number }>({ children: [], version: -1 });
  const counterRef = useRef(0);
  const version = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => { counterRef.current++; cb(); }),
    () => counterRef.current,
  );

  if (cacheRef.current.version !== version) {
    cacheRef.current = {
      children: kernel.store
        .allObjects()
        .filter((o) => o.parentId === obj.id && !o.deletedAt)
        .sort((a, b) => a.position - b.position),
      version,
    };
  }

  const children = cacheRef.current.children;

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

// ── Search Result Row ──────────────────────────────────────────────────────

function SearchResultRow({
  hit,
  kernel,
  isSelected,
}: {
  hit: SearchHit;
  kernel: ReturnType<typeof useKernel>;
  isSelected: boolean;
}) {
  const def = kernel.registry.get(hit.object.type);
  const icon = def?.icon ?? "\u25CB";

  return (
    <div
      data-testid={`search-hit-${hit.objectId}`}
      onClick={() => kernel.select(hit.objectId)}
      style={{
        display: "flex",
        alignItems: "center",
        padding: "3px 8px",
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
      <span style={{ flexShrink: 0 }}>{icon}</span>
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {hit.object.name}
      </span>
      <span style={{ marginLeft: "auto", fontSize: 9, color: "#666", flexShrink: 0 }}>
        {hit.object.type}
      </span>
    </div>
  );
}

// ── Template Gallery ──────────────────────────────────────────────────────

function TemplateGallery({
  kernel,
  onClose,
}: {
  kernel: ReturnType<typeof useKernel>;
  onClose: () => void;
}) {
  const templates = kernel.listTemplates();

  const handleInstantiate = useCallback(
    (template: { id: string; name: string; variables?: Array<{ name: string }> | undefined; }) => {
      const vars: Record<string, string> = {};
      // Use defaults for required variables
      for (const v of template.variables ?? []) {
        vars[v.name] = v.name === "title" ? `New ${template.name}` : v.name;
      }

      const result = kernel.instantiateTemplate(template.id, { variables: vars });
      if (result && result.created.length > 0) {
        const root = result.created[0] as GraphObject;
        kernel.select(root.id);
        kernel.notifications.add({
          title: `Created from "${template.name}"`,
          kind: "success",
        });
      }
      onClose();
    },
    [kernel, onClose],
  );

  return (
    <div
      data-testid="template-gallery"
      style={{
        padding: "8px",
        borderTop: "1px solid #333",
        background: "#2a2d2e",
      }}
    >
      <div style={{ fontSize: 10, fontWeight: 600, textTransform: "uppercase", color: "#888", marginBottom: 6 }}>
        Templates
        <button
          data-testid="close-templates"
          onClick={onClose}
          style={{ float: "right", background: "none", border: "none", color: "#888", cursor: "pointer", fontSize: 12 }}
        >
          {"\u2715"}
        </button>
      </div>
      {templates.length === 0 ? (
        <div style={{ fontSize: 11, color: "#666" }}>No templates registered</div>
      ) : (
        templates.map((t) => (
          <div
            key={t.id}
            data-testid={`template-${t.id}`}
            onClick={() => handleInstantiate(t)}
            style={{
              padding: "6px 8px",
              marginBottom: 4,
              fontSize: 11,
              background: "#333",
              border: "1px solid #444",
              borderRadius: 3,
              color: "#ccc",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "#3a3d3e")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "#333")}
          >
            <div style={{ fontWeight: 500 }}>{t.name}</div>
            {t.description && (
              <div style={{ fontSize: 10, color: "#888", marginTop: 2 }}>{t.description}</div>
            )}
          </div>
        ))
      )}
    </div>
  );
}

// ── Activity Feed ─────────────────────────────────────────────────────────

interface ActivityEventLike {
  id: string;
  objectId: string;
  verb: string;
  createdAt: string;
  meta?: Record<string, unknown>;
}

function ActivityFeed({
  kernel,
}: {
  kernel: ReturnType<typeof useKernel>;
}) {
  const [events, setEvents] = useState<ActivityEventLike[]>([]);

  // Collect all events across all objects on store changes
  const storeVersion = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects().length,
  );

  useEffect(() => {
    void storeVersion;
    const allJson = kernel.activity.toJSON();
    const allEvents: ActivityEventLike[] = [];
    for (const evts of Object.values(allJson)) {
      allEvents.push(...(evts as ActivityEventLike[]));
    }
    allEvents.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
    setEvents(allEvents.slice(0, 10));
  }, [kernel.activity, storeVersion]);

  if (events.length === 0) return null;

  return (
    <div
      data-testid="activity-feed"
      style={{
        padding: "8px 12px",
        borderTop: "1px solid #333",
      }}
    >
      <div style={{ fontSize: 10, fontWeight: 600, textTransform: "uppercase", color: "#666", marginBottom: 6, letterSpacing: 0.5 }}>
        Recent Activity
      </div>
      {events.map((evt) => {
        const name = (evt.meta?.["objectName"] as string) ?? evt.objectId;
        const label = `${evt.verb} "${name}"`;
        return (
          <div
            key={evt.id}
            data-testid={`activity-event-${evt.id}`}
            onClick={() => kernel.select(evt.objectId as ObjectId)}
            style={{
              fontSize: 10,
              color: "#999",
              padding: "2px 0",
              cursor: "pointer",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = "#ccc")}
            onMouseLeave={(e) => (e.currentTarget.style.color = "#999")}
          >
            {label}
          </div>
        );
      })}
    </div>
  );
}

// ── Explorer ────────────────────────────────────────────────────────────────

export function ObjectExplorer() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const [searchText, setSearchText] = useState("");
  const [showTemplates, setShowTemplates] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    // Expand all roots by default
    const roots = kernel.store
      .allObjects()
      .filter((o) => !o.parentId && !o.deletedAt);
    return new Set(roots.map((r) => r.id));
  });

  const rootCacheRef = useRef<{ roots: GraphObject[]; version: number }>({ roots: [], version: -1 });
  const rootCounterRef = useRef(0);
  const rootVersion = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => { rootCounterRef.current++; cb(); }),
    () => rootCounterRef.current,
  );
  if (rootCacheRef.current.version !== rootVersion) {
    rootCacheRef.current = {
      roots: kernel.store
        .allObjects()
        .filter((o) => !o.parentId && !o.deletedAt)
        .sort((a, b) => a.position - b.position),
      version: rootVersion,
    };
  }
  const rootObjects = rootCacheRef.current.roots;

  // Search results — re-computed when searchText or store changes
  const storeVersion = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects().length,
  );

  const searchResults = useMemo(() => {
    if (!searchText.trim()) return null;
    // storeVersion is referenced to trigger re-computation on store changes
    void storeVersion;
    return kernel.search.search({ query: searchText });
  }, [searchText, storeVersion, kernel.search]);

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

  const isSearching = searchResults !== null;

  return (
    <div
      data-testid="object-explorer"
      style={{ flex: 1, overflow: "auto", paddingTop: 4 }}
    >
      <div style={{ padding: "4px 8px" }}>
        <input
          data-testid="explorer-search"
          type="text"
          placeholder="Search objects..."
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          style={{
            width: "100%",
            padding: "4px 8px",
            fontSize: 11,
            background: "#1e1e1e",
            border: "1px solid #444",
            borderRadius: 3,
            color: "#ccc",
            outline: "none",
            boxSizing: "border-box",
          }}
        />
      </div>

      {isSearching ? (
        <div data-testid="search-results">
          {searchResults.hits.length === 0 ? (
            <div
              style={{
                padding: "8px 12px",
                fontSize: 11,
                color: "#666",
              }}
            >
              No results found
            </div>
          ) : (
            searchResults.hits.map((hit) => (
              <SearchResultRow
                key={hit.objectId}
                hit={hit}
                kernel={kernel}
                isSelected={selectedId === hit.objectId}
              />
            ))
          )}
        </div>
      ) : (
        rootObjects.map((obj) => (
          <TreeNodeWrapper
            key={obj.id}
            obj={obj}
            depth={0}
            kernel={kernel}
            selectedId={selectedId}
            expanded={expanded}
            onToggle={handleToggle}
          />
        ))
      )}

      <ActivityFeed kernel={kernel} />

      {showTemplates && (
        <TemplateGallery kernel={kernel} onClose={() => setShowTemplates(false)} />
      )}

      <div style={{ padding: "8px 12px", borderTop: "1px solid #333", marginTop: 4, display: "flex", gap: 4 }}>
        <button
          data-testid="create-page-btn"
          onClick={handleCreatePage}
          style={{
            flex: 1,
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
        <button
          data-testid="open-templates-btn"
          onClick={() => setShowTemplates(!showTemplates)}
          style={{
            flex: 1,
            padding: "4px 8px",
            fontSize: 11,
            background: showTemplates ? "#094771" : "#333",
            border: "1px solid #444",
            borderRadius: 3,
            color: "#ccc",
            cursor: "pointer",
          }}
        >
          Templates
        </button>
      </div>
    </div>
  );
}
