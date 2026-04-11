/* tslint:disable */
/* eslint-disable */

/**
 * For every statement in the source, return the 1-based line where it
 * *starts*. Used by luau-debugger to inject `__prism_trace(n)` without
 * breaking multi-line strings or statements.
 */
export function findStatementLines(source: string): Uint32Array;

/**
 * Extract every `ui.<kind>(...)` call from the source, with literal args.
 */
export function findUiCalls(source: string): any;

/**
 * Parse Luau source into a Unist-compatible RootNode.
 */
export function parse(source: string): any;

/**
 * Lightweight parse-only diagnostics. Empty array on success.
 */
export function validate(source: string): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly findStatementLines: (a: number, b: number) => [number, number, number, number];
    readonly findUiCalls: (a: number, b: number) => [number, number, number];
    readonly parse: (a: number, b: number) => [number, number, number];
    readonly validate: (a: number, b: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
