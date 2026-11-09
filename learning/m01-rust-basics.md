# Milestone 1: The Engine Core & First Real Rust

## What We Built
In Milestone 1, we implemented the pure mathematical and visual core of PACE in `crates/pace-engine`:
1. **Script Segmentation (`segmentation.rs` — `SCR-01`, `SCR-02`, `SCR-09`)**:
   - Splits raw text into paragraphs at blank/whitespace-only lines.
   - Enforces the 60,000 Unicode character limit.
   - Calculates word counts and estimated spoken durations at 150 words per minute.
2. **Timeline Timing & Zero Drift (`timing.rs` — `TML-05..07`, `EFX-06`, TDD 12.1)**:
   - Converts boundary times to frames independently: `round(time_ms * 30 / 1000)`.
   - Enforces $0.5\text{s}$ minimum scene lengths and $6$ body frames for transitions.
   - Proven mathematically by `proptest` across 300 random scenes over 40 minutes.
3. **Camera Motion Model (`motion.rs` — `EFX-01`, `EFX-03`, `EFX-07`, TDD 12.2)**:
   - `CamState { cx, cy, zoom }` in normalized coordinates.
   - 15 motion presets (`Still`, `ZoomIn`, `ZoomOut`, `PanLeft/Right/Up/Down`, and combinations).
   - $0.012/\text{s}$ zoom speed cap: limits short scene zoom violence while never stopping in long scenes.
   - Cubic smoothstep easing ($3t^2 - 2t^3$) and harmonic camera shake.
4. **Deterministic Variety (`effects.rs` — `EFX-02`, `EFX-08`, TDD 12.3)**:
   - Seeded with `rand_pcg::Pcg64`.
   - Never repeats one of the previous 2 presets.
   - No preset exceeds $35\%$ of total scenes.
   - Biases pans towards available image room beyond the focal point.
   - $1/8$ camera shake probability, strictly excluded on adjacent scenes.
   - Preserves manual creator overrides 100%.
5. **Fit Modes & Geometry (`fit.rs` — `EFX-04`, `EFX-05`, TDD 12.4)**:
   - `Fill`: Subpixel fractional source crop clamped strictly within image boundaries.
   - `FitBlur`: Static blurred background + uncropped moving foreground image.
   - Verified via property testing that crop rectangles never leave image boundaries for any zoom and focal point.
6. **Spike S1 Render Speed Benchmark (`learning/exercises/render-spike`)**:
   - Rendered 1,800 frames (60 seconds at 30 fps) from a 4K source image at both 720p and 1080p.
   - Achieved **10.1 FPS at 720p** on a single thread of an Intel Core i7-8550U laptop CPU.
   - Confirmed **CPU Drawing in Rust (`ADR-003`)** as our primary architecture.

---

## 3 Core Takeaways for Notes
1. Converting each boundary timestamp to a frame number independently (`round(time_ms * 30 / 1000)`) eliminates cumulative rounding error, keeping audio and video synchronized within half a frame across 40 minutes (`ADR-009`).
2. Rust's ownership model lets us write pure calculation crates (`pace-engine`) with zero heap churn and zero I/O that compile identically to native machine code and in-browser WebAssembly (`PRV-02`).
3. SIMD image resizing (`fast_image_resize`) with subpixel crop coordinates piped into FFmpeg stdin produces butter-smooth Ken Burns motion on a 15W laptop CPU without needing external GPU dependencies.
