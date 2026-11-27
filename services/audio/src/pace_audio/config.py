"""Configuration loader for the PACE Python audio worker."""

from __future__ import annotations

import os
import socket
from dataclasses import dataclass
from pathlib import Path

from dotenv import load_dotenv


def _load_project_env() -> None:
    """Load root .env when running locally from repo or services/audio."""
    cwd = Path.cwd()
    for parent in [cwd, *cwd.parents]:
        env_file = parent / ".env"
        if env_file.is_file():
            load_dotenv(env_file, override=False)
            break


@dataclass(frozen=True)
class AudioWorkerConfig:
    database_url: str
    s3_endpoint: str
    s3_bucket: str
    s3_region: str
    s3_access_key: str
    s3_secret_key: str
    groq_api_key: str
    worker_id: str
    http_host: str
    http_port: int
    poll_interval_secs: float
    local_whisper_model: str
    worker_shared_secret: str

    @classmethod
    def from_env(cls) -> "AudioWorkerConfig":
        _load_project_env()
        default_worker_id = f"audio-{socket.gethostname()}-{os.getpid()}"
        return cls(
            database_url=os.getenv(
                "DATABASE_URL", "postgres://pace:pace@localhost:5432/pace"
            ),
            s3_endpoint=os.getenv("S3_ENDPOINT", "http://localhost:9000"),
            s3_bucket=os.getenv("S3_BUCKET", "pace"),
            s3_region=os.getenv("S3_REGION", "us-east-1"),
            s3_access_key=os.getenv("S3_ACCESS_KEY", "pace"),
            s3_secret_key=os.getenv("S3_SECRET_KEY", "pace-secret"),
            groq_api_key=os.getenv("GROQ_API_KEY", "").strip(),
            worker_id=os.getenv("AUDIO_WORKER_ID", default_worker_id),
            http_host=os.getenv("AUDIO_WORKER_HOST", "127.0.0.1"),
            http_port=int(os.getenv("AUDIO_WORKER_PORT", "8091")),
            poll_interval_secs=float(os.getenv("AUDIO_POLL_INTERVAL_SECS", "1.0")),
            local_whisper_model=os.getenv("PACE_LOCAL_WHISPER_MODEL", "tiny.en"),
            worker_shared_secret=os.getenv("WORKER_SHARED_SECRET", ""),
        )
