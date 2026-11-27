import json
import subprocess
from pathlib import Path

import pytest

from pace_audio.align import SpokenWord
from pace_audio.audio_ops import (
    PermanentAudioError,
    compute_peaks,
    detect_silences,
    measure_loudness,
    probe_audio,
    transcode_asr_mp3,
    transcode_opus,
)
from pace_audio.transcribe import (
    GroqTranscriber,
    LocalTranscriber,
    TranscriptResult,
    merge_chunk_words,
    sanitize_prompt_hint,
    transcribe_with_fallback,
)


def _make_tone_with_silence(wav_path: Path) -> None:
    """Create a 3-second WAV: 1s 440Hz tone -> 1s silence -> 1s 520Hz tone."""
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
        "anullsrc=r=48000:cl=mono:d=1",
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
        str(wav_path),
    ]
    res = subprocess.run(cmd, capture_output=True, text=True, check=False)
    assert res.returncode == 0, res.stderr


def test_ffmpeg_prepare_pipeline_and_invalid_audio_rejection(tmp_path: Path):
    wav = tmp_path / "sample.wav"
    _make_tone_with_silence(wav)

    probe = probe_audio(wav)
    assert 2800 <= probe.duration_ms <= 3200
    assert probe.channels == 1

    lufs, peak_db = measure_loudness(wav)
    assert lufs is not None
    assert peak_db is not None

    silences = detect_silences(wav)
    assert len(silences) == 1
    assert silences[0].duration_ms >= 800

    peaks_path = tmp_path / "peaks.json"
    peaks = compute_peaks(wav, probe.duration_ms, peaks_path)
    loaded = json.loads(peaks_path.read_text(encoding="utf-8"))
    assert len(loaded) >= 200
    assert len(peaks) == len(loaded)
    assert all(len(pair) == 2 and pair[0] <= pair[1] for pair in loaded)

    opus = tmp_path / "audio.opus"
    opus_bytes = transcode_opus(wav, opus, probe.duration_ms)
    assert opus_bytes > 500
    assert opus.is_file()

    asr = tmp_path / "asr.mp3"
    asr_bytes = transcode_asr_mp3(wav, asr)
    assert asr_bytes > 500
    assert asr.is_file()

    # Corrupt / non-audio file fails permanently
    bad = tmp_path / "corrupt.wav"
    bad.write_bytes(b"not valid audio data")
    with pytest.raises(PermanentAudioError):
        probe_audio(bad)


def test_merge_chunk_words_and_groq_error_falls_back_to_local(monkeypatch, tmp_path: Path):
    chunk1 = [
        SpokenWord("hello", 0, 400, 0.9),
        SpokenWord("edge1", 599_200, 599_900, 0.9),  # inside right overlap (>599s) -> dropped
    ]
    chunk2 = [
        SpokenWord("edge2", 200, 600, 0.9),  # inside left overlap (0.4s < 1s) -> dropped
        SpokenWord("world", 1500, 2000, 0.95),  # >1s into chunk2 -> kept at 601_500ms
    ]
    merged = merge_chunk_words([(0, 600_000, chunk1), (600_000, 10_000, chunk2)], overlap_ms=2000)
    assert [w.word for w in merged] == ["hello", "world"]
    assert merged[1].start_ms == 601_500

    def broken_groq(*_args, **_kwargs):
        raise RuntimeError("Groq 500 / 429 failure")

    def fake_local(self, *_args, **_kwargs):
        return TranscriptResult(
            engine="local_faster_whisper",
            model=self.model_name,
            language="en",
            words=[SpokenWord("fallback", 100, 500, 0.88)],
        )

    monkeypatch.setattr(GroqTranscriber, "transcribe", broken_groq)
    monkeypatch.setattr(LocalTranscriber, "transcribe", fake_local)

    res = transcribe_with_fallback(
        asr_path=tmp_path / "asr.mp3",
        duration_ms=2000,
        language="en",
        prompt_hint="fallback",
        temp_dir=tmp_path,
        groq_api_key="dummy-key",
        local_model="tiny.en",
        force_local=False,
    )
    assert res.engine == "local_faster_whisper"
    assert res.words[0].word == "fallback"


def test_sanitize_prompt_hint_clamps_to_max_chars():
    long_script = "word " * 500  # 2500 chars
    clamped = sanitize_prompt_hint(long_script, max_chars=800)
    assert len(clamped) <= 800
    assert not clamped.endswith(" ")
    assert clamped.startswith("word word")

    short_script = "Hello world from deep ocean"
    assert sanitize_prompt_hint(short_script) == short_script
