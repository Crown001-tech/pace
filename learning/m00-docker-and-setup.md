# Milestone 0: Docker & Machine Setup Notes

## 1. Key Concepts in Plain Words

- **Image**: A frozen, read-only template containing an application and everything it needs to run (e.g. `postgres:16-alpine`). Think of it as a pre-packaged installer.
- **Container**: A running instance of an image, isolated from the host OS.
- **Volume**: A persistent storage folder that outlives container restarts (e.g., `pg-data`). Without volumes, container data vanishes when the container stops.
- **Port Mapping (`5432:5432`)**: Bridges a port on the host laptop (first number) to a port inside the container (second number).
- **Docker Compose**: A tool and YAML specification (`compose.yml`) that coordinates multiple containers (e.g., PostgreSQL + MinIO) as a single fleet.

---

## 2. Machine Configuration

- **OS**: Windows 11 + WSL2 Ubuntu (Intel Core i7-8550U, 8 GB RAM).
- **Memory Boundary**: `C:\Users\<user>\.wslconfig` limits WSL2 to 5 GB RAM and 4 GB swap to prevent starving the 8 GB host system.
- **Filesystem Rule**: Code lives inside native Linux (`~/code/pace`) to avoid the 9P filesystem translation penalty on `/mnt/c/`.

---

## 3. Daily Commands

```bash
# Start database and object storage in background
make up

# View container logs
make logs

# Stop containers (preserves database data)
make down

# Fresh start: stop containers and delete all data volumes
make reset

# Run tests
make test
```

---

## 4. First Experiments Log

### Experiment S2: Speech Transcription (Groq Whisper vs. Local `faster-whisper`)
- **Test Fixture**: `fixtures/audio/sample_1min_asr.mp3` (`79.78s` spoken English narration, 16 kHz mono 32 kbps MP3, `319,616` bytes).
- **Script**: `learning/exercises/s2-transcription/transcribe_spike.py`
- **Groq Whisper (`whisper-large-v3-turbo`)**:
  - **Time**: `1.65s – 1.94s` (**41.1x – 48.3x faster than real-time**, `137` words with word-level timestamps).
  - **Note**: Requires a custom `User-Agent` header (`pace-audio-worker/0.1`) so Cloudflare does not block default `python-httpx` requests with `HTTP 403`.
- **Local `faster-whisper` (`small.en`, `int8`, 4 CPU threads)**:
  - **Time**: `66.68s` transcription (**1.2x real-time**, `139` words with word timestamps and probabilities) + `54.22s` cold model load.
  - **Note**: Decoding MP3 to 16 kHz mono `float32` PCM via `ffmpeg` and passing the `numpy` array directly into `model.transcribe()` avoids `PyAV` (`av.open`) version incompatibilities.

### Experiment S3: Direct Browser-to-Storage Upload (MinIO)
- **Script & Page**: `learning/exercises/s3-upload/upload_spike.py` & `learning/exercises/s3-upload/index.html`
- **Result**:
  - Created bucket `pace` on local MinIO (`localhost:9000`).
  - Generated a 60-minute S3v4 pre-signed `PUT` URL.
  - Verified direct `PUT` (`HTTP 200`, `69` bytes stored) without passing file bytes or credentials through an API.
  - Aborting an upload mid-stream leaves no corrupted partial object in MinIO; retrying the `PUT` with the same signed link succeeds cleanly.

---

## 5. Three-Line Milestone Takeaway

1. Docker containers isolate PostgreSQL and MinIO with 512 MB RAM caps, while named volumes (`pg-data`, `minio-data`) keep data safe across container restarts.
2. Groq `whisper-large-v3-turbo` transcribes speech at ~41x real-time (~2s per minute of audio), while local `faster-whisper` (`small.en`, `int8`) runs at ~1.2x real-time on the i7-8550U CPU as a reliable $0 fallback when fed FFmpeg-decoded PCM.
3. Pre-signed S3 `PUT` links let the browser upload media straight to MinIO/B2 without routing heavy files through the Rust API, and aborted single-PUT uploads leave no corrupt partial state behind.
