"""FFmpeg & ffprobe audio preparation operations (TDD Section 10.2).

Every subprocess invocation passes explicit argument lists (`shell=False`)
and bounds memory usage by streaming PCM samples instead of loading full arrays.
"""

from __future__ import annotations

import json
import re
import struct
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import List, Tuple

from pace_audio.align import SilenceGap

MAX_NARRATION_DURATION_MS = 45 * 60 * 1000  # 45 minutes hard cap (NAR-02)


class PermanentAudioError(Exception):
    """Raised when an audio file cannot be repaired by retrying (corrupt, silent, >45 min)."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(frozen=True)
class AudioProbeResult:
    duration_ms: int
    sample_rate: int
    channels: int
    codec: str


def probe_audio(path: Path) -> AudioProbeResult:
    """Validate audio container, verify an audio stream exists, and check <= 45 min duration."""
    if not path.is_file() or path.stat().st_size == 0:
        raise PermanentAudioError("EMPTY_AUDIO", "Uploaded audio file is empty or missing")

    cmd = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=codec_name,sample_rate,channels:format=duration",
        "-of",
        "json",
        str(path),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise PermanentAudioError(
            "INVALID_AUDIO",
            f"ffprobe failed to inspect audio: {proc.stderr.strip()[:300]}",
        )

    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise PermanentAudioError("INVALID_AUDIO", f"Invalid ffprobe output: {exc}") from exc

    streams = payload.get("streams") or []
    if not streams:
        raise PermanentAudioError("NO_AUDIO_STREAM", "File contains no playable audio stream")

    stream = streams[0]
    raw_duration = (payload.get("format") or {}).get("duration") or stream.get("duration")
    try:
        duration_sec = float(raw_duration)
    except (TypeError, ValueError) as exc:
        raise PermanentAudioError("INVALID_DURATION", "Could not read audio duration") from exc

    duration_ms = int(round(duration_sec * 1000))
    if duration_ms <= 0:
        raise PermanentAudioError("INVALID_DURATION", "Audio duration must be greater than 0")
    if duration_ms > MAX_NARRATION_DURATION_MS:
        raise PermanentAudioError(
            "AUDIO_TOO_LONG",
            "Narration exceeds maximum allowed duration of 45 minutes",
        )

    # Decode-verify first + full lightweight decode check to catch truncated streams (NAR-02).
    verify_cmd = [
        "ffmpeg",
        "-v",
        "error",
        "-i",
        str(path),
        "-vn",
        "-f",
        "null",
        "-",
    ]
    verify_proc = subprocess.run(verify_cmd, capture_output=True, text=True, check=False)
    if verify_proc.returncode != 0:
        raise PermanentAudioError(
            "CORRUPT_AUDIO",
            f"Audio decode error: {verify_proc.stderr.strip()[:300]}",
        )

    return AudioProbeResult(
        duration_ms=duration_ms,
        sample_rate=int(stream.get("sample_rate") or 48000),
        channels=int(stream.get("channels") or 1),
        codec=str(stream.get("codec_name") or "unknown"),
    )


def measure_loudness(path: Path) -> Tuple[float | None, float | None]:
    """Measure integrated LUFS and true peak (dBTP) using FFmpeg ebur128 (NAR-05)."""
    cmd = [
        "ffmpeg",
        "-nostats",
        "-i",
        str(path),
        "-vn",
        "-af",
        "ebur128=peak=true",
        "-f",
        "null",
        "-",
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    text = proc.stderr or ""

    # Summary block appears at the bottom of ebur128 output.
    lufs_match = re.findall(r"\bI:\s*(-?\d+(?:\.\d+)?)\s*LUFS", text)
    peak_match = re.findall(r"\bPeak:\s*(-?\d+(?:\.\d+)?)\s*dBFS", text)

    lufs = float(lufs_match[-1]) if lufs_match else None
    peak = float(peak_match[-1]) if peak_match else None
    return lufs, peak


def detect_silences(path: Path, noise_db: str = "-35dB", min_dur_sec: float = 0.15) -> List[SilenceGap]:
    """Detect natural pauses >= 150ms for pause-snapping alignment (TDD 10.2)."""
    cmd = [
        "ffmpeg",
        "-nostats",
        "-i",
        str(path),
        "-vn",
        "-af",
        f"silencedetect=noise={noise_db}:d={min_dur_sec:.2f}",
        "-f",
        "null",
        "-",
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    silences: List[SilenceGap] = []
    pending_start_ms: int | None = None

    for line in (proc.stderr or "").splitlines():
        start_m = re.search(r"silence_start:\s*([0-9]+(?:\.[0-9]+)?)", line)
        if start_m:
            pending_start_ms = int(round(float(start_m.group(1)) * 1000))
            continue

        end_m = re.search(r"silence_end:\s*([0-9]+(?:\.[0-9]+)?)\s*\|\s*silence_duration:\s*([0-9]+(?:\.[0-9]+)?)", line)
        if end_m:
            end_ms = int(round(float(end_m.group(1)) * 1000))
            dur_ms = int(round(float(end_m.group(2)) * 1000))
            start_ms = pending_start_ms if pending_start_ms is not None else max(0, end_ms - dur_ms)
            pending_start_ms = None
            if dur_ms >= int(min_dur_sec * 1000):
                silences.append(SilenceGap(start_ms=start_ms, end_ms=end_ms, duration_ms=dur_ms))

    return silences


def compute_peaks(path: Path, duration_ms: int, out_json: Path) -> List[List[float]]:
    """Compute ~1,000 min/max waveform pairs per minute via bounded PCM streaming (NAR-03)."""
    minutes = max(1.0, duration_ms / 60_000.0)
    num_buckets = max(200, min(45_000, int(round(minutes * 1000))))
    sample_rate = 8000
    total_samples = max(num_buckets, int(round((duration_ms / 1000.0) * sample_rate)))
    samples_per_bucket = max(1, total_samples // num_buckets)

    cmd = [
        "ffmpeg",
        "-v",
        "error",
        "-i",
        str(path),
        "-vn",
        "-ac",
        "1",
        "-ar",
        str(sample_rate),
        "-f",
        "s16le",
        "pipe:1",
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert proc.stdout is not None

    peaks: List[List[float]] = []
    cur_min = 1.0
    cur_max = -1.0
    count_in_bucket = 0
    leftover = b""

    while True:
        chunk = proc.stdout.read(65_536)
        if not chunk:
            break
        buf = leftover + chunk
        usable_len = (len(buf) // 2) * 2
        leftover = buf[usable_len:]
        if usable_len == 0:
            continue

        sample_count = usable_len // 2
        samples = struct.unpack(f"<{sample_count}h", buf[:usable_len])
        for raw in samples:
            val = raw / 32768.0
            if val < cur_min:
                cur_min = val
            if val > cur_max:
                cur_max = val
            count_in_bucket += 1
            if count_in_bucket >= samples_per_bucket:
                peaks.append([round(cur_min, 3), round(cur_max, 3)])
                cur_min = 1.0
                cur_max = -1.0
                count_in_bucket = 0

    proc.wait()
    if count_in_bucket > 0:
        peaks.append([round(cur_min, 3), round(cur_max, 3)])

    if not peaks:
        peaks = [[0.0, 0.0]]

    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(peaks), encoding="utf-8")
    return peaks


def transcode_opus(src: Path, dst: Path, expected_duration_ms: int) -> int:
    """Create compressed 96 kbps 48 kHz Opus copy preserving channel count & verifying decode (NAR-06)."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "ffmpeg",
        "-y",
        "-v",
        "error",
        "-i",
        str(src),
        "-vn",
        "-c:a",
        "libopus",
        "-b:a",
        "96k",
        "-ar",
        "48000",
        str(dst),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise PermanentAudioError("TRANSCODE_FAILED", f"Opus transcode failed: {proc.stderr.strip()[:300]}")

    probe = probe_audio(dst)
    if abs(probe.duration_ms - expected_duration_ms) > 2500:
        raise PermanentAudioError(
            "TRANSCODE_MISMATCH",
            f"Opus duration ({probe.duration_ms}ms) mismatched original ({expected_duration_ms}ms)",
        )
    return dst.stat().st_size


def transcode_asr_mp3(src: Path, dst: Path) -> int:
    """Create lightweight 16 kHz mono 32 kbps MP3 transcription copy (TDD 10.2)."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "ffmpeg",
        "-y",
        "-v",
        "error",
        "-i",
        str(src),
        "-vn",
        "-ac",
        "1",
        "-ar",
        "16000",
        "-c:a",
        "libmp3lame",
        "-b:a",
        "32k",
        str(dst),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise PermanentAudioError("ASR_TRANSCODE_FAILED", f"ASR MP3 transcode failed: {proc.stderr.strip()[:300]}")
    return dst.stat().st_size


def split_audio_chunks(
    asr_path: Path,
    duration_ms: int,
    temp_dir: Path,
    chunk_sec: int = 600,
    overlap_sec: int = 2,
) -> List[Tuple[int, Path]]:
    """Split audio into 10-minute overlapping chunks if longer than chunk_sec (TDD 10.3)."""
    if duration_ms <= chunk_sec * 1000:
        return [(0, asr_path)]

    chunks: List[Tuple[int, Path]] = []
    start_sec = 0.0
    total_sec = duration_ms / 1000.0
    idx = 0

    while start_sec < total_sec:
        chunk_file = temp_dir / f"asr_chunk_{idx:03d}.mp3"
        seg_dur = chunk_sec + (overlap_sec if start_sec + chunk_sec < total_sec else 0)
        cmd = [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-ss",
            f"{start_sec:.3f}",
            "-t",
            f"{seg_dur:.3f}",
            "-i",
            str(asr_path),
            "-c",
            "copy",
            str(chunk_file),
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if proc.returncode != 0 or not chunk_file.is_file():
            raise PermanentAudioError("CHUNK_FAILED", f"Failed to split audio chunk {idx}: {proc.stderr[:200]}")

        chunks.append((int(round(start_sec * 1000)), chunk_file))
        if start_sec + chunk_sec >= total_sec:
            break
        start_sec += chunk_sec
        idx += 1

    return chunks
