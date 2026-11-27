"""End-to-end integration test verifying Python worker <-> PostgreSQL queue <-> MinIO S3 <-> FFmpeg pipeline."""

from __future__ import annotations

import json
import subprocess
import uuid
from pathlib import Path

import psycopg

from pace_audio.align import SpokenWord
from pace_audio.config import AudioWorkerConfig
from pace_audio.storage import AudioStorage
from pace_audio.transcribe import TranscriptResult
from pace_audio.worker import run_single_job_cycle


def _create_test_wav(path: Path) -> None:
    cmd = [
        "ffmpeg",
        "-y",
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=48000:cl=mono:d=0.6",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=520:duration=1",
        "-filter_complex",
        "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]",
        "-map",
        "[out]",
        "-ar",
        "48000",
        "-ac",
        "1",
        str(path),
    ]
    res = subprocess.run(cmd, capture_output=True, text=True, check=False)
    assert res.returncode == 0, res.stderr


def test_worker_end_to_end_postgres_minio_and_realign(monkeypatch, tmp_path: Path):
    cfg = AudioWorkerConfig.from_env()
    storage = AudioStorage(cfg)

    user_id = str(uuid.uuid4())
    project_id = str(uuid.uuid4())
    media_id = str(uuid.uuid4())
    orig_key = f"u/{user_id}/p/{project_id}/audio/{media_id}/original.wav"
    opus_key = f"u/{user_id}/p/{project_id}/audio/{media_id}/audio.opus"
    asr_key = f"u/{user_id}/p/{project_id}/audio/{media_id}/asr.mp3"
    peaks_key = f"u/{user_id}/p/{project_id}/audio/{media_id}/peaks.json"

    wav_path = tmp_path / "narration.wav"
    _create_test_wav(wav_path)
    wav_bytes = wav_path.stat().st_size
    storage.upload_file(wav_path, orig_key, "audio/wav")

    with psycopg.connect(cfg.database_url) as conn:
        with conn.cursor() as cur:
            cur.execute(
                """
                INSERT INTO users (id, email, name, password_hash, role, must_change_password, storage_quota_bytes, storage_used_bytes)
                VALUES (%s, %s, 'Worker Test', 'hash', 'creator', false, 100000000, %s)
                """,
                (user_id, f"py_worker_{uuid.uuid4()}@example.com", wav_bytes),
            )
            cur.execute(
                """
                INSERT INTO projects (id, owner_id, name, aspect_ratio, fps, language)
                VALUES (%s, %s, 'Worker E2E Project', '16:9', 30, 'en')
                """,
                (project_id, user_id),
            )
            cur.execute(
                """
                INSERT INTO sections (project_id, position, text)
                VALUES
                  (%s, 0, 'First paragraph opens our story cleanly.'),
                  (%s, 1, 'Second paragraph arrives right after the pause.')
                """,
                (project_id, project_id),
            )
            cur.execute(
                """
                INSERT INTO media_assets (id, project_id, owner_id, kind, storage_key, original_filename, mime, bytes, sha256, status)
                VALUES (%s, %s, %s, 'narration', %s, 'narration.wav', 'audio/wav', %s, 'sha_e2e', 'ready')
                """,
                (media_id, project_id, user_id, orig_key, wav_bytes),
            )
            payload = {
                "project_id": project_id,
                "user_id": user_id,
                "media_id": media_id,
                "storage_key": orig_key,
                "opus_key": opus_key,
                "asr_key": asr_key,
                "peaks_key": peaks_key,
                "mime": "audio/wav",
                "original_filename": "narration.wav",
                "keep_manual_boundaries": False,
                "force_local_transcription": False,
            }
            cur.execute(
                """
                INSERT INTO jobs (id, queue, type, project_id, user_id, priority, payload, max_attempts)
                VALUES (%s, 'audio', 'audio_process', %s, %s, 99999, %s::jsonb, 3)
                """,
                (str(uuid.uuid4()), project_id, user_id, json.dumps(payload)),
            )
        conn.commit()

    # Stub transcription so test runs fast and deterministically offline
    def fake_transcribe(**_kwargs):
        return TranscriptResult(
            engine="test_stub",
            model="whisper-test",
            language="en",
            words=[
                SpokenWord("First", 50, 250, 0.99),
                SpokenWord("paragraph", 260, 520, 0.99),
                SpokenWord("opens", 530, 720, 0.99),
                SpokenWord("our", 730, 820, 0.99),
                SpokenWord("story", 830, 950, 0.99),
                SpokenWord("cleanly", 960, 1080, 0.99),
                SpokenWord("Second", 1620, 1800, 0.99),
                SpokenWord("paragraph", 1810, 2000, 0.99),
                SpokenWord("arrives", 2010, 2180, 0.99),
                SpokenWord("right", 2190, 2290, 0.99),
                SpokenWord("after", 2300, 2400, 0.99),
                SpokenWord("the", 2410, 2480, 0.99),
                SpokenWord("pause", 2490, 2580, 0.99),
            ],
        )

    monkeypatch.setattr("pace_audio.worker.transcribe_with_fallback", fake_transcribe)

    try:
        worked = run_single_job_cycle(cfg, storage)
        assert worked is True

        # Original raw WAV deleted after verified Opus upload; Opus/ASR/Peaks exist in MinIO
        assert storage.head_size(orig_key) is None
        opus_size = storage.head_size(opus_key)
        assert opus_size is not None and opus_size > 0
        assert storage.head_size(asr_key) is not None
        assert storage.head_size(peaks_key) is not None

        with psycopg.connect(cfg.database_url) as conn:
            with conn.cursor() as cur:
                cur.execute("SELECT status, progress FROM jobs WHERE project_id = %s", (project_id,))
                job_row = cur.fetchone()
                assert job_row == ("succeeded", 100)

                cur.execute("SELECT duration_ms, peaks_key FROM narrations WHERE project_id = %s", (project_id,))
                nar_row = cur.fetchone()
                assert nar_row is not None and 2400 <= nar_row[0] <= 2800
                assert nar_row[1] == peaks_key

                cur.execute("SELECT index, time_ms FROM boundaries WHERE project_id = %s ORDER BY index", (project_id,))
                bounds = cur.fetchall()
                assert len(bounds) == 3
                assert bounds[0][1] == 0
                assert 1000 <= bounds[1][1] <= 1700
                assert bounds[2][1] == nar_row[0]

                cur.execute("SELECT position, text FROM subtitle_cues WHERE project_id = %s ORDER BY position", (project_id,))
                cues = cur.fetchall()
                assert len(cues) == 2

                # User quota reconciled to much smaller Opus size (NAR-06)
                cur.execute("SELECT storage_used_bytes FROM users WHERE id = %s", (user_id,))
                used = cur.fetchone()[0]
                assert used == opus_size < wav_bytes
    finally:
        for k in [orig_key, opus_key, asr_key, peaks_key]:
            storage.delete_object(k)
        with psycopg.connect(cfg.database_url) as conn:
            with conn.cursor() as cur:
                cur.execute("DELETE FROM jobs WHERE project_id = %s", (project_id,))
                cur.execute("DELETE FROM projects WHERE id = %s", (project_id,))
                cur.execute("DELETE FROM users WHERE id = %s", (user_id,))
            conn.commit()
