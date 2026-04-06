/**
 * ViewDef — view mode definitions and capability registry.
 *
 * Declares the 7 standard view modes and their feature capabilities.
 * The registry is queried by UI to determine what controls to show
 * (sort, filter, group, columns, inline edit, bulk select, etc.).
 *
 * Ported from legacy @core/ui/view with Helm→Prism rename.
 */

// ── View Modes ───────────────────────────────────────────────────────────────

export type ViewMode =
  | "list"
  | "kanban"
  | "grid"
  | "table"
  | "timeline"
  | "calendar"
  | "graph";

// ── View Definition ──────────────────────────────────────────────────────────

/** Capability descriptor for a view mode. */
export interface ViewDef {
  mode: ViewMode;
  label: string;
  description: string;

  // Feature capabilities
  supportsSort: boolean;
  supportsFilter: boolean;
  supportsGrouping: boolean;
  supportsColumns: boolean;
  supportsInlineEdit: boolean;
  supportsBulkSelect: boolean;
  supportsHierarchy: boolean;

  // Data requirements
  requiresDate: boolean;
  requiresStatus: boolean;
}

// ── Built-in View Definitions ────────────────────────────────────────────────

const LIST_DEF: ViewDef = {
  mode: "list",
  label: "List",
  description: "Sortable, filterable list with optional hierarchy",
  supportsSort: true,
  supportsFilter: true,
  supportsGrouping: true,
  supportsColumns: false,
  supportsInlineEdit: true,
  supportsBulkSelect: true,
  supportsHierarchy: true,
  requiresDate: false,
  requiresStatus: false,
};

const KANBAN_DEF: ViewDef = {
  mode: "kanban",
  label: "Kanban",
  description: "Drag-and-drop columns grouped by status or field",
  supportsSort: true,
  supportsFilter: true,
  supportsGrouping: true,
  supportsColumns: false,
  supportsInlineEdit: true,
  supportsBulkSelect: true,
  supportsHierarchy: false,
  requiresDate: false,
  requiresStatus: true,
};

const GRID_DEF: ViewDef = {
  mode: "grid",
  label: "Grid",
  description: "Card-based grid layout with image thumbnails",
  supportsSort: true,
  supportsFilter: true,
  supportsGrouping: false,
  supportsColumns: false,
  supportsInlineEdit: false,
  supportsBulkSelect: true,
  supportsHierarchy: false,
  requiresDate: false,
  requiresStatus: false,
};

const TABLE_DEF: ViewDef = {
  mode: "table",
  label: "Table",
  description: "Spreadsheet-style rows with configurable columns",
  supportsSort: true,
  supportsFilter: true,
  supportsGrouping: true,
  supportsColumns: true,
  supportsInlineEdit: true,
  supportsBulkSelect: true,
  supportsHierarchy: false,
  requiresDate: false,
  requiresStatus: false,
};

const TIMELINE_DEF: ViewDef = {
  mode: "timeline",
  label: "Timeline",
  description: "Horizontal timeline with date-based positioning",
  supportsSort: false,
  supportsFilter: true,
  supportsGrouping: true,
  supportsColumns: false,
  supportsInlineEdit: false,
  supportsBulkSelect: false,
  supportsHierarchy: false,
  requiresDate: true,
  requiresStatus: false,
};

const CALENDAR_DEF: ViewDef = {
  mode: "calendar",
  label: "Calendar",
  description: "Month/week/day calendar with date placement",
  supportsSort: false,
  supportsFilter: true,
  supportsGrouping: false,
  supportsColumns: false,
  supportsInlineEdit: false,
  supportsBulkSelect: false,
  supportsHierarchy: false,
  requiresDate: true,
  requiresStatus: false,
};

const GRAPH_DEF: ViewDef = {
  mode: "graph",
  label: "Graph",
  description: "Node-edge graph visualization of object relationships",
  supportsSort: false,
  supportsFilter: true,
  supportsGrouping: false,
  supportsColumns: false,
  supportsInlineEdit: false,
  supportsBulkSelect: false,
  supportsHierarchy: false,
  requiresDate: false,
  requiresStatus: false,
};

// ── View Registry ────────────────────────────────────────────────────────────

export interface ViewRegistry {
  /** Get the definition for a view mode. */
  get(mode: ViewMode): ViewDef | undefined;
  /** Get all registered view definitions. */
  all(): ViewDef[];
  /** Register a custom view mode definition. */
  register(def: ViewDef): void;
  /** Check if a mode supports a specific capability. */
  supports(mode: ViewMode, capability: keyof Omit<ViewDef, "mode" | "label" | "description">): boolean;
  /** Get all modes that support a capability. */
  modesWithCapability(capability: keyof Omit<ViewDef, "mode" | "label" | "description">): ViewMode[];
}

export function createViewRegistry(): ViewRegistry {
  const defs = new Map<ViewMode, ViewDef>();

  // Register built-ins
  for (const def of [LIST_DEF, KANBAN_DEF, GRID_DEF, TABLE_DEF, TIMELINE_DEF, CALENDAR_DEF, GRAPH_DEF]) {
    defs.set(def.mode, def);
  }

  return {
    get(mode: ViewMode): ViewDef | undefined {
      return defs.get(mode);
    },

    all(): ViewDef[] {
      return [...defs.values()];
    },

    register(def: ViewDef): void {
      defs.set(def.mode, def);
    },

    supports(mode: ViewMode, capability: keyof Omit<ViewDef, "mode" | "label" | "description">): boolean {
      const def = defs.get(mode);
      if (!def) return false;
      return Boolean(def[capability]);
    },

    modesWithCapability(capability: keyof Omit<ViewDef, "mode" | "label" | "description">): ViewMode[] {
      return [...defs.values()]
        .filter((def) => Boolean(def[capability]))
        .map((def) => def.mode);
    },
  };
}
