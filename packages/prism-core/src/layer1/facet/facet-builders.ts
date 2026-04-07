/**
 * FacetBuilders — Lua codegen helpers that generate common Lua patterns
 * for Prism plugins. Each function is pure: config in → Lua string out.
 */

// -- Helper -------------------------------------------------------------------

function luaString(s: string): string {
  return `"${s.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

// -- Browser View -------------------------------------------------------------

export interface BrowserViewColumn {
  field: string;
  label: string;
  width?: number;
}

export interface BrowserViewConfig {
  collectionId: string;
  title: string;
  columns: BrowserViewColumn[];
  sortField?: string;
  sortDirection?: 'asc' | 'desc';
  filterExpression?: string;
}

/**
 * Generate Lua for a collection browser view.
 *
 * Emits a `ui.column` layout with column headers and data rows
 * populated via `Collection.list()`.
 */
export function luaBrowserView(config: BrowserViewConfig): string {
  const { collectionId, title, columns, sortField, sortDirection, filterExpression } = config;

  const headerCells = columns
    .map((col) => {
      const widthArg = col.width !== undefined ? `, width = ${col.width}` : '';
      return `    ui.label(${luaString(col.label)}${widthArg})`;
    })
    .join(',\n');

  const sortOpts: string[] = [];
  if (sortField) sortOpts.push(`sort = ${luaString(sortField)}`);
  if (sortDirection) sortOpts.push(`direction = ${luaString(sortDirection)}`);
  if (filterExpression) sortOpts.push(`filter = ${luaString(filterExpression)}`);

  const listArgs = sortOpts.length > 0
    ? `${luaString(collectionId)}, { ${sortOpts.join(', ')} }`
    : luaString(collectionId);

  const rowCells = columns
    .map((col) => `      ui.text(item.${col.field})`)
    .join(',\n');

  return [
    `local items = Collection.list(${listArgs})`,
    `local rows = {}`,
    `for _, item in ipairs(items) do`,
    `  rows[#rows + 1] = ui.row({`,
    rowCells,
    `  })`,
    `end`,
    ``,
    `local view = ui.column({`,
    `  ui.label(${luaString(title)}),`,
    `  ui.row({`,
    headerCells,
    `  }),`,
    `  table.unpack(rows)`,
    `})`,
    `return view`,
  ].join('\n');
}

// -- Collection Rule ----------------------------------------------------------

export interface CollectionRuleConfig {
  ruleName: string;
  entityType: string;
  field: string;
  operator: string;
  value: string;
  message: string;
}

/**
 * Generate Lua for a collection validation rule.
 *
 * Emits a `validate_<ruleName>` function that checks a field condition
 * on entities of the given type.
 */
export function luaCollectionRule(config: CollectionRuleConfig): string {
  const { ruleName, entityType, field, operator, value, message } = config;

  return [
    `function validate_${ruleName}(obj)`,
    `  if obj.type == ${luaString(entityType)} then`,
    `    if not (obj.${field} ${operator} ${value}) then`,
    `      return { valid = false, message = ${luaString(message)} }`,
    `    end`,
    `  end`,
    `  return { valid = true }`,
    `end`,
  ].join('\n');
}

// -- Stats Command ------------------------------------------------------------

export type StatsOperation = 'count' | 'sum' | 'avg' | 'min' | 'max';

export interface StatsFieldConfig {
  field: string;
  operation: StatsOperation;
}

export interface StatsCommandConfig {
  commandName: string;
  collectionId: string;
  fields: StatsFieldConfig[];
}

/**
 * Generate Lua for a summary/statistics command.
 *
 * Emits a `cmd_<commandName>` function that iterates `Collection.list()`
 * and computes count/sum/avg/min/max for each specified field.
 */
export function luaStatsCommand(config: StatsCommandConfig): string {
  const { commandName, collectionId, fields } = config;

  const initLines: string[] = [];
  const loopLines: string[] = [];
  const postLines: string[] = [];

  for (const f of fields) {
    const key = `${f.field}_${f.operation}`;
    switch (f.operation) {
      case 'count':
        initLines.push(`  stats.${key} = 0`);
        loopLines.push(`    stats.${key} = stats.${key} + 1`);
        break;
      case 'sum':
        initLines.push(`  stats.${key} = 0`);
        loopLines.push(`    stats.${key} = stats.${key} + (item.${f.field} or 0)`);
        break;
      case 'avg':
        initLines.push(`  stats.${key} = 0`);
        initLines.push(`  local ${key}_total = 0`);
        initLines.push(`  local ${key}_count = 0`);
        loopLines.push(`    ${key}_total = ${key}_total + (item.${f.field} or 0)`);
        loopLines.push(`    ${key}_count = ${key}_count + 1`);
        postLines.push(`  if ${key}_count > 0 then stats.${key} = ${key}_total / ${key}_count end`);
        break;
      case 'min':
        initLines.push(`  stats.${key} = nil`);
        loopLines.push(`    if stats.${key} == nil or item.${f.field} < stats.${key} then stats.${key} = item.${f.field} end`);
        break;
      case 'max':
        initLines.push(`  stats.${key} = nil`);
        loopLines.push(`    if stats.${key} == nil or item.${f.field} > stats.${key} then stats.${key} = item.${f.field} end`);
        break;
    }
  }

  return [
    `function cmd_${commandName}()`,
    `  local items = Collection.list(${luaString(collectionId)})`,
    `  local stats = {}`,
    ...initLines,
    `  for _, item in ipairs(items) do`,
    ...loopLines,
    `  end`,
    ...postLines,
    `  return stats`,
    `end`,
  ].join('\n');
}

// -- Menu Item ----------------------------------------------------------------

export interface MenuItemConfig {
  id: string;
  label: string;
  icon?: string;
  shortcut?: string;
  action: string;
}

/**
 * Generate Lua for a menu contribution.
 *
 * Emits a `Plugin.registerMenu()` call with the given properties.
 */
export function luaMenuItem(config: MenuItemConfig): string {
  const { id, label, icon, shortcut, action } = config;

  const fields: string[] = [
    `  id = ${luaString(id)}`,
    `  label = ${luaString(label)}`,
  ];

  if (icon !== undefined) {
    fields.push(`  icon = ${luaString(icon)}`);
  }
  if (shortcut !== undefined) {
    fields.push(`  shortcut = ${luaString(shortcut)}`);
  }

  fields.push(`  action = function()\n    ${action}\n  end`);

  return [
    `Plugin.registerMenu({`,
    fields.join(',\n'),
    `})`,
  ].join('\n');
}

// -- Command ------------------------------------------------------------------

export interface CommandConfig {
  id: string;
  name: string;
  shortcut: string;
  body: string;
}

/**
 * Generate Lua for a keyboard command.
 *
 * Emits a `Plugin.registerCommand()` call with an `execute` callback.
 */
export function luaCommand(config: CommandConfig): string {
  const { id, name, shortcut, body } = config;

  return [
    `Plugin.registerCommand({`,
    `  id = ${luaString(id)},`,
    `  name = ${luaString(name)},`,
    `  shortcut = ${luaString(shortcut)},`,
    `  execute = function(ctx)`,
    `    ${body}`,
    `  end`,
    `})`,
  ].join('\n');
}
