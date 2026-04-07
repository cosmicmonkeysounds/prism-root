/**
 * Sequencer framework -- data model and Lua emission for visual condition/script builders.
 *
 * Ported from Helm's Wizard framework. The Sequencer lets non-programmers compose
 * Lua condition expressions and action scripts by selecting from dropdown menus.
 * Pure functions: SequencerConditionState -> Lua string, SequencerScriptState -> Lua string.
 *
 * Renamed: Wizard -> Sequencer throughout.
 */

// -- Condition types ----------------------------------------------------------

/** The subject type for a condition clause. */
export type SequencerSubjectKind =
  | 'variable'    // a named variable (Var["scope.id"])
  | 'field'       // a field on an entity (Entity["id"].fieldName)
  | 'event'       // whether an event has fired (Event.HasFired(id))
  | 'custom';     // raw Lua expression (escape hatch)

/** A subject available for selection in the sequencer (populated from a registry). */
export interface SequencerSubject {
  kind: SequencerSubjectKind;
  id: string;
  label: string;
  type?: string;  // 'number' | 'string' | 'boolean'
}

/** Comparison operator for a condition clause. */
export type SequencerOperator =
  | 'is'           // ==
  | 'is-not'       // ~=
  | 'gt'           // >
  | 'lt'           // <
  | 'gte'          // >=
  | 'lte'          // <=
  | 'contains'     // string.find(subject, value)
  | 'starts-with'  // string.sub(subject, 1, #value) == value
  | 'is-true'      // subject == true (no value needed)
  | 'is-false'     // subject == false (no value needed)
  | 'is-nil'       // subject == nil
  | 'is-not-nil';  // subject ~= nil

/** Logical combinator: all clauses must match (AND) or any (OR). */
export type SequencerCombinator = 'all' | 'any';

/** A single condition clause. */
export interface SequencerConditionClause {
  id: string;
  subjectKind: SequencerSubjectKind;
  /** The subject reference (e.g. 'player.gold', 'quest_started'). */
  subject: string;
  /** Sub-field for field subjects (e.g. 'relationship'). */
  subjectField?: string;
  operator: SequencerOperator;
  /** Value to compare against. Empty string for is-true/is-false/is-nil. */
  value: string;
}

/** The full condition state. */
export interface SequencerConditionState {
  combinator: SequencerCombinator;
  clauses: SequencerConditionClause[];
}

// -- Script types -------------------------------------------------------------

/** The kind of action in a script step. */
export type SequencerActionKind =
  | 'set-variable'   // Var.set("scope", "id", value)
  | 'add-variable'   // Var.set("scope", "id", Var.get("scope","id") + amount)
  | 'emit-event'     // Event.emit("eventId", payload?)
  | 'call-function'  // SomeLuaGlobal.method(args...)
  | 'custom';        // raw Lua statement

/** A single script action step. */
export interface SequencerScriptStep {
  id: string;
  actionKind: SequencerActionKind;
  /** Target (variable scope.id, event ID, function name). */
  target: string;
  /** Value or argument. */
  value: string;
  /** Extra arguments (for call-function). */
  extraArgs?: string[];
}

/** The full script state. */
export interface SequencerScriptState {
  steps: SequencerScriptStep[];
}

// -- Lua emission: conditions -------------------------------------------------

function emitSubject(clause: SequencerConditionClause): string {
  switch (clause.subjectKind) {
    case 'variable':
      return `Var["${clause.subject}"]`;
    case 'field':
      return clause.subjectField
        ? `Entity["${clause.subject}"].${clause.subjectField}`
        : `Entity["${clause.subject}"]`;
    case 'event':
      return `Event.HasFired("${clause.subject}")`;
    case 'custom':
      return clause.subject;
  }
}

function emitValue(value: string): string {
  if (value === 'true') return 'true';
  if (value === 'false') return 'false';
  if (value === 'nil') return 'nil';
  const n = Number(value);
  if (!isNaN(n) && value.trim() !== '') return value;
  return `"${value.replace(/"/g, '\\"')}"`;
}

function emitOperator(op: SequencerOperator, subject: string, value: string): string {
  const val = emitValue(value);
  switch (op) {
    case 'is':          return `${subject} == ${val}`;
    case 'is-not':      return `${subject} ~= ${val}`;
    case 'gt':          return `${subject} > ${val}`;
    case 'lt':          return `${subject} < ${val}`;
    case 'gte':         return `${subject} >= ${val}`;
    case 'lte':         return `${subject} <= ${val}`;
    case 'contains':    return `string.find(${subject}, ${val}) ~= nil`;
    case 'starts-with': return `string.sub(${subject}, 1, #${val}) == ${val}`;
    case 'is-true':     return `${subject} == true`;
    case 'is-false':    return `${subject} == false`;
    case 'is-nil':      return `${subject} == nil`;
    case 'is-not-nil':  return `${subject} ~= nil`;
  }
}

function emitClause(clause: SequencerConditionClause): string {
  const subject = emitSubject(clause);
  return emitOperator(clause.operator, subject, clause.value);
}

/**
 * Emit a Lua condition expression from a SequencerConditionState.
 *
 * @example
 *   emitConditionLua({ combinator: 'all', clauses: [
 *     { id: '1', subjectKind: 'variable', subject: 'player.gold', operator: 'gte', value: '50' },
 *     { id: '2', subjectKind: 'event', subject: 'quest_started', operator: 'is-true', value: '' },
 *   ]})
 *   // -> '(Var["player.gold"] >= 50 and Event.HasFired("quest_started") == true)'
 */
export function emitConditionLua(state: SequencerConditionState): string {
  if (state.clauses.length === 0) return 'true';

  const parts = state.clauses
    .filter((c) => c.subject.trim() !== '' || c.subjectKind === 'custom')
    .map(emitClause);

  if (parts.length === 0) return 'true';

  const join = state.combinator === 'all' ? ' and ' : ' or ';
  const expr = parts.join(join);

  // Wrap in parens if multiple clauses for clarity
  return parts.length > 1 ? `(${expr})` : expr;
}

// -- Lua emission: scripts ----------------------------------------------------

function emitStep(step: SequencerScriptStep): string {
  switch (step.actionKind) {
    case 'set-variable':
      return `Var["${step.target}"] = ${emitValue(step.value)}`;

    case 'add-variable': {
      const amount = emitValue(step.value);
      return `Var["${step.target}"] = Var["${step.target}"] + ${amount}`;
    }

    case 'emit-event': {
      const payload = step.value.trim()
        ? `, ${step.value.trim()}`
        : '';
      return `Event.emit("${step.target}"${payload})`;
    }

    case 'call-function': {
      const allArgs = [step.value, ...(step.extraArgs ?? [])]
        .filter((a) => a.trim() !== '')
        .join(', ');
      return `${step.target}(${allArgs})`;
    }

    case 'custom':
      return step.target || step.value;
  }
}

/**
 * Emit a multi-line Lua script from a SequencerScriptState.
 *
 * @example
 *   emitScriptLua({ steps: [
 *     { id: '1', actionKind: 'set-variable', target: 'player.gold', value: '100' },
 *     { id: '2', actionKind: 'emit-event', target: 'quest_complete', value: '' },
 *   ]})
 *   // -> 'Var["player.gold"] = 100\nEvent.emit("quest_complete")'
 */
export function emitScriptLua(state: SequencerScriptState): string {
  return state.steps
    .filter((s) => s.target.trim() !== '' || s.actionKind === 'custom')
    .map(emitStep)
    .join('\n');
}
