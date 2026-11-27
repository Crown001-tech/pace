"""PostgreSQL job queue worker + Starlette wake/health server for PACE audio processing (TDD 8 & 10)."""

from __future__ import annotations

import json
import logging
import os
import tempfile
import threading
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

import psycopg
from psycopg.rows import dict_row
from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import Route
import uvicorn

from pace_audio.align import (
    SectionInput,
    SilenceGap,
    SpokenWord,
    calculate_boundaries,
)
from pace_audio.audio_ops import (
    PermanentAudioError,
    compute_peaks,
    detect_silences,
    measure_loudness,
    probe_audio,
    transcode_asr_mp3,
    transcode_opus,
)
from pace_audio.config import AudioWorkerConfig
from pace_audio.storage import AudioStorage
from pace_audio.transcribe import transcribe_with_fallback

logging.basicConfig(
    level=os.getenv("LOG_LEVEL", "INFO").upper(),
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("pace_audio.worker")


def claim_next_audio_job(conn: psycopg.Connection[Any], worker_id: str) -> Optional[Dict[str, Any]]:
    """Claim next queued or stale (>120s heartbeat) audio job with FOR UPDATE SKIP LOCKED (TDD 8.2)."""
    with conn.cursor(row_factory=dict_row) as cur:
        cur.execute(
            """
            UPDATE jobs
            SET status = 'running',
                locked_by = %s,
                started_at = COALESCE(started_at, now()),
                heartbeat_at = now(),
                attempts = attempts + 1,
                updated_at = now()
            WHERE id = (
                SELECT id
                FROM jobs
                WHERE queue = 'audio'
                  AND (
                        (status = 'queued' AND run_after <= now())
                     OR (status = 'running' AND heartbeat_at < now() - interval '120 seconds')
                  )
                ORDER BY priority DESC, created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING *
            """,
            (worker_id,),
        )
        row = cur.fetchone()
    conn.commit()
    return dict(row) if row else None


def update_job_stage(
    conn: psycopg.Connection[Any],
    job_id: Any,
    worker_id: str,
    progress: int,
    stage: str,
) -> bool:
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE jobs
            SET heartbeat_at = now(),
                progress = %s,
                stage = %s,
                updated_at = now()
            WHERE id = %s AND locked_by = %s AND status = 'running'
            """,
            (max(0, min(100, progress)), stage, job_id, worker_id),
        )
        updated = cur.rowcount > 0
    conn.commit()
    return updated


def complete_audio_job(
    conn: psycopg.Connection[Any],
    job_id: Any,
    worker_id: str,
    result: Dict[str, Any],
) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE jobs
            SET status = 'succeeded',
                progress = 100,
                stage = 'completed',
                result = %s::jsonb,
                error_code = NULL,
                error_message = NULL,
                finished_at = now(),
                heartbeat_at = now(),
                updated_at = now()
            WHERE id = %s AND locked_by = %s AND status = 'running'
            """,
            (json.dumps(result), job_id, worker_id),
        )
    conn.commit()


def fail_audio_job(
    conn: psycopg.Connection[Any],
    job_id: Any,
    worker_id: str,
    attempts: int,
    max_attempts: int,
    error_code: str,
    error_message: str,
    permanent: bool,
) -> None:
    should_fail = permanent or attempts >= max_attempts
    with conn.cursor() as cur:
        if should_fail:
            cur.execute(
                """
                UPDATE jobs
                SET status = 'failed',
                    stage = 'failed',
                    error_code = %s,
                    error_message = %s,
                    finished_at = now(),
                    updated_at = now()
                WHERE id = %s AND locked_by = %s AND status = 'running'
                """,
                (error_code, error_message[:1000], job_id, worker_id),
            )
        else:
            backoff_secs = 30 if attempts == 1 else (120 if attempts == 2 else 600)
            cur.execute(
                """
                UPDATE jobs
                SET status = 'queued',
                    stage = 'retrying',
                    error_code = %s,
                    error_message = %s,
                    locked_by = NULL,
                    heartbeat_at = NULL,
                    run_after = now() + (%s || ' seconds')::interval,
                    updated_at = now()
                WHERE id = %s AND locked_by = %s AND status = 'running'
                """,
                (error_code, error_message[:1000], str(backoff_secs), job_id, worker_id),
            )
    conn.commit()


class HeartbeatLoop:
    """Keeps heartbeat_at fresh every 5 seconds while FFmpeg/Whisper run."""

    def __init__(self, database_url: str, job_id: Any, worker_id: str) -> None:
        self._database_url = database_url
        self._job_id = job_id
        self._worker_id = worker_id
        self._stop = threading.Event()
        self._progress = 5
        self._stage = "preparing"
        self._lock = threading.Lock()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def set_stage(self, progress: int, stage: str) -> None:
        with self._lock:
            self._progress = progress
            self._stage = stage

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=6.0)

    def _run(self) -> None:
        try:
            with psycopg.connect(self._database_url) as conn:
                while not self._stop.wait(5.0):
                    with self._lock:
                        p, s = self._progress, self._stage
                    update_job_stage(conn, self._job_id, self._worker_id, p, s)
        except Exception as exc:
            logger.debug("Heartbeat thread exited: %s", exc)


def _persist_alignment_results(
    conn: psycopg.Connection[Any],
    project_id: Any,
    media_id: Any,
    alignment: Any,
    keep_manual: bool,
) -> None:
    with conn.cursor() as cur:
        cur.execute("DELETE FROM boundaries WHERE project_id = %s", (project_id,))
        for b in alignment.boundaries:
            cur.execute(
                """
                INSERT INTO boundaries (project_id, index, time_ms, source, confidence, flags)
                VALUES (%s, %s, %s, %s, %s, %s)
                """,
                (project_id, b.index, b.time_ms, b.source, b.confidence, b.flags),
            )

        if keep_manual:
            cur.execute("DELETE FROM subtitle_cues WHERE project_id = %s AND edited = false", (project_id,))
        else:
            cur.execute("DELETE FROM subtitle_cues WHERE project_id = %s", (project_id,))

        for cue in alignment.subtitle_cues:
            cur.execute(
                """
                INSERT INTO subtitle_cues (project_id, position, start_ms, end_ms, text, edited)
                VALUES (%s, %s, %s, %s, %s, false)
                ON CONFLICT (project_id, position) DO UPDATE
                SET start_ms = EXCLUDED.start_ms,
                    end_ms = EXCLUDED.end_ms,
                    text = CASE WHEN subtitle_cues.edited THEN subtitle_cues.text ELSE EXCLUDED.text END,
                    updated_at = now()
                """,
                (project_id, cue.position, cue.start_ms, cue.end_ms, cue.text),
            )

        cur.execute(
            """
            UPDATE sections
            SET needs_resync = false,
                updated_at = now()
            WHERE project_id = %s
            """,
            (project_id,),
        )
    conn.commit()


def process_audio_job(
    conn: psycopg.Connection[Any],
    cfg: AudioWorkerConfig,
    storage: AudioStorage,
    job: Dict[str, Any],
) -> Dict[str, Any]:
    job_id = job["id"]
    job_type = job["type"]
    payload = job["payload"]
    project_id = payload["project_id"]
    user_id = payload["user_id"]
    media_id = payload["media_id"]
    keep_manual = bool(payload.get("keep_manual_boundaries", False))
    force_local = bool(payload.get("force_local_transcription", False))

    with conn.cursor(row_factory=dict_row) as cur:
        cur.execute("SELECT language, aspect_ratio FROM projects WHERE id = %s", (project_id,))
        proj_row = cur.fetchone()
        if not proj_row:
            raise PermanentAudioError("PROJECT_NOT_FOUND", "Target project no longer exists")
        cur.execute(
            "SELECT id, position, text FROM sections WHERE project_id = %s ORDER BY position ASC",
            (project_id,),
        )
        sec_rows = cur.fetchall()

    language = proj_row["language"] or "en"
    aspect_ratio = proj_row["aspect_ratio"] or "16:9"
    sections = [SectionInput(id=str(r["id"]), position=int(r["position"]), text=r["text"]) for r in sec_rows]
    script_hint = " ".join(s.text for s in sections)

    manual_boundaries: Dict[int, int] = {}
    if keep_manual:
        with conn.cursor(row_factory=dict_row) as cur:
            cur.execute(
                "SELECT index, time_ms FROM boundaries WHERE project_id = %s AND source = 'manual'",
                (project_id,),
            )
            manual_boundaries = {int(r["index"]): int(r["time_ms"]) for r in cur.fetchall()}

    hb = HeartbeatLoop(cfg.database_url, job_id, cfg.worker_id)
    hb.start()
    try:
        with tempfile.TemporaryDirectory(prefix=f"pace_audio_{job_id}_") as tmp:
            tmp_dir = Path(tmp)

            if job_type == "realign":
                with conn.cursor(row_factory=dict_row) as cur:
                    cur.execute(
                        "SELECT duration_ms, silences FROM narrations WHERE project_id = %s",
                        (project_id,),
                    )
                    nar_row = cur.fetchone()
                    cur.execute(
                        "SELECT words FROM transcripts WHERE project_id = %s ORDER BY updated_at DESC LIMIT 1",
                        (project_id,),
                    )
                    tr_row = cur.fetchone()

                if nar_row and tr_row:
                    hb.set_stage(75, "aligning")
                    update_job_stage(conn, job_id, cfg.worker_id, 75, "aligning")
                    duration_ms = int(nar_row["duration_ms"])
                    silences = [SilenceGap(**s) for s in (nar_row["silences"] or [])]
                    spoken_words = [SpokenWord(**w) for w in (tr_row["words"] or [])]
                    alignment = calculate_boundaries(
                        sections=sections,
                        spoken_words=spoken_words,
                        silences=silences,
                        narration_duration_ms=duration_ms,
                        aspect_ratio=aspect_ratio,
                        manual_boundaries=manual_boundaries if keep_manual else None,
                    )
                    _persist_alignment_results(conn, project_id, media_id, alignment, keep_manual)
                    return {
                        "duration_ms": duration_ms,
                        "boundary_count": len(alignment.boundaries),
                        "subtitle_count": len(alignment.subtitle_cues),
                        "overall_confidence": alignment.overall_confidence,
                    }

            # 1. Preparing stage: download original or opus fallback, validate, measure loudness/silences/peaks, transcode
            hb.set_stage(15, "preparing")
            update_job_stage(conn, job_id, cfg.worker_id, 15, "preparing")

            storage_key = payload["storage_key"]
            opus_key = payload["opus_key"]
            asr_key = payload["asr_key"]
            peaks_key = payload["peaks_key"]

            ext = Path(storage_key).suffix or ".audio"
            src_path = tmp_dir / f"source{ext}"
            if storage.head_size(storage_key) is not None:
                storage.download_file(storage_key, src_path)
            elif storage.head_size(opus_key) is not None:
                src_path = tmp_dir / "source.opus"
                storage.download_file(opus_key, src_path)
            else:
                raise PermanentAudioError("SOURCE_NOT_FOUND", "Narration audio object not found in storage")

            probe = probe_audio(src_path)
            lufs, peak_db = measure_loudness(src_path)
            silences = detect_silences(src_path)

            peaks_path = tmp_dir / "peaks.json"
            compute_peaks(src_path, probe.duration_ms, peaks_path)

            opus_path = tmp_dir / "audio.opus"
            opus_bytes = transcode_opus(src_path, opus_path, probe.duration_ms)

            asr_path = tmp_dir / "asr.mp3"
            transcode_asr_mp3(src_path, asr_path)

            storage.upload_file(peaks_path, peaks_key, "application/json")
            storage.upload_file(opus_path, opus_key, "audio/opus")
            storage.upload_file(asr_path, asr_key, "audio/mpeg")

            # NAR-06: delete original uncompressed audio once Opus copy verified uploaded, and reclaim quota diff
            if storage_key != opus_key:
                storage.delete_object(storage_key)

            with conn.cursor() as cur:
                cur.execute(
                    "SELECT bytes FROM media_assets WHERE id = %s AND project_id = %s FOR UPDATE",
                    (media_id, project_id),
                )
                row = cur.fetchone()
                old_bytes = int(row[0]) if row else opus_bytes
                delta = opus_bytes - old_bytes

                cur.execute(
                    """
                    UPDATE media_assets
                    SET storage_key = %s,
                        mime = 'audio/opus',
                        bytes = %s,
                        duration_ms = %s,
                        updated_at = now()
                    WHERE id = %s AND project_id = %s
                    """,
                    (opus_key, opus_bytes, probe.duration_ms, media_id, project_id),
                )
                if delta != 0:
                    cur.execute(
                        """
                        UPDATE users
                        SET storage_used_bytes = GREATEST(0, storage_used_bytes + %s),
                            updated_at = now()
                        WHERE id = %s
                        """,
                        (delta, user_id),
                    )

                silences_json = json.dumps(
                    [{"start_ms": s.start_ms, "end_ms": s.end_ms, "duration_ms": s.duration_ms} for s in silences]
                )
                cur.execute(
                    """
                    INSERT INTO narrations (project_id, media_id, duration_ms, loudness_lufs, true_peak_db, peaks_key, silences)
                    VALUES (%s, %s, %s, %s, %s, %s, %s::jsonb)
                    ON CONFLICT (project_id) DO UPDATE
                    SET media_id = EXCLUDED.media_id,
                        duration_ms = EXCLUDED.duration_ms,
                        loudness_lufs = EXCLUDED.loudness_lufs,
                        true_peak_db = EXCLUDED.true_peak_db,
                        peaks_key = EXCLUDED.peaks_key,
                        silences = EXCLUDED.silences,
                        updated_at = now()
                    """,
                    (project_id, media_id, probe.duration_ms, lufs, peak_db, peaks_key, silences_json),
                )
            conn.commit()

            # 2. Transcribing stage: Groq Whisper API with faster-whisper fallback
            hb.set_stage(50, "transcribing")
            update_job_stage(conn, job_id, cfg.worker_id, 50, "transcribing")

            transcript = transcribe_with_fallback(
                asr_path=asr_path,
                duration_ms=probe.duration_ms,
                language=language,
                prompt_hint=script_hint,
                temp_dir=tmp_dir,
                groq_api_key=cfg.groq_api_key,
                local_model=cfg.local_whisper_model,
                force_local=force_local,
            )

            words_json = json.dumps(
                [
                    {
                        "word": w.word,
                        "start_ms": w.start_ms,
                        "end_ms": w.end_ms,
                        "probability": round(w.probability, 4),
                    }
                    for w in transcript.words
                ]
            )
            with conn.cursor() as cur:
                cur.execute("DELETE FROM transcripts WHERE project_id = %s", (project_id,))
                cur.execute(
                    """
                    INSERT INTO transcripts (project_id, narration_media_id, engine, model, language, words)
                    VALUES (%s, %s, %s, %s, %s, %s::jsonb)
                    """,
                    (project_id, media_id, transcript.engine, transcript.model, transcript.language, words_json),
                )
            conn.commit()

            # 3. Aligning stage: Needleman-Wunsch + pause snapping + subtitles
            hb.set_stage(85, "aligning")
            update_job_stage(conn, job_id, cfg.worker_id, 85, "aligning")

            alignment = calculate_boundaries(
                sections=sections,
                spoken_words=transcript.words,
                silences=silences,
                narration_duration_ms=probe.duration_ms,
                aspect_ratio=aspect_ratio,
                manual_boundaries=manual_boundaries if keep_manual else None,
            )
            _persist_alignment_results(conn, project_id, media_id, alignment, keep_manual)

            return {
                "duration_ms": probe.duration_ms,
                "loudness_lufs": lufs,
                "true_peak_db": peak_db,
                "engine": transcript.engine,
                "model": transcript.model,
                "word_count": len(transcript.words),
                "boundary_count": len(alignment.boundaries),
                "subtitle_count": len(alignment.subtitle_cues),
                "overall_confidence": alignment.overall_confidence,
            }
    finally:
        hb.stop()


def run_single_job_cycle(cfg: AudioWorkerConfig, storage: Optional[AudioStorage] = None) -> bool:
    """Attempt to claim and execute one queued audio job. Returns True if a job was processed."""
    storage_client = storage or AudioStorage(cfg)
    with psycopg.connect(cfg.database_url) as conn:
        job = claim_next_audio_job(conn, cfg.worker_id)
        if not job:
            return False

        job_id = job["id"]
        logger.info("Claimed audio job %s (type=%s, attempt=%s)", job_id, job["type"], job["attempts"])
        try:
            result = process_audio_job(conn, cfg, storage_client, job)
            complete_audio_job(conn, job_id, cfg.worker_id, result)
            logger.info("Completed audio job %s: %s", job_id, result)
        except PermanentAudioError as exc:
            logger.error("Permanent error on audio job %s [%s]: %s", job_id, exc.code, exc.message)
            fail_audio_job(
                conn,
                job_id,
                cfg.worker_id,
                int(job["attempts"]),
                int(job["max_attempts"]),
                exc.code,
                exc.message,
                permanent=True,
            )
        except Exception as exc:
            logger.exception("Retryable error on audio job %s: %s", job_id, exc)
            fail_audio_job(
                conn,
                job_id,
                cfg.worker_id,
                int(job["attempts"]),
                int(job["max_attempts"]),
                "AUDIO_WORKER_ERROR",
                str(exc),
                permanent=False,
            )
        return True


class WakeSignal:
    def __init__(self) -> None:
        self.event = threading.Event()

    def trigger(self) -> None:
        self.event.set()


def build_http_app(cfg: AudioWorkerConfig, wake_signal: WakeSignal) -> Starlette:
    async def healthz(_request: Request) -> JSONResponse:
        return JSONResponse({"status": "ok", "worker_id": cfg.worker_id})

    async def wake(request: Request) -> JSONResponse:
        if cfg.worker_shared_secret:
            token = request.headers.get("x-worker-secret", "")
            if token != cfg.worker_shared_secret:
                return JSONResponse({"error": "unauthorized"}, status_code=401)
        wake_signal.trigger()
        return JSONResponse({"status": "awake"})

    return Starlette(
        routes=[
            Route("/healthz", healthz, methods=["GET"]),
            Route("/wake", wake, methods=["POST"]),
        ]
    )


def serve_forever() -> None:
    cfg = AudioWorkerConfig.from_env()
    storage = AudioStorage(cfg)
    wake_signal = WakeSignal()
    stop_flag = threading.Event()

    def poll_loop() -> None:
        logger.info("PACE Python audio worker %s listening for queue='audio' jobs...", cfg.worker_id)
        while not stop_flag.is_set():
            try:
                worked = run_single_job_cycle(cfg, storage)
                if worked:
                    continue
            except Exception as exc:
                logger.error("Queue poll error: %s", exc)
            wake_signal.event.wait(timeout=cfg.poll_interval_secs)
            wake_signal.event.clear()

    worker_thread = threading.Thread(target=poll_loop, daemon=True)
    worker_thread.start()

    app = build_http_app(cfg, wake_signal)
    try:
        uvicorn.run(app, host=cfg.http_host, port=cfg.http_port, log_level="warning")
    finally:
        stop_flag.set()
        wake_signal.trigger()
        worker_thread.join(timeout=3.0)


if __name__ == "__main__":
    serve_forever()
