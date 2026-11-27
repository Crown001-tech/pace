# ADR-004: CapCut-Grade Camera Motion, Velocity Curves, Handheld Drift, and Motion Transitions

## Status
**Accepted** (2026-11-27)

## Context
Creator testing of PACE Studio (`M7`/`M8`) revealed that scene camera motion, camera shake, and boundary transitions felt static and noticeably weaker than modern editors such as CapCut, especially on punchy 2–6 second documentary and short-form (`9:16`) scenes:

1. **Zoom Choke (`TDD 12.2`)**: `MAX_ZOOM_SPEED_PER_SEC = 0.012` (`1.2%/sec`) capped a 3-second scene to `3.6%` total zoom (`1.00 -> 1.036`) and clamped `Subtle`, `Normal`, and `Strong` to the exact same delta on every scene shorter than 15 seconds.
2. **Fixed Pan Headroom & Edge Clamping (`TDD 12.2`, `12.4`)**: Pure pans hardcoded `PAN_ZOOM = 1.15` regardless of `Intensity`. Setting `Intensity::Strong` (`1.5x`) requested more pan travel than `PAN_ZOOM` provided, causing `calculate_fill_crop` to clamp against image edges and freeze camera movement mid-scene. Combo presets (`ZoomInPan*`) started at `zoom = 1.0`, leaving zero pan room at the start of the shot.
3. **Missing Fast-Out / Whip Velocity Curves (`EFX-03`)**: Only `Steady` (linear) and `EaseInOut` (smoothstep) existed, lacking the exponential burst (`PunchOut`) and cut-acceleration (`RampIn`) curves that give CapCut edits visual energy.
4. **Progress-Normalized Shake Bug & Single-Sine Wobble (`EFX-07`)**: `cam_state_at` evaluated `2π * frequency_hz * progress` (`progress ∈ [0, 1]`) rather than elapsed seconds (`progress * duration_sec`), stretching `2.5 Hz` across an entire 20s scene (`0.125 Hz`) at `0.003` amplitude.
5. **Alpha-Only Transitions & Preview Brightness Dip (`EFX-06`, `PRV-02`)**: Only `Cut`, `Crossfade`, and `FadeBlack` were supported, and `Crossfade` emitted `out_opacity = 1 - u` + `in_opacity = u`, which under Canvas 2D Porter-Duff `source-over` dimmed mid-transition frames by up to `25%`.

## Decision
We upgrade the dual-compiled `pace-engine` (and downstream `pace-render`, `pace-engine-wasm`, `pace-api`, and `apps/web`) while preserving 100% determinism, zero I/O in `pace-engine`, and full backward compatibility with stored JSON records:

1. **Expressive Zoom & Intensity-Scaled Pan Headroom (`crates/pace-engine/src/motion.rs`)**:
   - Raise `BASE_ZOOM_TARGET` to `1.24` and `MAX_ZOOM_SPEED_PER_SEC` to `0.14` (`14%/sec`), ensuring `Subtle (0.6x)`, `Normal (1.0x)`, and `Strong (1.55x)` produce distinct, visible motion even on 2–4s scenes while keeping ultra-short (`0.5s`) cuts stable.
   - Scale pan zoom headroom dynamically with `Intensity` (`pan_zoom = 1.0 + 0.18 * mult`) and start combo zoom-pans above `1.08x` so `Strong` pans travel farther without ever hitting edge-clamp walls.
2. **20 Motion Presets (`EFX-01`)**:
   - Retain all 15 existing presets and add 5 CapCut-style presets: `PunchZoomIn`, `PunchZoomOut`, `KenBurnsDiagonalIn`, `KenBurnsDiagonalOut`, and `HandheldDrift`.
3. **5 Velocity Easing Curves (`EFX-03`)**:
   - Retain `EaseInOut` and `Steady`, and add `PunchOut` (fast exponential burst then slow glide), `RampIn` (cubic acceleration into the cut), and `DynamicCurve` (steep mid-scene S-whip).
4. **Real-Time Multi-Harmonic Handheld Shake (`EFX-07`)**:
   - Store `duration_ms` on `Motion` (`#[serde(default)]`) so `cam_state_at` computes real elapsed seconds and layers primary handheld sway, secondary micro-tremor, and subtle zoom breathing within safe crop margins.
5. **Motion Transitions & True Source-Over Compositing (`EFX-06`, `PRV-02`)**:
   - Add `ZoomThrough`, `WhipPanLeft`, `WhipPanRight`, and `SlideUp` to `TransitionKind`.
   - Use true Porter-Duff `source-over` layer opacities (`out = 1.0`, `in = eased_u`) and per-layer clipped compositing in `pace-render::raster` so browser Canvas 2D preview and Rust CPU MP4 exports remain pixel-aligned without mid-crossfade darkening.

## Documents Requiring Update
- `docs/product/PACE_PRD.docx`: Update `EFX-01` (20 motion presets), `EFX-03` (5 easing curves & updated intensity scaling), `EFX-06` (7 transition kinds including motion transitions), and `EFX-07` (organic handheld drift).
- `docs/tdd/PACE_Technical_Design_Document.docx`: Update Section `12.2` (zoom/pan constants, diagonal/punch/handheld formulas, elapsed-time shake), Section `12.5` (motion transition geometry & `source-over` opacities), and Section `13.2` (per-layer clipped compositing in `raster.rs`).
