//! WebAssembly bindings for `pace-engine` (`M7-2`).
//!
//! Exposes `WasmRenderEngine` to the browser so frame plans (`FramePlan`)
//! are evaluated directly by Rust in real time and returned as flat `Float32Array`
//! typed buffers without per-frame JSON serialization.

use pace_engine::spec::{
    compile_spec, encode_frame_plan_into, frame_plan, CompileSpecInput, RenderSpec,
};
use pace_engine::stickers::{encode_stickers_flat_into, evaluate_stickers_at};
use wasm_bindgen::prelude::*;

/// Stateful WASM render evaluator holding a compiled `RenderSpec` in linear memory.
#[wasm_bindgen]
pub struct WasmRenderEngine {
    spec: RenderSpec,
    flat_scratch: Vec<f32>,
}

#[wasm_bindgen]
impl WasmRenderEngine {
    /// Compiles a `CompileSpecInput` JSON payload into a frozen `RenderSpec`.
    #[wasm_bindgen(constructor)]
    pub fn new(input_json: &str) -> Result<WasmRenderEngine, String> {
        let input: CompileSpecInput =
            serde_json::from_str(input_json).map_err(|e| format!("invalid compile input: {e}"))?;
        let spec = compile_spec(&input).map_err(|e| format!("compile_spec failed: {e}"))?;
        Ok(Self {
            spec,
            flat_scratch: Vec::with_capacity(64),
        })
    }

    /// Loads an already-compiled `RenderSpec` JSON payload (e.g. from `/api/v1/projects/{id}/render-spec`).
    #[wasm_bindgen]
    pub fn from_compiled_spec(spec_json: &str) -> Result<WasmRenderEngine, String> {
        let spec: RenderSpec =
            serde_json::from_str(spec_json).map_err(|e| format!("invalid render spec: {e}"))?;
        Ok(Self {
            spec,
            flat_scratch: Vec::with_capacity(64),
        })
    }

    /// Canonical SHA-256 hash of the frozen `RenderSpec`.
    #[wasm_bindgen(getter)]
    pub fn spec_hash(&self) -> String {
        self.spec.spec_hash.clone()
    }

    /// Total frames at `fps`.
    #[wasm_bindgen(getter)]
    pub fn total_frames(&self) -> u32 {
        self.spec.total_frames
    }

    /// Target frame rate (`fps`).
    #[wasm_bindgen(getter)]
    pub fn fps(&self) -> u32 {
        self.spec.fps
    }

    /// Output frame width in pixels.
    #[wasm_bindgen(getter)]
    pub fn frame_width(&self) -> f32 {
        self.spec.frame_size.w
    }

    /// Output frame height in pixels.
    #[wasm_bindgen(getter)]
    pub fn frame_height(&self) -> f32 {
        self.spec.frame_size.h
    }

    /// Returns the compiled `RenderSpec` serialized as JSON.
    #[wasm_bindgen]
    pub fn compiled_spec_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.spec).map_err(|e| format!("serialize spec failed: {e}"))
    }

    /// Evaluates frame `frame_index` and returns a flat `Float32Array`:
    /// `[fade_black_alpha, primary_scene_index, layer_count, flash_white_alpha, ...layers(stride=12)]`.
    #[wasm_bindgen]
    pub fn frame_plan_flat(&mut self, frame_index: u32) -> Vec<f32> {
        let plan = frame_plan(&self.spec, frame_index);
        encode_frame_plan_into(&plan, &mut self.flat_scratch);
        self.flat_scratch.clone()
    }

    /// Evaluates active stickers at `frame_index` and returns `[count, ...stickers(stride=6)]`.
    #[wasm_bindgen]
    pub fn sticker_plan_flat(&mut self, frame_index: u32) -> Vec<f32> {
        let active = evaluate_stickers_at(&self.spec.stickers, frame_index, self.spec.fps);
        encode_stickers_flat_into(&active, &mut self.flat_scratch);
        self.flat_scratch.clone()
    }
}

/// Stateless helper compiling `CompileSpecInput` JSON and returning `RenderSpec` JSON.
#[wasm_bindgen]
pub fn compile_spec_json(input_json: &str) -> Result<String, String> {
    let input: CompileSpecInput =
        serde_json::from_str(input_json).map_err(|e| format!("invalid compile input: {e}"))?;
    let spec = compile_spec(&input).map_err(|e| format!("compile_spec failed: {e}"))?;
    serde_json::to_string(&spec).map_err(|e| format!("serialize spec failed: {e}"))
}
