/* tslint:disable */
/* eslint-disable */

/**
 * Stateful WASM render evaluator holding a compiled `RenderSpec` in linear memory.
 */
export class WasmRenderEngine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Returns the compiled `RenderSpec` serialized as JSON.
     */
    compiled_spec_json(): string;
    /**
     * Evaluates frame `frame_index` and returns a flat `Float32Array`:
     * `[fade_black_alpha, primary_scene_index, layer_count, flash_white_alpha, ...layers(stride=12)]`.
     */
    frame_plan_flat(frame_index: number): Float32Array;
    /**
     * Loads an already-compiled `RenderSpec` JSON payload (e.g. from `/api/v1/projects/{id}/render-spec`).
     */
    static from_compiled_spec(spec_json: string): WasmRenderEngine;
    /**
     * Compiles a `CompileSpecInput` JSON payload into a frozen `RenderSpec`.
     */
    constructor(input_json: string);
    /**
     * Evaluates active stickers at `frame_index` and returns `[count, ...stickers(stride=6)]`.
     */
    sticker_plan_flat(frame_index: number): Float32Array;
    /**
     * Target frame rate (`fps`).
     */
    readonly fps: number;
    /**
     * Output frame height in pixels.
     */
    readonly frame_height: number;
    /**
     * Output frame width in pixels.
     */
    readonly frame_width: number;
    /**
     * Canonical SHA-256 hash of the frozen `RenderSpec`.
     */
    readonly spec_hash: string;
    /**
     * Total frames at `fps`.
     */
    readonly total_frames: number;
}

/**
 * Stateless helper compiling `CompileSpecInput` JSON and returning `RenderSpec` JSON.
 */
export function compile_spec_json(input_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmrenderengine_free: (a: number, b: number) => void;
    readonly compile_spec_json: (a: number, b: number) => [number, number, number, number];
    readonly wasmrenderengine_compiled_spec_json: (a: number) => [number, number, number, number];
    readonly wasmrenderengine_fps: (a: number) => number;
    readonly wasmrenderengine_frame_height: (a: number) => number;
    readonly wasmrenderengine_frame_plan_flat: (a: number, b: number) => [number, number];
    readonly wasmrenderengine_frame_width: (a: number) => number;
    readonly wasmrenderengine_from_compiled_spec: (a: number, b: number) => [number, number, number];
    readonly wasmrenderengine_new: (a: number, b: number) => [number, number, number];
    readonly wasmrenderengine_spec_hash: (a: number) => [number, number];
    readonly wasmrenderengine_sticker_plan_flat: (a: number, b: number) => [number, number];
    readonly wasmrenderengine_total_frames: (a: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
