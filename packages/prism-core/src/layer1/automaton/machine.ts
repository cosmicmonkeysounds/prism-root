/**
 * Machine — flat finite state machine.
 *
 * Context-free: the machine holds no data. State is a plain string.
 * Observable: subscribe to state changes with on().
 * Serializable: toJSON() for persistence.
 *
 * Wildcard from: use '*' to match any current state.
 * Guards: transitions can be blocked by a guard predicate.
 * Actions: run during the transition (after exit, before enter).
 */

export interface StateNode<TState extends string> {
  id: TState;
  onEnter?: () => void;
  onExit?: () => void;
  terminal?: boolean;
}

export interface Transition<TState extends string, TEvent extends string> {
  from: TState | TState[] | "*";
  event: TEvent;
  to: TState;
  guard?: () => boolean;
  action?: () => void;
}

export interface MachineDefinition<
  TState extends string,
  TEvent extends string,
> {
  initial: TState;
  states: StateNode<TState>[];
  transitions: Transition<TState, TEvent>[];
}

export type MachineListener<
  TState extends string,
  TEvent extends string,
> = (state: TState, event: TEvent) => void;

export class Machine<TState extends string, TEvent extends string> {
  private _state: TState;
  private readonly _listeners = new Set<MachineListener<TState, TEvent>>();
  private readonly _nodeMap: Map<TState, StateNode<TState>>;
  private readonly _transByEvent: Map<TEvent, Transition<TState, TEvent>[]>;

  constructor(
    private readonly def: MachineDefinition<TState, TEvent>,
    options: { initial?: TState | undefined; skipInitialEnter?: boolean | undefined } = {},
  ) {
    this._state = options.initial ?? def.initial;

    this._nodeMap = new Map(def.states.map((s) => [s.id, s]));
    this._transByEvent = new Map();
    for (const t of def.transitions) {
      if (!this._transByEvent.has(t.event))
        this._transByEvent.set(t.event, []);
      this._transByEvent.get(t.event)!.push(t);
    }

    if (!options.skipInitialEnter) {
      this._nodeMap.get(this._state)?.onEnter?.();
    }
  }

  get state(): TState {
    return this._state;
  }

  matches(state: TState | TState[]): boolean {
    return Array.isArray(state)
      ? state.includes(this._state)
      : this._state === state;
  }

  can(event: TEvent): boolean {
    const node = this._nodeMap.get(this._state);
    if (node?.terminal) return false;
    const t = this._findTransition(event);
    if (!t) return false;
    return !t.guard || t.guard();
  }

  send(event: TEvent): boolean {
    const node = this._nodeMap.get(this._state);
    if (node?.terminal) return false;

    const t = this._findTransition(event);
    if (!t) return false;
    if (t.guard && !t.guard()) return false;

    this._nodeMap.get(this._state)?.onExit?.();
    t.action?.();
    this._state = t.to;
    this._nodeMap.get(t.to)?.onEnter?.();

    for (const l of this._listeners) l(this._state, event);
    return true;
  }

  on(listener: MachineListener<TState, TEvent>): () => void {
    this._listeners.add(listener);
    return () => this._listeners.delete(listener);
  }

  toJSON(): { state: TState } {
    return { state: this._state };
  }

  private _findTransition(
    event: TEvent,
  ): Transition<TState, TEvent> | undefined {
    const candidates = this._transByEvent.get(event);
    if (!candidates) return undefined;
    for (const t of candidates) {
      if (t.from === "*") return t;
      if (
        Array.isArray(t.from)
          ? t.from.includes(this._state)
          : t.from === this._state
      )
        return t;
    }
    return undefined;
  }
}

export function createMachine<
  TState extends string,
  TEvent extends string,
>(def: MachineDefinition<TState, TEvent>) {
  return {
    start(initial?: TState): Machine<TState, TEvent> {
      return new Machine<TState, TEvent>(def, initial ? { initial } : {});
    },
    restore(state: TState): Machine<TState, TEvent> {
      return new Machine(def, { initial: state, skipInitialEnter: true });
    },
  };
}
