# PACE

> Automated narrated video production from a script, images, and a voiceover.

PACE turns a script, a folder of images, and one narration recording into a finished, synchronised MP4 video (20–40 minutes, 16:9 or 9:16). It automatically splits sections, aligns narration to words, computes frame-accurate camera motions, and renders on CPU with FFmpeg encoding.

---

## Architecture Overview

- **`crates/`**: Pure Rust shared calculation engine (`pace-engine`), Web API (`pace-api`), render worker (`pace-render-worker`), DB migrations (`pace-db`), and storage abstractions (`pace-storage`).
- **`services/audio`**: Lightweight Python service for audio checks, Groq Whisper transcription, and Needleman-Wunsch script alignment.
- **`apps/web`**: Next.js + TypeScript static export web app with in-browser WebAssembly preview.
- **`infra/docker`**: Local Docker Compose environment running PostgreSQL 16 and MinIO.

---

## Getting Started in 5 Commands

```bash
# 1. Clone the repository
git clone <repo-url> && cd pace

# 2. Copy the example environment file
cp .env.example .env

# 3. Start local database and object storage
make up

# 4. Run Rust engine tests
make test

# 5. Stop containers when done
make down
```

---

## Documentation

Full architectural and product specifications live in `docs/`:
- `docs/product/PACE_PRD.docx`: What to build (Acceptance criteria)
- `docs/tdd/PACE_Technical_Design_Document.docx`: How it is built
- `docs/product/PACE_File_Structure.docx`: Where code lives
- `docs/product/PACE_Milestones.docx`: Build order and learning tracks
- `AGENTS.md`: Permanent engineering and pair-programming guidelines
