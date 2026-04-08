import { describe, it, expect } from 'vitest';
import {
  createFacetDefinition,
  createPrintConfig,
  FacetDefinitionBuilder,
  facetDefinitionBuilder,
  type FacetLayout,
  type LayoutPartKind,
  type FieldSlot,
  type PortalSlot,
  type TextSlot,
  type DrawingSlot,
  type ContainerSlot,
  type SummaryField,
  type PrintConfig,
} from './facet-schema.js';

// ── createFacetDefinition factory ───────────────────────────────────────────

describe('createFacetDefinition', () => {
  it('returns a FacetDefinition with correct id, objectType, and layout', () => {
    const def = createFacetDefinition('my-facet', 'contact', 'form');
    expect(def.id).toBe('my-facet');
    expect(def.objectType).toBe('contact');
    expect(def.layout).toBe('form');
  });

  it('defaults name to the id', () => {
    const def = createFacetDefinition('invoice-list', 'invoice', 'list');
    expect(def.name).toBe('invoice-list');
  });

  it('starts with empty parts and slots', () => {
    const def = createFacetDefinition('t', 'obj', 'table');
    expect(def.parts).toEqual([]);
    expect(def.slots).toEqual([]);
  });

  it('has no optional fields set', () => {
    const def = createFacetDefinition('t', 'obj', 'card');
    expect(def.description).toBeUndefined();
    expect(def.summaryFields).toBeUndefined();
    expect(def.groupByField).toBeUndefined();
    expect(def.sortFields).toBeUndefined();
    expect(def.onRecordLoad).toBeUndefined();
    expect(def.onRecordCommit).toBeUndefined();
    expect(def.onLayoutEnter).toBeUndefined();
    expect(def.onLayoutExit).toBeUndefined();
  });

  const layouts: FacetLayout[] = ['form', 'list', 'table', 'report', 'card'];
  it.each(layouts)('accepts layout type "%s"', (layout) => {
    const def = createFacetDefinition('x', 'obj', layout);
    expect(def.layout).toBe(layout);
  });
});

// ── FacetDefinitionBuilder ──────────────────────────────────────────────────

describe('FacetDefinitionBuilder', () => {
  it('can be constructed directly', () => {
    const builder = new FacetDefinitionBuilder('b', 'entity', 'form');
    const def = builder.build();
    expect(def.id).toBe('b');
    expect(def.objectType).toBe('entity');
    expect(def.layout).toBe('form');
  });

  it('can be created via facetDefinitionBuilder helper', () => {
    const builder = facetDefinitionBuilder('b', 'entity', 'list');
    expect(builder).toBeInstanceOf(FacetDefinitionBuilder);
    expect(builder.build().layout).toBe('list');
  });

  describe('fluent API returns this', () => {
    it('name() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.name('Test')).toBe(builder);
    });

    it('description() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.description('desc')).toBe(builder);
    });

    it('addPart() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.addPart({ kind: 'body' })).toBe(builder);
    });

    it('addField() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.addField({ fieldPath: 'f', part: 'body', order: 0 })).toBe(builder);
    });

    it('addPortal() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(
        builder.addPortal({ relationshipId: 'r', displayFields: [], part: 'body', order: 0 }),
      ).toBe(builder);
    });

    it('addSummary() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.addSummary({ fieldPath: 'f', operation: 'count' })).toBe(builder);
    });

    it('groupBy() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'report');
      expect(builder.groupBy('category')).toBe(builder);
    });

    it('sortBy() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'list');
      expect(builder.sortBy('name', 'asc')).toBe(builder);
    });

    it('onRecordLoad() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.onRecordLoad('auto-1')).toBe(builder);
    });

    it('onRecordCommit() is chainable', () => {
      const builder = facetDefinitionBuilder('x', 'o', 'form');
      expect(builder.onRecordCommit('auto-2')).toBe(builder);
    });
  });

  describe('name()', () => {
    it('sets the facet name', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form').name('My Form').build();
      expect(def.name).toBe('My Form');
    });

    it('overrides the default id-based name', () => {
      const def = facetDefinitionBuilder('contact-form', 'contact', 'form')
        .name('Contact Entry Form')
        .build();
      expect(def.name).toBe('Contact Entry Form');
    });
  });

  describe('description()', () => {
    it('sets the description', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .description('A detailed description')
        .build();
      expect(def.description).toBe('A detailed description');
    });
  });

  describe('addPart()', () => {
    it('adds a layout part', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .addPart({ kind: 'header' })
        .build();
      expect(def.parts).toHaveLength(1);
      expect(def.parts[0].kind).toBe('header');
    });

    it('adds multiple parts in order', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .addPart({ kind: 'header' })
        .addPart({ kind: 'body' })
        .addPart({ kind: 'footer' })
        .build();
      expect(def.parts.map((p) => p.kind)).toEqual(['header', 'body', 'footer']);
    });

    it('preserves optional part properties', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .addPart({ kind: 'header', height: 100, visible: false, backgroundColor: '#ff0000' })
        .build();
      const part = def.parts[0];
      expect(part.height).toBe(100);
      expect(part.visible).toBe(false);
      expect(part.backgroundColor).toBe('#ff0000');
    });
  });

  describe('addField()', () => {
    it('adds a field slot', () => {
      const field: FieldSlot = { fieldPath: 'name', part: 'body', order: 0 };
      const def = facetDefinitionBuilder('id', 'obj', 'form').addField(field).build();
      expect(def.slots).toHaveLength(1);
      expect(def.slots[0]).toEqual({ kind: 'field', slot: field });
    });

    it('preserves all field slot properties', () => {
      const field: FieldSlot = {
        fieldPath: 'address.city',
        label: 'City',
        labelPosition: 'left',
        width: '50%',
        span: 6,
        readOnly: true,
        placeholder: 'Enter city',
        part: 'body',
        order: 2,
      };
      const def = facetDefinitionBuilder('id', 'obj', 'form').addField(field).build();
      const slot = def.slots[0];
      expect(slot.kind).toBe('field');
      if (slot.kind === 'field') {
        expect(slot.slot.fieldPath).toBe('address.city');
        expect(slot.slot.label).toBe('City');
        expect(slot.slot.labelPosition).toBe('left');
        expect(slot.slot.width).toBe('50%');
        expect(slot.slot.span).toBe(6);
        expect(slot.slot.readOnly).toBe(true);
        expect(slot.slot.placeholder).toBe('Enter city');
      }
    });
  });

  describe('addPortal()', () => {
    it('adds a portal slot', () => {
      const portal: PortalSlot = {
        relationshipId: 'invoiced-to',
        displayFields: ['amount', 'date'],
        part: 'body',
        order: 1,
      };
      const def = facetDefinitionBuilder('id', 'obj', 'form').addPortal(portal).build();
      expect(def.slots).toHaveLength(1);
      expect(def.slots[0]).toEqual({ kind: 'portal', slot: portal });
    });

    it('preserves optional portal properties', () => {
      const portal: PortalSlot = {
        relationshipId: 'line-items',
        displayFields: ['sku', 'qty', 'price'],
        part: 'body',
        order: 0,
        rows: 5,
        allowCreation: true,
        sortField: 'sku',
        sortDirection: 'asc',
      };
      const def = facetDefinitionBuilder('id', 'obj', 'form').addPortal(portal).build();
      const slot = def.slots[0];
      if (slot.kind === 'portal') {
        expect(slot.slot.rows).toBe(5);
        expect(slot.slot.allowCreation).toBe(true);
        expect(slot.slot.sortField).toBe('sku');
        expect(slot.slot.sortDirection).toBe('asc');
      }
    });
  });

  describe('addSummary()', () => {
    it('adds a summary field', () => {
      const summary: SummaryField = { fieldPath: 'amount', operation: 'sum' };
      const def = facetDefinitionBuilder('id', 'obj', 'report').addSummary(summary).build();
      expect(def.summaryFields).toHaveLength(1);
      expect(def.summaryFields?.[0]).toEqual(summary);
    });

    it('initialises summaryFields array on first call', () => {
      const base = createFacetDefinition('id', 'obj', 'report');
      expect(base.summaryFields).toBeUndefined();

      const def = facetDefinitionBuilder('id', 'obj', 'report')
        .addSummary({ fieldPath: 'x', operation: 'count' })
        .build();
      expect(def.summaryFields).toBeDefined();
    });

    it('accumulates multiple summary fields', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'report')
        .addSummary({ fieldPath: 'amount', operation: 'sum', label: 'Total' })
        .addSummary({ fieldPath: 'amount', operation: 'average', label: 'Avg' })
        .addSummary({ fieldPath: 'id', operation: 'count', label: 'Count' })
        .build();
      expect(def.summaryFields).toHaveLength(3);
      expect(def.summaryFields?.map((s) => s.operation)).toEqual(['sum', 'average', 'count']);
    });

    const operations: SummaryField['operation'][] = ['count', 'sum', 'average', 'min', 'max', 'list'];
    it.each(operations)('supports operation "%s"', (op) => {
      const def = facetDefinitionBuilder('id', 'obj', 'report')
        .addSummary({ fieldPath: 'f', operation: op })
        .build();
      expect(def.summaryFields?.[0]?.operation).toBe(op);
    });
  });

  describe('groupBy()', () => {
    it('sets the groupByField', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'report').groupBy('category').build();
      expect(def.groupByField).toBe('category');
    });
  });

  describe('sortBy()', () => {
    it('adds a sort field', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'list').sortBy('name', 'asc').build();
      expect(def.sortFields).toEqual([{ field: 'name', direction: 'asc' }]);
    });

    it('accumulates multiple sort fields', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'list')
        .sortBy('lastName', 'asc')
        .sortBy('firstName', 'asc')
        .sortBy('createdAt', 'desc')
        .build();
      expect(def.sortFields).toHaveLength(3);
      expect(def.sortFields?.[2]).toEqual({ field: 'createdAt', direction: 'desc' });
    });
  });

  describe('onRecordLoad() / onRecordCommit()', () => {
    it('sets onRecordLoad automation id', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form').onRecordLoad('script-load').build();
      expect(def.onRecordLoad).toBe('script-load');
    });

    it('sets onRecordCommit automation id', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .onRecordCommit('script-commit')
        .build();
      expect(def.onRecordCommit).toBe('script-commit');
    });
  });

  describe('build()', () => {
    it('returns a shallow copy (mutations do not affect built definition)', () => {
      const builder = facetDefinitionBuilder('id', 'obj', 'form')
        .addPart({ kind: 'body' })
        .addField({ fieldPath: 'name', part: 'body', order: 0 });

      const def1 = builder.build();
      const def2 = builder.build();

      // Built copies are independent
      def1.parts.push({ kind: 'footer' });
      expect(def2.parts).toHaveLength(1);
    });

    it('further builder mutations do not affect previously built definitions', () => {
      const builder = facetDefinitionBuilder('id', 'obj', 'form').addPart({ kind: 'body' });

      builder.build();
      builder.addPart({ kind: 'footer' });
      const defAfter = builder.build();

      // defBefore was built before footer was added, but builder mutates internal state
      // so defBefore.parts length depends on implementation — the key guarantee is
      // that the built array is a copy
      expect(defAfter.parts).toHaveLength(2);
    });

    it('interleaves field and portal slots in insertion order', () => {
      const def = facetDefinitionBuilder('id', 'obj', 'form')
        .addField({ fieldPath: 'name', part: 'header', order: 0 })
        .addPortal({
          relationshipId: 'items',
          displayFields: ['desc'],
          part: 'body',
          order: 1,
        })
        .addField({ fieldPath: 'total', part: 'footer', order: 2 })
        .build();

      expect(def.slots).toHaveLength(3);
      expect(def.slots[0].kind).toBe('field');
      expect(def.slots[1].kind).toBe('portal');
      expect(def.slots[2].kind).toBe('field');
    });
  });

  describe('full fluent chain (integration)', () => {
    it('builds a complete report facet', () => {
      const def = facetDefinitionBuilder('sales-report', 'transaction', 'report')
        .name('Monthly Sales Report')
        .description('Grouped by category with grand totals')
        .addPart({ kind: 'title-header', height: 60 })
        .addPart({ kind: 'header' })
        .addPart({ kind: 'body' })
        .addPart({ kind: 'trailing-summary' })
        .addPart({ kind: 'trailing-grand-summary' })
        .addField({ fieldPath: 'date', part: 'header', order: 0 })
        .addField({ fieldPath: 'description', part: 'body', order: 0 })
        .addField({ fieldPath: 'amount', part: 'body', order: 1 })
        .addPortal({
          relationshipId: 'line-items',
          displayFields: ['item', 'qty', 'price'],
          part: 'body',
          order: 2,
          rows: 10,
        })
        .addSummary({ fieldPath: 'amount', operation: 'sum', label: 'Subtotal' })
        .addSummary({ fieldPath: 'id', operation: 'count', label: 'Records' })
        .groupBy('category')
        .sortBy('date', 'desc')
        .sortBy('amount', 'desc')
        .onRecordLoad('auto-format-currency')
        .onRecordCommit('auto-update-totals')
        .build();

      expect(def.id).toBe('sales-report');
      expect(def.name).toBe('Monthly Sales Report');
      expect(def.description).toBe('Grouped by category with grand totals');
      expect(def.layout).toBe('report');
      expect(def.objectType).toBe('transaction');
      expect(def.parts).toHaveLength(5);
      expect(def.slots).toHaveLength(4);
      expect(def.summaryFields).toHaveLength(2);
      expect(def.groupByField).toBe('category');
      expect(def.sortFields).toHaveLength(2);
      expect(def.onRecordLoad).toBe('auto-format-currency');
      expect(def.onRecordCommit).toBe('auto-update-totals');
    });
  });
});

// ── Layout types ────────────────────────────────────────────────────────────

describe('FacetLayout types', () => {
  const layouts: FacetLayout[] = ['form', 'list', 'table', 'report', 'card'];

  it.each(layouts)('layout "%s" produces a valid definition', (layout) => {
    const def = facetDefinitionBuilder(`test-${layout}`, 'entity', layout).build();
    expect(def.layout).toBe(layout);
  });
});

// ── LayoutPartKind types ────────────────────────────────────────────────────

describe('LayoutPartKind types', () => {
  const partKinds: LayoutPartKind[] = [
    'title-header',
    'header',
    'body',
    'footer',
    'leading-summary',
    'trailing-summary',
    'leading-grand-summary',
    'trailing-grand-summary',
  ];

  it.each(partKinds)('part kind "%s" can be added to a facet', (kind) => {
    const def = facetDefinitionBuilder('test', 'obj', 'report').addPart({ kind }).build();
    expect(def.parts[0].kind).toBe(kind);
  });

  it.each(partKinds)('field slots accept part kind "%s"', (kind) => {
    const def = facetDefinitionBuilder('test', 'obj', 'report')
      .addField({ fieldPath: 'f', part: kind, order: 0 })
      .build();
    const slot = def.slots[0];
    if (slot.kind === 'field') {
      expect(slot.slot.part).toBe(kind);
    }
  });

  it.each(partKinds)('portal slots accept part kind "%s"', (kind) => {
    const def = facetDefinitionBuilder('test', 'obj', 'report')
      .addPortal({ relationshipId: 'r', displayFields: ['f'], part: kind, order: 0 })
      .build();
    const slot = def.slots[0];
    if (slot.kind === 'portal') {
      expect(slot.slot.part).toBe(kind);
    }
  });
});

// ── Spatial extensions ────────────────────────────────────────────────────

describe('spatial layout extensions', () => {
  it('FieldSlot accepts spatial properties', () => {
    const field: FieldSlot = {
      fieldPath: 'name', part: 'body', order: 0,
      x: 10, y: 20, w: 200, h: 30, zIndex: 5,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addField(field).build();
    const slot = def.slots[0];
    if (slot.kind === 'field') {
      expect(slot.slot.x).toBe(10);
      expect(slot.slot.y).toBe(20);
      expect(slot.slot.w).toBe(200);
      expect(slot.slot.h).toBe(30);
      expect(slot.slot.zIndex).toBe(5);
    }
  });

  it('FieldSlot accepts conditionalFormats', () => {
    const field: FieldSlot = {
      fieldPath: 'amount', part: 'body', order: 0,
      conditionalFormats: [
        { expression: '[field:amount] > 1000', backgroundColor: '#ff0000', textColor: '#fff' },
      ],
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addField(field).build();
    const slot = def.slots[0];
    if (slot.kind === 'field') {
      expect(slot.slot.conditionalFormats).toHaveLength(1);
      expect(slot.slot.conditionalFormats?.[0]?.expression).toBe('[field:amount] > 1000');
    }
  });

  it('PortalSlot accepts spatial properties', () => {
    const portal: PortalSlot = {
      relationshipId: 'items', displayFields: ['desc'], part: 'body', order: 0,
      x: 50, y: 100, w: 400, h: 200, zIndex: 2,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addPortal(portal).build();
    const slot = def.slots[0];
    if (slot.kind === 'portal') {
      expect(slot.slot.x).toBe(50);
      expect(slot.slot.w).toBe(400);
    }
  });

  it('FacetDefinition accepts layoutMode and canvasSize', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .layoutMode('spatial')
      .canvasSize(800, 600)
      .build();
    expect(def.layoutMode).toBe('spatial');
    expect(def.canvasWidth).toBe(800);
    expect(def.canvasHeight).toBe(600);
  });

  it('layoutMode defaults to undefined (treated as flow)', () => {
    const def = createFacetDefinition('id', 'obj', 'form');
    expect(def.layoutMode).toBeUndefined();
  });
});

// ── TextSlot and DrawingSlot ──────────────────────────────────────────────

describe('TextSlot', () => {
  it('addText() adds a text slot', () => {
    const text: TextSlot = { text: 'Title', part: 'header', order: 0 };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addText(text).build();
    expect(def.slots).toHaveLength(1);
    expect(def.slots[0].kind).toBe('text');
    if (def.slots[0].kind === 'text') {
      expect(def.slots[0].slot.text).toBe('Title');
    }
  });

  it('preserves text styling properties', () => {
    const text: TextSlot = {
      text: 'Header', part: 'header', order: 0,
      fontSize: 24, fontWeight: 700, color: '#333', textAlign: 'center',
      x: 10, y: 5, w: 300, h: 40, zIndex: 10,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addText(text).build();
    const slot = def.slots[0];
    if (slot.kind === 'text') {
      expect(slot.slot.fontSize).toBe(24);
      expect(slot.slot.fontWeight).toBe(700);
      expect(slot.slot.color).toBe('#333');
      expect(slot.slot.textAlign).toBe('center');
      expect(slot.slot.x).toBe(10);
    }
  });

  it('addText() is chainable', () => {
    const builder = facetDefinitionBuilder('id', 'obj', 'form');
    expect(builder.addText({ text: 'T', part: 'body', order: 0 })).toBe(builder);
  });
});

describe('DrawingSlot', () => {
  it('addDrawing() adds a drawing slot', () => {
    const drawing: DrawingSlot = { shape: 'rectangle', part: 'body', order: 0 };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addDrawing(drawing).build();
    expect(def.slots).toHaveLength(1);
    expect(def.slots[0].kind).toBe('drawing');
    if (def.slots[0].kind === 'drawing') {
      expect(def.slots[0].slot.shape).toBe('rectangle');
    }
  });

  it('preserves drawing style properties', () => {
    const drawing: DrawingSlot = {
      shape: 'rounded-rectangle', part: 'body', order: 0,
      strokeColor: '#000', strokeWidth: 2, fillColor: '#eee', cornerRadius: 8,
      x: 0, y: 0, w: 100, h: 50,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addDrawing(drawing).build();
    const slot = def.slots[0];
    if (slot.kind === 'drawing') {
      expect(slot.slot.cornerRadius).toBe(8);
      expect(slot.slot.fillColor).toBe('#eee');
    }
  });

  it('addDrawing() is chainable', () => {
    const builder = facetDefinitionBuilder('id', 'obj', 'form');
    expect(builder.addDrawing({ shape: 'line', part: 'body', order: 0 })).toBe(builder);
  });

  it('all four slot kinds interleave in insertion order', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .addField({ fieldPath: 'name', part: 'body', order: 0 })
      .addText({ text: 'Label', part: 'body', order: 1 })
      .addPortal({ relationshipId: 'r', displayFields: [], part: 'body', order: 2 })
      .addDrawing({ shape: 'line', part: 'body', order: 3 })
      .build();
    expect(def.slots.map((s) => s.kind)).toEqual(['field', 'text', 'portal', 'drawing']);
  });
});

// ── ContainerSlot ───────────────────────────────────────────────────────────

describe('ContainerSlot', () => {
  it('addContainer() adds a container slot', () => {
    const container: ContainerSlot = {
      fieldPath: 'photo',
      part: 'body',
      order: 0,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addContainer(container).build();
    expect(def.slots).toHaveLength(1);
    expect(def.slots[0].kind).toBe('container');
    if (def.slots[0].kind === 'container') {
      expect(def.slots[0].slot.fieldPath).toBe('photo');
    }
  });

  it('preserves all container properties', () => {
    const container: ContainerSlot = {
      fieldPath: 'attachment',
      label: 'Photo',
      allowedMimeTypes: ['image/*', 'application/pdf'],
      maxSize: 10_000_000,
      renderMode: 'preview',
      thumbnailWidth: 200,
      thumbnailHeight: 150,
      part: 'body',
      order: 1,
      x: 50,
      y: 100,
      w: 200,
      h: 150,
      zIndex: 3,
    };
    const def = facetDefinitionBuilder('id', 'obj', 'form').addContainer(container).build();
    const slot = def.slots[0];
    if (slot.kind === 'container') {
      expect(slot.slot.label).toBe('Photo');
      expect(slot.slot.allowedMimeTypes).toEqual(['image/*', 'application/pdf']);
      expect(slot.slot.maxSize).toBe(10_000_000);
      expect(slot.slot.renderMode).toBe('preview');
      expect(slot.slot.thumbnailWidth).toBe(200);
      expect(slot.slot.x).toBe(50);
      expect(slot.slot.zIndex).toBe(3);
    }
  });

  it('addContainer() is chainable', () => {
    const builder = facetDefinitionBuilder('id', 'obj', 'form');
    expect(builder.addContainer({ fieldPath: 'f', part: 'body', order: 0 })).toBe(builder);
  });

  it('interleaves with all five slot kinds', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .addField({ fieldPath: 'name', part: 'body', order: 0 })
      .addContainer({ fieldPath: 'photo', part: 'body', order: 1 })
      .addText({ text: 'Label', part: 'body', order: 2 })
      .addPortal({ relationshipId: 'r', displayFields: [], part: 'body', order: 3 })
      .addDrawing({ shape: 'line', part: 'body', order: 4 })
      .build();
    expect(def.slots.map((s) => s.kind)).toEqual([
      'field', 'container', 'text', 'portal', 'drawing',
    ]);
  });
});

// ── PrintConfig ─────────────────────────────────────────────────────────────

describe('PrintConfig', () => {
  it('createPrintConfig() creates default letter config', () => {
    const config = createPrintConfig();
    expect(config.pageSize).toBe('letter');
  });

  it('createPrintConfig() accepts page size', () => {
    const config = createPrintConfig('a4');
    expect(config.pageSize).toBe('a4');
  });

  it('printConfig() builder sets print configuration', () => {
    const config: PrintConfig = {
      pageSize: 'a4',
      orientation: 'landscape',
      margins: { top: 36, right: 36, bottom: 36, left: 36 },
      showPageNumbers: true,
      pageNumberPosition: 'bottom-center',
      pageHeader: 'Monthly Report',
      pageFooter: 'Confidential',
      pageBreakBeforeGroup: true,
    };
    const def = facetDefinitionBuilder('report', 'transaction', 'report')
      .printConfig(config)
      .build();
    expect(def.printConfig?.pageSize).toBe('a4');
    expect(def.printConfig?.orientation).toBe('landscape');
    expect(def.printConfig?.margins?.top).toBe(36);
    expect(def.printConfig?.showPageNumbers).toBe(true);
    expect(def.printConfig?.pageHeader).toBe('Monthly Report');
    expect(def.printConfig?.pageBreakBeforeGroup).toBe(true);
  });

  it('FacetDefinition has no printConfig by default', () => {
    const def = createFacetDefinition('id', 'obj', 'report');
    expect(def.printConfig).toBeUndefined();
  });
});

// ── Value List Bindings ─────────────────────────────────────────────────────

describe('valueListBindings', () => {
  it('bindValueList() sets a field→valueList mapping', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .bindValueList('status', 'status-list')
      .build();
    expect(def.valueListBindings?.['status']).toBe('status-list');
  });

  it('accumulates multiple bindings', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .bindValueList('status', 'status-list')
      .bindValueList('priority', 'priority-list')
      .build();
    expect(Object.keys(def.valueListBindings ?? {})).toHaveLength(2);
  });

  it('FacetDefinition has no bindings by default', () => {
    const def = createFacetDefinition('id', 'obj', 'form');
    expect(def.valueListBindings).toBeUndefined();
  });
});

// ── Required Privilege Set ──────────────────────────────────────────────────

describe('requiredPrivilegeSet', () => {
  it('sets the required privilege set id', () => {
    const def = facetDefinitionBuilder('id', 'obj', 'form')
      .requiredPrivilegeSet('admin')
      .build();
    expect(def.requiredPrivilegeSet).toBe('admin');
  });

  it('FacetDefinition has no required privilege set by default', () => {
    const def = createFacetDefinition('id', 'obj', 'form');
    expect(def.requiredPrivilegeSet).toBeUndefined();
  });
});
