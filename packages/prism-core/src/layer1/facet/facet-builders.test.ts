import { describe, it, expect } from 'vitest';
import {
  luaBrowserView,
  luaCollectionRule,
  luaStatsCommand,
  luaMenuItem,
  luaCommand,
} from './facet-builders.js';

describe('luaBrowserView', () => {
  it('generates a collection browser view with headers and data rows', () => {
    const lua = luaBrowserView({
      collectionId: 'tasks',
      title: 'All Tasks',
      columns: [
        { field: 'name', label: 'Name', width: 200 },
        { field: 'status', label: 'Status' },
        { field: 'priority', label: 'Priority', width: 100 },
      ],
    });

    expect(lua).toContain('Collection.list("tasks")');
    expect(lua).toContain('ui.label("All Tasks")');
    expect(lua).toContain('ui.label("Name", width = 200)');
    expect(lua).toContain('ui.label("Status")');
    expect(lua).toContain('ui.text(item.name)');
    expect(lua).toContain('ui.text(item.status)');
    expect(lua).toContain('ui.text(item.priority)');
    expect(lua).toContain('return view');
  });

  it('includes sort and filter options when specified', () => {
    const lua = luaBrowserView({
      collectionId: 'contacts',
      title: 'Contacts',
      columns: [{ field: 'name', label: 'Name' }],
      sortField: 'name',
      sortDirection: 'asc',
      filterExpression: 'status == "active"',
    });

    expect(lua).toContain('sort = "name"');
    expect(lua).toContain('direction = "asc"');
    expect(lua).toContain('filter = "status == \\"active\\""');
  });
});

describe('luaCollectionRule', () => {
  it('generates a validation function for a field condition', () => {
    const lua = luaCollectionRule({
      ruleName: 'positive_amount',
      entityType: 'Transaction',
      field: 'amount',
      operator: '>',
      value: '0',
      message: 'Amount must be positive',
    });

    expect(lua).toContain('function validate_positive_amount(obj)');
    expect(lua).toContain('obj.type == "Transaction"');
    expect(lua).toContain('not (obj.amount > 0)');
    expect(lua).toContain('valid = false');
    expect(lua).toContain('"Amount must be positive"');
    expect(lua).toContain('valid = true');
  });

  it('escapes special characters in the message', () => {
    const lua = luaCollectionRule({
      ruleName: 'name_check',
      entityType: 'Contact',
      field: 'name',
      operator: '~=',
      value: '""',
      message: 'Name can\'t be "empty"',
    });

    expect(lua).toContain('Name can\'t be \\"empty\\"');
  });
});

describe('luaStatsCommand', () => {
  it('generates stats computation for count and sum', () => {
    const lua = luaStatsCommand({
      commandName: 'task_summary',
      collectionId: 'tasks',
      fields: [
        { field: 'id', operation: 'count' },
        { field: 'hours', operation: 'sum' },
      ],
    });

    expect(lua).toContain('function cmd_task_summary()');
    expect(lua).toContain('Collection.list("tasks")');
    expect(lua).toContain('stats.id_count = stats.id_count + 1');
    expect(lua).toContain('stats.hours_sum = stats.hours_sum + (item.hours or 0)');
    expect(lua).toContain('return stats');
  });

  it('generates avg with separate total and count tracking', () => {
    const lua = luaStatsCommand({
      commandName: 'avg_score',
      collectionId: 'reviews',
      fields: [{ field: 'score', operation: 'avg' }],
    });

    expect(lua).toContain('score_avg_total = 0');
    expect(lua).toContain('score_avg_count = 0');
    expect(lua).toContain('score_avg_total = score_avg_total + (item.score or 0)');
    expect(lua).toContain('score_avg_count = score_avg_count + 1');
    expect(lua).toContain('stats.score_avg = score_avg_total / score_avg_count');
  });

  it('generates min and max with nil-guarded comparisons', () => {
    const lua = luaStatsCommand({
      commandName: 'price_range',
      collectionId: 'items',
      fields: [
        { field: 'price', operation: 'min' },
        { field: 'price', operation: 'max' },
      ],
    });

    expect(lua).toContain('stats.price_min = nil');
    expect(lua).toContain('stats.price_max = nil');
    expect(lua).toContain('item.price < stats.price_min');
    expect(lua).toContain('item.price > stats.price_max');
  });
});

describe('luaMenuItem', () => {
  it('generates a Plugin.registerMenu call with all options', () => {
    const lua = luaMenuItem({
      id: 'file.export-csv',
      label: 'Export as CSV',
      icon: 'download',
      shortcut: 'Ctrl+Shift+E',
      action: 'export_collection("csv")',
    });

    expect(lua).toContain('Plugin.registerMenu({');
    expect(lua).toContain('id = "file.export-csv"');
    expect(lua).toContain('label = "Export as CSV"');
    expect(lua).toContain('icon = "download"');
    expect(lua).toContain('shortcut = "Ctrl+Shift+E"');
    expect(lua).toContain('action = function()');
    expect(lua).toContain('export_collection("csv")');
  });

  it('omits icon and shortcut when not provided', () => {
    const lua = luaMenuItem({
      id: 'help.about',
      label: 'About',
      action: 'show_about()',
    });

    expect(lua).not.toContain('icon');
    expect(lua).not.toContain('shortcut');
    expect(lua).toContain('id = "help.about"');
    expect(lua).toContain('show_about()');
  });
});

describe('luaCommand', () => {
  it('generates a Plugin.registerCommand call with execute callback', () => {
    const lua = luaCommand({
      id: 'editor.save',
      name: 'Save Document',
      shortcut: 'Ctrl+S',
      body: 'ctx.document:save()',
    });

    expect(lua).toContain('Plugin.registerCommand({');
    expect(lua).toContain('id = "editor.save"');
    expect(lua).toContain('name = "Save Document"');
    expect(lua).toContain('shortcut = "Ctrl+S"');
    expect(lua).toContain('execute = function(ctx)');
    expect(lua).toContain('ctx.document:save()');
    expect(lua).toContain('})');
  });

  it('handles special characters in the body', () => {
    const lua = luaCommand({
      id: 'debug.log',
      name: 'Log Selection',
      shortcut: 'Ctrl+L',
      body: 'print("selected: " .. tostring(ctx.selection))',
    });

    expect(lua).toContain('print("selected: " .. tostring(ctx.selection))');
  });
});
