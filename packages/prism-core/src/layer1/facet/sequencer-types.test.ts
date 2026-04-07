import { describe, it, expect } from 'vitest';
import {
  emitConditionLua,
  emitScriptLua,
} from './sequencer-types.js';
import type {
  SequencerConditionState,
  SequencerConditionClause,
  SequencerScriptState,
  SequencerScriptStep,
} from './sequencer-types.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function clause(
  overrides: Partial<SequencerConditionClause> & Pick<SequencerConditionClause, 'subjectKind' | 'subject' | 'operator'>,
): SequencerConditionClause {
  return { id: '1', value: '', ...overrides };
}

function step(
  overrides: Partial<SequencerScriptStep> & Pick<SequencerScriptStep, 'actionKind' | 'target'>,
): SequencerScriptStep {
  return { id: '1', value: '', ...overrides };
}

// ---------------------------------------------------------------------------
// emitConditionLua
// ---------------------------------------------------------------------------

describe('emitConditionLua', () => {
  it('returns "true" for empty clauses', () => {
    const state: SequencerConditionState = { combinator: 'all', clauses: [] };
    expect(emitConditionLua(state)).toBe('true');
  });

  it('returns "true" when all clauses have blank subjects', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: '  ', operator: 'is', value: '1' })],
    };
    expect(emitConditionLua(state)).toBe('true');
  });

  // -- Subject kinds -------------------------------------------------------

  it('emits variable subject', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'player.gold', operator: 'is', value: '100' })],
    };
    expect(emitConditionLua(state)).toBe('Var["player.gold"] == 100');
  });

  it('emits field subject without sub-field', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'field', subject: 'npc1', operator: 'is-not-nil' })],
    };
    expect(emitConditionLua(state)).toBe('Entity["npc1"] ~= nil');
  });

  it('emits field subject with sub-field', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'field', subject: 'npc1', subjectField: 'relationship', operator: 'gte', value: '50' })],
    };
    expect(emitConditionLua(state)).toBe('Entity["npc1"].relationship >= 50');
  });

  it('emits event subject', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'event', subject: 'quest_started', operator: 'is-true' })],
    };
    expect(emitConditionLua(state)).toBe('Event.HasFired("quest_started") == true');
  });

  it('emits custom subject as raw Lua', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'custom', subject: 'math.random()', operator: 'gt', value: '0.5' })],
    };
    expect(emitConditionLua(state)).toBe('math.random() > 0.5');
  });

  // -- Operators -----------------------------------------------------------

  it('operator: is (==)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: 'hello' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == "hello"');
  });

  it('operator: is-not (~=)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is-not', value: '5' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] ~= 5');
  });

  it('operator: gt (>)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'gt', value: '10' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] > 10');
  });

  it('operator: lt (<)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'lt', value: '3' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] < 3');
  });

  it('operator: gte (>=)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'gte', value: '50' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] >= 50');
  });

  it('operator: lte (<=)', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'lte', value: '99' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] <= 99');
  });

  it('operator: contains', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'name', operator: 'contains', value: 'foo' })],
    };
    expect(emitConditionLua(state)).toBe('string.find(Var["name"], "foo") ~= nil');
  });

  it('operator: starts-with', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'name', operator: 'starts-with', value: 'pre' })],
    };
    expect(emitConditionLua(state)).toBe('string.sub(Var["name"], 1, #"pre") == "pre"');
  });

  it('operator: is-true', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'flag', operator: 'is-true' })],
    };
    expect(emitConditionLua(state)).toBe('Var["flag"] == true');
  });

  it('operator: is-false', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'flag', operator: 'is-false' })],
    };
    expect(emitConditionLua(state)).toBe('Var["flag"] == false');
  });

  it('operator: is-nil', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is-nil' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == nil');
  });

  it('operator: is-not-nil', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is-not-nil' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] ~= nil');
  });

  // -- Value emission ------------------------------------------------------

  it('emits boolean true without quotes', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: 'true' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == true');
  });

  it('emits boolean false without quotes', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: 'false' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == false');
  });

  it('emits nil without quotes', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: 'nil' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == nil');
  });

  it('emits numeric values without quotes', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: '42' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == 42');
  });

  it('emits string values with quotes and escapes embedded quotes', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: 'say "hi"' })],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == "say \\"hi\\""');
  });

  // -- Combinators ---------------------------------------------------------

  it('joins multiple clauses with "and" for combinator "all"', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [
        clause({ id: '1', subjectKind: 'variable', subject: 'player.gold', operator: 'gte', value: '50' }),
        clause({ id: '2', subjectKind: 'event', subject: 'quest_started', operator: 'is-true' }),
      ],
    };
    expect(emitConditionLua(state)).toBe('(Var["player.gold"] >= 50 and Event.HasFired("quest_started") == true)');
  });

  it('joins multiple clauses with "or" for combinator "any"', () => {
    const state: SequencerConditionState = {
      combinator: 'any',
      clauses: [
        clause({ id: '1', subjectKind: 'variable', subject: 'a', operator: 'is', value: '1' }),
        clause({ id: '2', subjectKind: 'variable', subject: 'b', operator: 'is', value: '2' }),
      ],
    };
    expect(emitConditionLua(state)).toBe('(Var["a"] == 1 or Var["b"] == 2)');
  });

  it('does not wrap single clause in parens', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'variable', subject: 'x', operator: 'is', value: '1' })],
    };
    const result = emitConditionLua(state);
    expect(result).not.toMatch(/^\(/);
    expect(result).not.toMatch(/\)$/);
  });

  it('filters out blank-subject non-custom clauses', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [
        clause({ id: '1', subjectKind: 'variable', subject: '', operator: 'is', value: '1' }),
        clause({ id: '2', subjectKind: 'variable', subject: 'x', operator: 'is', value: '2' }),
      ],
    };
    expect(emitConditionLua(state)).toBe('Var["x"] == 2');
  });

  it('keeps custom clauses even with blank subject', () => {
    const state: SequencerConditionState = {
      combinator: 'all',
      clauses: [clause({ subjectKind: 'custom', subject: '', operator: 'is-true' })],
    };
    // custom with blank subject emits ` == true` -- verifies it's not filtered
    expect(emitConditionLua(state)).not.toBe('true');
  });
});

// ---------------------------------------------------------------------------
// emitScriptLua
// ---------------------------------------------------------------------------

describe('emitScriptLua', () => {
  it('returns empty string for no steps', () => {
    const state: SequencerScriptState = { steps: [] };
    expect(emitScriptLua(state)).toBe('');
  });

  // -- set-variable --------------------------------------------------------

  it('emits set-variable with numeric value', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'set-variable', target: 'player.gold', value: '100' })],
    };
    expect(emitScriptLua(state)).toBe('Var["player.gold"] = 100');
  });

  it('emits set-variable with string value', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'set-variable', target: 'player.name', value: 'Alice' })],
    };
    expect(emitScriptLua(state)).toBe('Var["player.name"] = "Alice"');
  });

  it('emits set-variable with boolean value', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'set-variable', target: 'flag', value: 'true' })],
    };
    expect(emitScriptLua(state)).toBe('Var["flag"] = true');
  });

  // -- add-variable --------------------------------------------------------

  it('emits add-variable', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'add-variable', target: 'player.gold', value: '25' })],
    };
    expect(emitScriptLua(state)).toBe('Var["player.gold"] = Var["player.gold"] + 25');
  });

  // -- emit-event ----------------------------------------------------------

  it('emits emit-event without payload', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'emit-event', target: 'quest_complete', value: '' })],
    };
    expect(emitScriptLua(state)).toBe('Event.emit("quest_complete")');
  });

  it('emits emit-event with payload', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'emit-event', target: 'quest_complete', value: '{ reward = 50 }' })],
    };
    expect(emitScriptLua(state)).toBe('Event.emit("quest_complete", { reward = 50 })');
  });

  // -- call-function -------------------------------------------------------

  it('emits call-function with value arg', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'call-function', target: 'NPC.greet', value: '"hello"' })],
    };
    expect(emitScriptLua(state)).toBe('NPC.greet("hello")');
  });

  it('emits call-function with extra args', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'call-function', target: 'Math.clamp', value: 'x', extraArgs: ['0', '100'] })],
    };
    expect(emitScriptLua(state)).toBe('Math.clamp(x, 0, 100)');
  });

  it('emits call-function with no args', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'call-function', target: 'doStuff', value: '' })],
    };
    expect(emitScriptLua(state)).toBe('doStuff()');
  });

  // -- custom --------------------------------------------------------------

  it('emits custom from target', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'custom', target: 'print("debug")' })],
    };
    expect(emitScriptLua(state)).toBe('print("debug")');
  });

  it('emits custom from value when target is empty', () => {
    const state: SequencerScriptState = {
      steps: [step({ actionKind: 'custom', target: '', value: 'return 42' })],
    };
    // custom with empty target still passes the filter because actionKind === 'custom'
    expect(emitScriptLua(state)).toBe('return 42');
  });

  // -- multi-step ----------------------------------------------------------

  it('joins multiple steps with newlines', () => {
    const state: SequencerScriptState = {
      steps: [
        step({ id: '1', actionKind: 'set-variable', target: 'player.gold', value: '100' }),
        step({ id: '2', actionKind: 'emit-event', target: 'quest_complete', value: '' }),
      ],
    };
    expect(emitScriptLua(state)).toBe('Var["player.gold"] = 100\nEvent.emit("quest_complete")');
  });

  it('filters out blank-target non-custom steps', () => {
    const state: SequencerScriptState = {
      steps: [
        step({ id: '1', actionKind: 'set-variable', target: '  ', value: '1' }),
        step({ id: '2', actionKind: 'set-variable', target: 'x', value: '2' }),
      ],
    };
    expect(emitScriptLua(state)).toBe('Var["x"] = 2');
  });
});
