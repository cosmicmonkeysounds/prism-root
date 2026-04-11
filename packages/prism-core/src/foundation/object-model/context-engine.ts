/**
 * ContextEngine — context-aware suggestion engine.
 *
 * Given what the user is looking at / doing, the engine answers:
 *   - What edge types can I create from this object?
 *   - What edge types are valid between these two objects?
 *   - What child types can I create under this object?
 *   - What should the right-click menu contain?
 *   - What edge type should fire when the user types [[...]]?
 *
 * The engine is UI-agnostic. It returns plain data structures that any
 * UI layer (React, Vue, native menu, etc.) can consume.
 *
 * All answers are derived entirely from the ObjectRegistry — nothing is
 * hardcoded here. Register different types / edge types / presets and
 * the context menu / autocomplete changes automatically.
 */

import type { EdgeTypeDef, EntityDef } from "./types.js";
import type { ObjectRegistry } from "./registry.js";

// ── Output types ──────────────────────────────────────────────────────────────

export interface EdgeOption {
  relation: string;
  label: string;
  description?: string | undefined;
  behavior: EdgeTypeDef["behavior"];
  isInline: boolean;
  def: EdgeTypeDef;
}

export interface ChildOption {
  type: string;
  label: string;
  pluralLabel: string;
  def: EntityDef;
}

// ── Context menu types ────────────────────────────────────────────────────────

export type ContextMenuAction =
  | "create-child"
  | "create-edge"
  | "delete"
  | "duplicate"
  | "move";

export interface ContextMenuItem {
  id: string;
  label: string;
  action: ContextMenuAction;
  payload: Record<string, unknown>;
  shortcut?: string;
}

export interface ContextMenuSection {
  id: string;
  title: string;
  items: ContextMenuItem[];
}

// ── Autocomplete result ───────────────────────────────────────────────────────

export interface AutocompleteSuggestion {
  edgeTypes: EdgeTypeDef[];
  defaultRelation: string | null;
}

// ── ContextEngine ─────────────────────────────────────────────────────────────

export class ContextEngine<TIcon = unknown> {
  constructor(private readonly registry: ObjectRegistry<TIcon>) {}

  getEdgeOptions(sourceType: string, targetType?: string): EdgeOption[] {
    const defs = targetType
      ? this.registry.getEdgesBetween(sourceType, targetType)
      : this.registry.getEdgesFrom(sourceType);

    return defs.map((def) => ({
      relation: def.relation,
      label: def.label,
      description: def.description,
      behavior: def.behavior,
      isInline: def.suggestInline ?? false,
      def,
    }));
  }

  getInlineLinkTypes(sourceType: string): EdgeTypeDef[] {
    return this.registry
      .getEdgesFrom(sourceType)
      .filter((def) => def.suggestInline);
  }

  getInlineEdgeTypes(): EdgeTypeDef[] {
    return this.registry.allEdgeDefs().filter((def) => def.suggestInline);
  }

  getAutocompleteSuggestions(sourceType: string): AutocompleteSuggestion {
    const fromSource = this.getInlineLinkTypes(sourceType);
    const all = this.getInlineEdgeTypes();
    const edgeTypes = fromSource.length > 0 ? fromSource : all;
    return {
      edgeTypes,
      defaultRelation: edgeTypes[0]?.relation ?? null,
    };
  }

  getChildOptions(parentType: string): ChildOption[] {
    return this.registry.getAllowedChildTypes(parentType).map((type) => {
      const def = this.registry.get(type) as EntityDef;
      return {
        type,
        label: def.label,
        pluralLabel: def.pluralLabel ?? def.label,
        def,
      };
    });
  }

  getContextMenu(
    objectType: string,
    targetType?: string,
  ): ContextMenuSection[] {
    const sections: ContextMenuSection[] = [];

    const childOpts = this.getChildOptions(objectType);
    if (childOpts.length > 0) {
      sections.push({
        id: "create",
        title: "Create",
        items: childOpts.map((opt) => ({
          id: `create-child:${opt.type}`,
          label: `New ${opt.label}`,
          action: "create-child",
          payload: { childType: opt.type },
        })),
      });
    }

    const edgeOpts = this.getEdgeOptions(objectType, targetType).filter(
      (o) => !o.isInline,
    );

    if (edgeOpts.length > 0) {
      sections.push({
        id: "connect",
        title: "Connect",
        items: edgeOpts.map((opt) => ({
          id: `create-edge:${opt.relation}`,
          label: `${opt.label}…`,
          action: "create-edge",
          payload: { relation: opt.relation },
        })),
      });
    }

    sections.push({
      id: "object",
      title: "Object",
      items: [
        {
          id: "duplicate",
          label: "Duplicate",
          action: "duplicate",
          payload: {},
        },
        { id: "delete", label: "Delete", action: "delete", payload: {} },
      ],
    });

    return sections;
  }
}
