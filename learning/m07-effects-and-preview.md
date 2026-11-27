# Milestone 7 Learning Notes: Effects and Browser Preview

1. Loading a compiled `RenderSpec` into `WasmRenderEngine` and reading flat `Float32Array` buffers per frame avoids per-frame JSON overhead while guaranteeing bit-identical motion and crop geometry between browser preview and native export.
2. Drawing `fit_blur` backgrounds on Canvas 2D with `ctx.clip()` and a 28px bleed around `ctx.filter = "blur(20px)"` eliminates transparent dark-edge artifacts at frame borders.
3. Optimistic local WASM recompilation paired with backend `PATCH /effect` and `POST /effects/reroll` keeps interactive scrubbing, focal-point picking, and transitions instantaneous while preserving manual overrides across reloads.
