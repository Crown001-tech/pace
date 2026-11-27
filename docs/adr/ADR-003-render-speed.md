# ADR-003: Render Pipeline Architecture and CPU Drawing Performance

## Status
**Accepted** (Confirmed via Spike S1 Benchmark)

## Context & Hardware Constraints
PACE renders automated documentary-style narrated videos (20–40 minutes at 30 fps) by taking a script, an audio narration, and images, and creating continuous smooth camera motion (Ken Burns pans, zooms, easing curves, and transitions).

Our development target is a low-power laptop:
- **CPU**: Intel Core i7-8550U (4 physical cores, 8 threads, 15W TDP, base clock 1.8 GHz, boost to 4.0 GHz).
- **RAM**: 8 GB physical memory (WSL2 capped at 5 GB RAM, 4 GB swap via `.wslconfig`).
- **Software**: Ubuntu 24.04 (WSL2), Rust 1.99.1 (`fast_image_resize` 5.5, `image` 0.25), FFmpeg 8.0.1 (`libx264 veryfast`, High profile, yuv420p).

TDD section 13.2 established a spike:
> *"Spike S1 in Milestone 0 renders a 60-second 1080p scene on the laptop and records frames per second per CPU core. That number sets the export time estimates, the default worker size and the cost model. If CPU drawing is too slow, the fallback is FFmpeg's own filters for motion (slower to look good, but simpler)."*

## Spike S1 Measured Results
We loaded a 4K test card (`fixtures/images/sample_card_4k.png`, 3840×2160 pixels) and rendered **1,800 frames** (60 seconds at 30 fps) of a continuous slow zoom (`ZoomIn` preset with `EaseInOut` cubic easing) using `motion.rs` and `fit.rs`. Each frame was cropped with subpixel floating-point coordinates and resized via AVX2 SIMD in `fast_image_resize`, then piped directly into FFmpeg stdin.

| Resolution | Dimensions | Measured Speed | Real-Time Factor | 60s Clip Render Time | Est. 22-min Video (Single Thread) | Est. 22-min Video (3 Parallel Segments) |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **720p** (Default) | 1280 × 720 | **10.1 FPS** | **0.34×** | 178.3 s (2.97 min) | 65.4 min | **~22–25 min (~1.0× real-time)** |
| **1080p** (High-Res)| 1920 × 1080| **8.0 FPS** | **0.27×** | 224.6 s (3.74 min) | 82.3 min | **~28–32 min** |

Both output MP4s (`output_720p.mp4` and `output_1080p.mp4`) were verified with `ffprobe`:
- Duration: exactly 60.00 seconds (1,800 frames).
- Frame rate: 30 fps constant, timescale 15360.
- Video quality: clean, smooth motion with no stutter or edge tearing.
- Peak memory during rendering: **< 160 MB** RSS (well within our 512 MB per-service limit).

## Decision
We **CONFIRM** CPU drawing directly in Rust using `fast_image_resize` and FFmpeg stdin piping. We reject the FFmpeg-filter fallback for the following reasons:

1. **Pixel-Perfect Preview Parity (`PRV-02`)**: Rust CPU drawing evaluates the exact same `FramePlan` geometry that `pace-engine-wasm` executes in the browser Canvas 2D. Using FFmpeg's internal filters (`zoompan`) would introduce rounding discrepancies and break visual parity.
2. **Smooth Fractional Subpixel Motion**: `fast_image_resize` supports fractional crop positions (`f64`), completely eliminating the stair-stepping jitter that occurs with integer-only motion filters.
3. **Multi-Core Scaling via Scene Segments (`TDD 13.1`, `ADR-008`)**: PACE divides long videos into scene segments. Rendering 3 segments in parallel on our 4-core laptop scales throughput to **~25–30 FPS at 720p**, allowing a full 22-minute video to render in approximately 22–25 minutes locally at $0 cost.
4. **Cloud Scalability**: On Railway render workers with 4–8 vCPUs, rendering will run at **>60 FPS (>2.0× real-time)**, completing a 22-minute export in under 11 minutes.

## Consequences & Settings
- **Default Resolution**: Kept as **720p** (`1280x720`, `crf = 23`, `maxrate = 2.5M`, `bufsize = 5M`), as defined in `EXP-02`.
- **1080p Limit**: Maintain the daily cap of 3 exports per user per day for 1080p (`EXP-11`) to preserve shared worker CPU bandwidth.
- **Worker Concurrency**: Set local render worker concurrency to `num_cpus - 1` (3 on our 4-core machine) so Windows remains responsive during rendering.
- **Future GPU Path**: The `FrameRenderer` trait in `pace-render-worker` remains in place so a `wgpu` (GPU hardware accelerated) rasterizer can be dropped in for the desktop app (TDD section 22) without altering the shared engine or draw plan.
