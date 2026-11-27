# PACE Python Audio Worker (`services/audio`)

Processes narration uploads from the `audio` job queue in PostgreSQL (`jobs` table with `FOR UPDATE SKIP LOCKED`):

1. **Prepare (`audio_ops.py`)**: Validates stream integrity and <= 45 min cap (`ffprobe` + decode check), measures loudness (`ebur128`), detects speech silences (`silencedetect`), generates `peaks.json`, transcodes to 96 kbps Opus (`audio.opus`) + 16 kHz mono MP3 (`asr.mp3`), uploads outputs to MinIO/S3, and deletes the uncompressed upload to reclaim user quota.
2. **Transcribe (`transcribe.py`)**: Calls Groq `whisper-large-v3-turbo` (`GROQ_API_KEY`) with chunking & word timestamps, automatically falling back to local `faster-whisper` (`int8` CPU).
3. **Align (`align.py`)**: Normalizes script and spoken words (`NFKC` + numbers/years), anchors 3-grams, runs Needleman-Wunsch + phonetic near-match scoring, snaps cuts to natural pauses, preserves manual boundaries on re-align, and generates script-accurate subtitle cues.

## Local commands (WSL)

```bash
cd services/audio
uv sync --python 3.12
uv run pytest -v
uv run python -m pace_audio.worker
```

Health & wake endpoints listen on `http://127.0.0.1:8091` (`GET /healthz`, `POST /wake`).
