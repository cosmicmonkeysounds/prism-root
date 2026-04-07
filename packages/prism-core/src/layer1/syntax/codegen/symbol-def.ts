/**
 * SymbolDef -- the unified "describe once, emit many targets" codegen DSL.
 *
 * A SymbolDef describes one exported symbol (function, constant, class, enum,
 * namespace). The same definition is fed to multiple emitters to produce:
 *   - TypeScript types/constants (.ts)
 *   - C# classes/enums (.cs)
 *   - Lua EmmyDoc stubs (.d.lua)
 *   - GDScript stubs (.gd) -- via a pluggable emitter
 *
 * This replaces the pattern of writing three separate emitters per package.
 * Package authors describe their exported symbols once; the pipeline handles
 * all target languages.
 *
 * Usage:
 *   const symbols: SymbolDef[] = [
 *     {
 *       name: 'CONVERSATIONS',
 *       kind: 'namespace',
 *       description: 'Compiled conversation IDs',
 *       children: [
 *         { name: 'TAVERN_INTRO', kind: 'constant', type: 'string', value: 'tavern_intro' },
 *         { name: 'GUARD_PATROL', kind: 'constant', type: 'string', value: 'guard_patrol' },
 *       ],
 *     },
 *     {
 *       name: 'StartConversation',
 *       kind: 'function',
 *       params: [{ name: 'id', type: 'string' }, { name: 'actorId', type: 'string', optional: true }],
 *       returns: 'void',
 *       description: 'Start a conversation by ID.',
 *     },
 *   ];
 *
 *   const pipeline = new CodegenPipeline()
 *     .register(new SymbolTypeScriptEmitter({ moduleName: 'MyModule' }))
 *     .register(new SymbolCSharpEmitter({ namespace: 'Prism.MyModule' }))
 *     .register(new SymbolEmmyDocEmitter({ globalName: 'MyModule' }));
 *
 *   const result = pipeline.run(symbols, { projectName: 'MyProject' });
 */

// -- Symbol kinds -----------------------------------------------------------

/**
 * The kind of symbol being declared.
 *   'constant'  -- an immutable scalar value (string, number, boolean)
 *   'function'  -- a callable
 *   'class'     -- a type / class declaration
 *   'enum'      -- an enumeration of string or number values
 *   'namespace' -- a grouping container (becomes a table in Lua, namespace in C#, const object in TS)
 *   'field'     -- a field on a class or namespace (used inside 'class' or 'namespace' children)
 */
export type SymbolKind = 'constant' | 'function' | 'class' | 'enum' | 'namespace' | 'field';

// -- Parameter --------------------------------------------------------------

export interface SymbolParam {
  /** Parameter name. */
  name: string;
  /**
   * Type string. Language-agnostic:
   *   'string', 'number', 'boolean', 'void'
   *   'string[]', 'number[]'
   *   Custom class names (matched by name in emitters)
   */
  type: string;
  optional?: boolean;
  description?: string;
  /** Default value (for TS optional params). */
  defaultValue?: unknown;
}

// -- The symbol definition --------------------------------------------------

export interface SymbolDef {
  /** Symbol name. For namespaced children, this is the short name (no dots). */
  name: string;
  kind: SymbolKind;
  /** Human-readable description (emitted as doc comment). */
  description?: string;

  // -- Constant / field -----------------------------------------------------
  /**
   * Value for 'constant' kind. Used to emit the actual value in TS/CS/Lua.
   * e.g. 'tavern_intro' for a conversation ID constant.
   */
  value?: string | number | boolean;
  /**
   * Type annotation for constants and fields.
   * e.g. 'string', 'number', 'ConversationId'
   */
  type?: string;

  // -- Function -------------------------------------------------------------
  params?: SymbolParam[];
  returns?: string;
  /** Whether this function is async (affects TS emit). */
  async?: boolean;

  // -- Enum -----------------------------------------------------------------
  /**
   * For 'enum' kind: the enumeration values.
   * Each entry: { name: 'ACTIVE', value: 'active', description?: '...' }
   */
  enumValues?: Array<{ name: string; value: string | number; description?: string }>;
  /** Whether enum values are string literals (true) or numbers (false). Default: true. */
  enumIsString?: boolean;

  // -- Namespace / class children -------------------------------------------
  /** Child symbols (for 'namespace' and 'class' kinds). */
  children?: SymbolDef[];

  // -- Metadata -------------------------------------------------------------
  deprecated?: string;
  /** Source package that owns this symbol (e.g. 'mypackage'). */
  provider?: string;
}

// -- Helpers ----------------------------------------------------------------

/** Build a namespace of string constants from a flat record. Useful for ID banks. */
export function constantNamespace(
  name: string,
  values: Record<string, string>,
  description?: string,
): SymbolDef {
  return {
    name,
    kind: 'namespace',
    ...(description !== undefined && { description }),
    children: Object.entries(values).map(([k, v]) => ({
      name: k,
      kind: 'constant' as const,
      type: 'string',
      value: v,
    })),
  };
}

/** Build a function SymbolDef. */
export function fnSymbol(
  name: string,
  params: SymbolParam[],
  returns: string,
  description?: string,
): SymbolDef {
  return {
    name,
    kind: 'function',
    params,
    returns,
    ...(description !== undefined && { description }),
  };
}
