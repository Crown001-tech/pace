# Milestone 0: Docker & Machine Setup Notes

## 1. Key Concepts in Plain Words

- **Image**: A frozen, read-only template containing an application and everything it needs to run (e.g. `postgres:16-alpine`). Think of it as a pre-packaged installer.
- **Container**: A running instance of an image, isolated from the host OS.
- **Volume**: A persistent storage folder that outlives container restarts (e.g., `pg-data`). Without volumes, container data vanishes when the container stops.
- **Port Mapping (`5432:5432`)**: Bridges a port on the host laptop (first number) to a port inside the container (second number).
- **Docker Compose**: A tool and YAML specification (`compose.yml`) that coordinates multiple containers (e.g., PostgreSQL + MinIO) as a single fleet.

---

## 2. Machine Configuration

- **OS**: Windows 11 + WSL2 Ubuntu.
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

### Experiment S2: Speech Transcription (Groq Whisper vs. Local)
- *Status*: Pending (Milestone 0)
- *Groq result*:
- *faster-whisper local result*:

### Experiment S3: Direct Browser-to-Storage Upload (MinIO)
- *Status*: Pending (Milestone 0)
- *Observation*:
