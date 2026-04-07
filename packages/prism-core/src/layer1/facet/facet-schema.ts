/**
 * FacetSchema — layout-driven view definitions for Prism objects.
 *
 * Inspired by FileMaker Pro's Layout model: a FacetDefinition describes
 * how to project an entity type into a visual form, list, table, report,
 * or card. Layout parts (header, body, footer, summaries) contain field
 * slots and portal slots. Portals display related records inline.
 *
 * Use the builder API for ergonomic construction:
 *   const facet = facetDefinitionBuilder('contact-form', 'contact', 'form')
 *     .name('Contact Form')
 *     .addPart({ kind: 'header' })
 *     .addField({ fieldPath: 'name', part: 'header', order: 0 })
 *     .addPortal({ relationshipId: 'invoiced-to', displayFields: ['amount', 'date'], part: 'body', order: 1 })
 *     .build();
 */

// ── Layout types ─────────────────────────────────────────────────────────────

export type FacetLayout = 'form' | 'list' | 'table' | 'report' | 'card';

export type LayoutPartKind =
  | 'title-header'
  | 'header'
  | 'body'
  | 'footer'
  | 'leading-summary'
  | 'trailing-summary'
  | 'leading-grand-summary'
  | 'trailing-grand-summary';

export interface LayoutPart {
  kind: LayoutPartKind;
  /** Height in pixels. Auto if omitted. */
  height?: number;
  visible?: boolean;
  backgroundColor?: string;
}

// ── Slot types ───────────────────────────────────────────────────────────────

export interface FieldSlot {
  /** Dot-path into GraphObject.data (e.g. "name", "address.city"). */
  fieldPath: string;
  /** Override field label. */
  label?: string;
  labelPosition?: 'top' | 'left' | 'hidden';
  /** Width in px or percentage string (e.g. "50%"). */
  width?: number | string;
  /** Grid column span (1-12). */
  span?: number;
  readOnly?: boolean;
  placeholder?: string;
  /** Which layout part this field belongs to. */
  part: LayoutPartKind;
  /** Sort order within the part. */
  order: number;
}

export interface PortalSlot {
  /** EdgeTypeDef id for the relationship. */
  relationshipId: string;
  /** Fields to show from related objects. */
  displayFields: string[];
  /** Which layout part this portal belongs to. */
  part: LayoutPartKind;
  /** Sort order within the part. */
  order: number;
  /** Visible rows (scrollable if more). */
  rows?: number;
  /** Can create related records inline. */
  allowCreation?: boolean;
  sortField?: string;
  sortDirection?: 'asc' | 'desc';
}

export type FacetSlot =
  | { kind: 'field'; slot: FieldSlot }
  | { kind: 'portal'; slot: PortalSlot };

// ── Summary fields ───────────────────────────────────────────────────────────

export interface SummaryField {
  fieldPath: string;
  operation: 'count' | 'sum' | 'average' | 'min' | 'max' | 'list';
  label?: string;
}

// ── Facet definition ─────────────────────────────────────────────────────────

export interface FacetDefinition {
  id: string;
  name: string;
  description?: string;
  layout: FacetLayout;
  /** EntityDef type this facet projects. */
  objectType: string;
  parts: LayoutPart[];
  slots: FacetSlot[];
  summaryFields?: SummaryField[];
  /** Report-specific: group records by this field. */
  groupByField?: string;
  sortFields?: Array<{ field: string; direction: 'asc' | 'desc' }>;
  /** Automation ID to fire when a record loads. */
  onRecordLoad?: string;
  /** Automation ID to fire when a record commits. */
  onRecordCommit?: string;
  /** Automation ID to fire when entering this layout. */
  onLayoutEnter?: string;
  /** Automation ID to fire when exiting this layout. */
  onLayoutExit?: string;
}

// ── Factory ──────────────────────────────────────────────────────────────────

export function createFacetDefinition(
  id: string,
  objectType: string,
  layout: FacetLayout,
): FacetDefinition {
  return {
    id,
    name: id,
    layout,
    objectType,
    parts: [],
    slots: [],
  };
}

// ── Builder ──────────────────────────────────────────────────────────────────

export class FacetDefinitionBuilder {
  private readonly _def: FacetDefinition;

  constructor(id: string, objectType: string, layout: FacetLayout) {
    this._def = createFacetDefinition(id, objectType, layout);
  }

  name(n: string): this {
    this._def.name = n;
    return this;
  }

  description(d: string): this {
    this._def.description = d;
    return this;
  }

  addPart(part: LayoutPart): this {
    this._def.parts.push(part);
    return this;
  }

  addField(slot: FieldSlot): this {
    this._def.slots.push({ kind: 'field', slot });
    return this;
  }

  addPortal(slot: PortalSlot): this {
    this._def.slots.push({ kind: 'portal', slot });
    return this;
  }

  addSummary(field: SummaryField): this {
    if (!this._def.summaryFields) {
      this._def.summaryFields = [];
    }
    this._def.summaryFields.push(field);
    return this;
  }

  groupBy(field: string): this {
    this._def.groupByField = field;
    return this;
  }

  sortBy(field: string, direction: 'asc' | 'desc'): this {
    if (!this._def.sortFields) {
      this._def.sortFields = [];
    }
    this._def.sortFields.push({ field, direction });
    return this;
  }

  onRecordLoad(automationId: string): this {
    this._def.onRecordLoad = automationId;
    return this;
  }

  onRecordCommit(automationId: string): this {
    this._def.onRecordCommit = automationId;
    return this;
  }

  build(): FacetDefinition {
    return { ...this._def, parts: [...this._def.parts], slots: [...this._def.slots] };
  }
}

export function facetDefinitionBuilder(
  id: string,
  objectType: string,
  layout: FacetLayout,
): FacetDefinitionBuilder {
  return new FacetDefinitionBuilder(id, objectType, layout);
}
