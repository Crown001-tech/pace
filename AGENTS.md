# AGENTS.md — PACE Engineering & Collaboration Guide

This document is the permanent source of truth for how we build PACE. Every agent, assistant, and pair-programming session must adhere strictly to these principles, constraints, and workflows.

---

## The Project

PACE turns a script, a set of images, and one narration recording into a finished, synchronised MP4 video (20–40 minutes, 16:9 or 9:16).
It splits the script into sections, assigns one image per section, aligns narration to the script to time each scene, adds camera motion, transitions, optional music and subtitles, previews in the browser, and exports.

---

## Source of Truth

Four core design documents define PACE:
1. `docs/product/PACE_PRD.docx`: **WHAT** to build (requirements have IDs like `ALN-03`).
2. `docs/tdd/PACE_Technical_Design_Document.docx`: **HOW** it is built.
3. `docs/product/PACE_File_Structure.docx`: **WHERE** code goes.
4. `docs/product/PACE_Milestones.docx`: the **ORDER** we build and learn in.

### Conflict Resolution Hierarchy
If documents conflict:
- **PRD** wins on *what*.
- **TDD** wins on *how*.
- **File Structure** wins on *where*.
- **Milestones** wins on *order*.

---

## Fixed Decisions (Do NOT change without explicit user consent)

1. **Rust Core**: API (`axum`, `sqlx`), render worker, and a pure shared engine crate (`pace-engine`).
2. **Dual-Compiled Engine**: `pace-engine` contains pure mathematical and visual logic (zero file, network, clock, or database I/O) and is compiled twice:
   - Natively for the API and render worker.
   - To WebAssembly (`wasm-bindgen`, `pace-engine-wasm`) for in-browser preview, guaranteeing preview and export frame parity.
3. **Rust Draws Every Frame**: Rust rasterizes every video frame directly on the CPU (using SIMD via `fast_image_resize`). FFmpeg only encodes frames and mixes/ducks audio.
4. **Python Audio Worker**: One small Python service (`services/audio`) dedicated exclusively to audio checks, loudness normalization, Groq Whisper transcription (with local `faster-whisper` fallback), and Needleman-Wunsch script alignment.
5. **Web App**: Next.js + TypeScript built as a **static export** (`output: 'export'`), hosted on Cloudflare Pages in production.
6. **PostgreSQL Dual Role**: PostgreSQL 16 stores all structured application data **and** acts as the job queue (`jobs` table with `FOR UPDATE SKIP LOCKED`).
7. **Object Storage**: S3-compatible storage abstracted behind a `Storage` trait. MinIO locally in Docker; Backblaze B2 online. Large media files **never** pass through the API—the browser uploads straight to storage using pre-signed URLs.
8. **Shared Data Contracts**: Data shapes are written once in Rust. TypeScript types (`packages/shared-types`) and Python models (`services/audio/src/pace_audio/contracts.py`) are strictly generated via `cargo xtask gen`.
9. **Local-First & $0 Cost**: Everything runs locally on the laptop in Docker at $0. No paid service, account, or credit card until Milestone M11 (Go online).

---

## Setup & Hardware Constraints

- **Host**: Windows 11 laptop, Intel Core i7-8550U (4 cores, 8 threads, 15W TDP), 8 GB RAM.
- **Environment**: WSL2 Ubuntu, Docker Desktop (with WSL2 backend & Ubuntu integration), VS Code with the WSL extension.
- **Workspace Location**: Code lives inside Ubuntu at `~/code/pace` (avoiding the slow `/mnt/c/` 9P mount).
- **Resource Constraints**:
  - Keep memory use low: `.wslconfig` sets memory to 5 GB and swap to 4 GB.
  - Rust builds use `CARGO_BUILD_JOBS=4` and the `mold` linker to avoid RAM exhaustion.
  - Local tests use short clips (1–5 minutes) and 720p/360p resolution. Never assume a powerful workstation.

---

## How We Work

1. **One milestone at a time**: Follow the exact sequence in `PACE_Milestones.docx`. Never jump ahead.
2. **Learn first**: Every milestone begins with its "Learn first" items. Explain each concept in plain words with a tiny example before writing code.
3. **Test-first (TDD)**: Write the failing test first, run it to show it fail, then write the minimal code to pass it.
4. **Small, verified steps**: After each step, provide exact commands for the user to run and verify themselves.
5. **Teacher and pair programmer**: Guide the user to write code themselves (especially Rust). When providing code, always provide a beginner-friendly breakdown of what was done and why.
6. **Plain-English error explanations**: When an error occurs, explain what it means in plain words before suggesting or applying a fix.
7. **Complete files only**: Always supply complete, ready-to-drop-in files. Never diffs, partial snippets, or placeholders.
8. **Justify all dependencies**: Before introducing any new dependency, explain what it is, why it is needed, and the lighter alternative if one exists.
9. **Architectural Decision Records (ADRs)**: If something in the specifications needs to change, write an ADR in `docs/adr/` and specify which documents require updating.
10. **Session wrap-up checklist**: Every session must conclude with:
    - What was finished.
    - What is next.
    - A suggested Git commit message.
    - Exactly 3 lines for user learning notes (`learning/mNN-*.md`).

