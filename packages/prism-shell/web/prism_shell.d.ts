/* tslint:disable */
/* eslint-disable */

/**
 * Browser entry point. `wasm-bindgen` calls this automatically via
 * its `(start)` attribute so the HTML loader only has to import the
 * generated JS module and invoke `init()`.
 */
export function web_start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly web_start: () => void;
    readonly send_keyboard_string_sequence: (a: number, b: number) => void;
    readonly slint_get_mocked_time: () => bigint;
    readonly slint_mock_elapsed_time: (a: bigint) => void;
    readonly slint_send_keyboard_char: (a: number, b: number, c: number) => void;
    readonly slint_send_mouse_click: (a: number, b: number, c: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hfca8740585b6d5ae: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h360fad65593b1f58: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h3a043e914441431d: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hbf91394d281d4e59: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hd022baa82c58ffd3: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h1dbacae8f78e0bd4: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h08dc423846b10247: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hf2098a5f041d2f65: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h972f317ce545aa36: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hc73f65c542540294: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hc51c0d76bb8a9e76: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__heb4e3e2eb24e9216: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hdb04c4ff60acb754: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h04213d3fbb2eb503: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h33bac1d243d273a4: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hd0d7b8fe88be662c: (a: number, b: number) => number;
    readonly wasm_bindgen__convert__closures_____invoke__hd714cb7e9ee5d7ba: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
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
