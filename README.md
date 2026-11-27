# PACE

> **Automated narrated video production from a script, images, and a voiceover.**

PACE turns a script, a set of images, and a narration recording into a finished, frame-accurate MP4 video (`16:9` or `9:16`). It segments scripts, aligns voiceover narration to exact scene timings, applies CapCut-style camera motion curves, effects, transitions, animated overlay stickers, background music ducking, and burned-in subtitles, previews in the browser via WebAssembly, and exports MP4s on a Rust + FFmpeg render pipeline.

---

## Key Engineering Highlights

- **Dual-Compiled Pure Engine (`pace-engine` + `pace-engine-wasm`)**: Pure mathematical frame timing, aspect-ratio fitting, CapCut-inspired camera motion curves (`Zoom 1`, `Zoom 2`, `Pendulum`, `Wobble`, `Fall Left/Right`), visual effects (`Camera Shake`, `Zoom Lens`, `Handheld`), transitions (`Shutter Flash`, directional slides/whips), and timeline-range stickers (`Subscribe`, `Like`, `Comment`, `Bell`, `Follow`) compiled both **natively** for server-side CPU rendering and to **WebAssembly** (`wasm-bindgen`) for 1:1 browser preview parity.
- **CPU Frame Rasterizer & Resumable Render Worker (`pace-render`)**: Rasterizes frames directly on CPU using `fast_image_resize` SIMD with segment caching and FFmpeg encoding + audio mixing/ducking (`sidechaincompress` / loudness normalization).
- **PostgreSQL-Backed Job Queue (`pace-jobs`)**: Uses PostgreSQL 16 `FOR UPDATE SKIP LOCKED` for reliable async audio alignment and video render jobs without a separate broker.
- **Python Audio Alignment Worker (`services/audio`)**: Audio validation, LUFS loudness normalization, Groq Whisper transcription (with local `faster-whisper` fallback), and **Needleman-Wunsch** sequence alignment to map spoken words back to script sections.
- **Direct-to-Object-Storage Media Pipeline (`pace-storage`)**: Pre-signed S3/MinIO uploads keep large audio, image, and exported MP4 payloads off the API server, plus `$0` server-side stock media search (Openverse / Pexels / Pixabay) and zero-key AI image generation (Pollinations + Stable Horde fallback).
- **Single-Source Shared Contracts (`xtask`)**: Data models defined once in Rust and code-generated into TypeScript (`packages/shared-types`) and Python (`services/audio/src/pace_audio/contracts.py`) via `cargo xtask gen`.
- **Desktop & Mobile CapCut-Style Studio (`apps/web`)**: Next.js + TypeScript + Tailwind CSS static-export web app featuring both a full desktop timeline workspace and a touch-friendly mobile studio.

---

## Architecture

```
apps/web (Next.js + TypeScript + pace-engine-wasm)
   │
   ├── Pre-signed Upload/Download ───────────────► S3 / MinIO (pace-storage)
   │                                                      ▲
   ▼ REST / OpenAPI                                       │
crates/pace-api (Axum + SQLx)                             │
   │                                                      │
   ▼ Jobs via PostgreSQL 16 (FOR UPDATE SKIP LOCKED)      │
   ├──► services/audio (Python: Whisper + Needleman-Wunsch)
   └──► crates/pace-render (Rust + pace-engine + FFmpeg) ─┘
```

---

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/pace-engine` | Pure timing, segmentation, motion, effects, transitions, stickers, subtitles |
| `crates/pace-engine-wasm` | `wasm-bindgen` bindings for browser preview |
| `crates/pace-api` | Axum HTTP API, auth/sessions/CSRF, OpenAPI spec, `create_admin` CLI |
| `crates/pace-db` | SQLx PostgreSQL models, repositories, and migrations |
| `crates/pace-jobs` | PostgreSQL `SKIP LOCKED` job queue & worker state machine |
| `crates/pace-render` | CPU frame rasterizer, segment cache, and FFmpeg MP4 muxer |
| `crates/pace-storage` | S3-compatible storage trait & MinIO/B2 client |
| `services/audio` | Python audio check, Whisper transcription & Needleman-Wunsch alignment |
| `apps/web` | Next.js + TypeScript web application (Desktop & Mobile Studio) |
| `packages/shared-types` | Generated TypeScript contracts from Rust |
| `xtask` | Contract code generator (`cargo run -p xtask -- gen`) |
| `infra/docker` | Local PostgreSQL 16 & MinIO Docker Compose setup |

---

## Quick Start (Local Docker)

```bash
# 1. Copy environment config
cp .env.example .env

# 2. Start PostgreSQL 16 and MinIO
docker compose -f infra/docker/compose.yml up -d

# 3. Run Rust workspace tests
cargo test --workspace

# 4. Run web typecheck & unit tests
pnpm --filter web typecheck
pnpm --filter web test

# 5. Start API, render worker, audio worker, and web dev server
cargo run -p pace-api --bin pace-api
cargo run -p pace-render --bin pace-render-worker
pnpm --filter web dev
```

---

## License

MIT — see [LICENSE](LICENSE).
