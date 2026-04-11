import { describe, it, expect } from 'vitest';
import {
  luauBrowserView,
  luauCollectionRule,
  luauStatsCommand,
  luauMenuItem,
  luauCommand,
} from './facet-builders.js';

describe('luauBrowserView', () => {
  it('generates a collection browser view with headers and data rows', () => {
    const luau = luauBrowserView({
      collectionId: 'tasks',
      title: 'All Tasks',
      columns: [
        { field: 'name', label: 'Name', width: 200 },
        { field: 'status', label: 'Status' },
        { field: 'priority', label: 'Priority', width: 100 },
      ],
    });

    expect(luau).toContain('Collection.list("tasks")');
    expect(luau).toContain('ui.label("All Tasks")');
    expect(luau).toContain('ui.label("Name", width = 200)');
    expect(luau).toContain('ui.label("Status")');
    expect(luau).toContain('ui.text(item.name)');
    expect(luau).toContain('ui.text(item.status)');
    expect(luau).toContain('ui.text(item.priority)');
    expect(luau).toContain('return view');
  });

  it('includes sort and filter options when specified', () => {
    const luau = luauBrowserView({
      collectionId: 'contacts',
      title: 'Contacts',
      columns: [{ field: 'name', label: 'Name' }],
      sortField: 'name',
      sortDirection: 'asc',
      filterExpression: 'status == "active"',
    });

    expect(luau).toContain('sort = "name"');
    expect(luau).toContain('direction = "asc"');
    expect(luau).toContain('filter = "status == \\"active\\""');
  });
});

describe('luauCollectionRule', () => {
  it('generates a validation function for a field condition', () => {
    const luau = luauCollectionRule({
      ruleName: 'positive_amount',
      entityType: 'Transaction',
      field: 'amount',
      operator: '>',
      value: '0',
      message: 'Amount must be positive',
    });

    expect(luau).toContain('function validate_positive_amount(obj)');
    expect(luau).toContain('obj.type == "Transaction"');
    expect(luau).toContain('not (obj.amount > 0)');
    expect(luau).toContain('valid = false');
    expect(luau).toContain('"Amount must be positive"');
    expect(luau).toContain('valid = true');
  });

  it('escapes special characters in the message', () => {
    const luau = luauCollectionRule({
      ruleName: 'name_check',
      entityType: 'Contact',
      field: 'name',
      operator: '~=',
      value: '""',
      message: 'Name can\'t be "empty"',
    });

    expect(luau).toContain('Name can\'t be \\"empty\\"');
  });
});

describe('luauStatsCommand', () => {
  it('generates stats computation for count and sum', () => {
    const luau = luauStatsCommand({
      commandName: 'task_summary',
      collectionId: 'tasks',
      fields: [
        { field: 'id', operation: 'count' },
        { field: 'hours', operation: 'sum' },
      ],
    });

    expect(luau).toContain('function cmd_task_summary()');
    expect(luau).toContain('Collection.list("tasks")');
    expect(luau).toContain('stats.id_count = stats.id_count + 1');
    expect(luau).toContain('stats.hours_sum = stats.hours_sum + (item.hours or 0)');
    expect(luau).toContain('return stats');
  });

  it('generates avg with separate total and count tracking', () => {
    const luau = luauStatsCommand({
      commandName: 'avg_score',
      collectionId: 'reviews',
      fields: [{ field: 'score', operation: 'avg' }],
    });

    expect(luau).toContain('score_avg_total = 0');
    expect(luau).toContain('score_avg_count = 0');
    expect(luau).toContain('score_avg_total = score_avg_total + (item.score or 0)');
    expect(luau).toContain('score_avg_count = score_avg_count + 1');
    expect(luau).toContain('stats.score_avg = score_avg_total / score_avg_count');
  });

  it('generates min and max with nil-guarded comparisons', () => {
    const luau = luauStatsCommand({
      commandName: 'price_range',
      collectionId: 'items',
      fields: [
        { field: 'price', operation: 'min' },
        { field: 'price', operation: 'max' },
      ],
    });

    expect(luau).toContain('stats.price_min = nil');
    expect(luau).toContain('stats.price_max = nil');
    expect(luau).toContain('item.price < stats.price_min');
    expect(luau).toContain('item.price > stats.price_max');
  });
});

describe('luauMenuItem', () => {
  it('generates a Plugin.registerMenu call with all options', () => {
    const luau = luauMenuItem({
      id: 'file.export-csv',
      label: 'Export as CSV',
      icon: 'download',
      shortcut: 'Ctrl+Shift+E',
      action: 'export_collection("csv")',
    });

    expect(luau).toContain('Plugin.registerMenu({');
    expect(luau).toContain('id = "file.export-csv"');
    expect(luau).toContain('label = "Export as CSV"');
    expect(luau).toContain('icon = "download"');
    expect(luau).toContain('shortcut = "Ctrl+Shift+E"');
    expect(luau).toContain('action = function()');
    expect(luau).toContain('export_collection("csv")');
  });

  it('omits icon and shortcut when not provided', () => {
    const luau = luauMenuItem({
      id: 'help.about',
      label: 'About',
      action: 'show_about()',
    });

    expect(luau).not.toContain('icon');
    expect(luau).not.toContain('shortcut');
    expect(luau).toContain('id = "help.about"');
    expect(luau).toContain('show_about()');
  });
});

describe('luauCommand', () => {
  it('generates a Plugin.registerCommand call with execute callback', () => {
    const luau = luauCommand({
      id: 'editor.save',
      name: 'Save Document',
      shortcut: 'Ctrl+S',
      body: 'ctx.document:save()',
    });

    expect(luau).toContain('Plugin.registerCommand({');
    expect(luau).toContain('id = "editor.save"');
    expect(luau).toContain('name = "Save Document"');
    expect(luau).toContain('shortcut = "Ctrl+S"');
    expect(luau).toContain('execute = function(ctx)');
    expect(luau).toContain('ctx.document:save()');
    expect(luau).toContain('})');
  });

  it('handles special characters in the body', () => {
    const luau = luauCommand({
      id: 'debug.log',
      name: 'Log Selection',
      shortcut: 'Ctrl+L',
      body: 'print("selected: " .. tostring(ctx.selection))',
    });

    expect(luau).toContain('print("selected: " .. tostring(ctx.selection))');
  });
});
