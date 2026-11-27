"""Transcription adapters for Groq Whisper API and local faster-whisper fallback (TDD 10.3)."""

from __future__ import annotations

import logging
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, List, Protocol, Tuple

import httpx

from pace_audio.align import SpokenWord
from pace_audio.audio_ops import PermanentAudioError, split_audio_chunks

logger = logging.getLogger(__name__)

GROQ_TRANSCRIBE_URL = "https://api.groq.com/openai/v1/audio/transcriptions"
GROQ_MODEL = "whisper-large-v3-turbo"
# Groq hard limit is 896 characters for the prompt parameter; clamp to 800 to be safe.
MAX_PROMPT_CHARS = 800


def sanitize_prompt_hint(prompt_hint: str, max_chars: int = MAX_PROMPT_CHARS) -> str:
    """Sanitize and trim prompt hint to fit within Whisper prompt character limits."""
    cleaned = " ".join((prompt_hint or "").split())
    if len(cleaned) <= max_chars:
        return cleaned
    trimmed = cleaned[:max_chars]
    last_space = trimmed.rfind(" ")
    return (trimmed[:last_space] if last_space > 0 else trimmed).strip()


@dataclass(frozen=True)
class TranscriptResult:
    engine: str
    model: str
    language: str
    words: List[SpokenWord]


class Transcriber(Protocol):
    def transcribe(
        self,
        asr_path: Path,
        duration_ms: int,
        language: str,
        prompt_hint: str,
        temp_dir: Path,
    ) -> TranscriptResult: ...


def merge_chunk_words(
    chunk_results: List[Tuple[int, int, List[SpokenWord]]],
    overlap_ms: int = 2000,
) -> List[SpokenWord]:
    """Join overlapping 10-min chunks preferring words further from chunk edges (TDD 10.3)."""
    if not chunk_results:
        return []
    if len(chunk_results) == 1:
        _, _, words = chunk_results[0]
        return words

    merged: List[SpokenWord] = []
    half_overlap = overlap_ms // 2

    for idx, (offset_ms, seg_len_ms, words) in enumerate(chunk_results):
        is_first = idx == 0
        is_last = idx == len(chunk_results) - 1

        left_cutoff = 0 if is_first else offset_ms + half_overlap
        right_cutoff = (
            offset_ms + seg_len_ms + 1000
            if is_last
            else offset_ms + seg_len_ms - half_overlap
        )

        for w in words:
            shifted_start = offset_ms + w.start_ms
            shifted_end = offset_ms + w.end_ms
            mid = (shifted_start + shifted_end) // 2
            if mid >= left_cutoff and (is_last or mid < right_cutoff):
                merged.append(
                    SpokenWord(
                        word=w.word,
                        start_ms=shifted_start,
                        end_ms=max(shifted_start + 10, shifted_end),
                        probability=w.probability,
                    )
                )

    return merged


def _extract_words_from_groq_json(payload: dict) -> List[SpokenWord]:
    words_out: List[SpokenWord] = []
    raw_words = payload.get("words") or []

    if not raw_words and payload.get("segments"):
        for seg in payload["segments"]:
            seg_words = seg.get("words") or []
            if seg_words:
                raw_words.extend(seg_words)
            else:
                text_tokens = str(seg.get("text") or "").strip().split()
                if text_tokens:
                    s_ms = int(round(float(seg.get("start", 0.0)) * 1000))
                    e_ms = max(s_ms + 100, int(round(float(seg.get("end", 0.0)) * 1000)))
                    step = max(1, (e_ms - s_ms) // len(text_tokens))
                    for i, tok in enumerate(text_tokens):
                        w_s = s_ms + i * step
                        w_e = e_ms if i + 1 == len(text_tokens) else min(e_ms, w_s + step)
                        words_out.append(SpokenWord(word=tok, start_ms=w_s, end_ms=w_e, probability=0.75))

    for item in raw_words:
        tok = str(item.get("word") or "").strip()
        if not tok:
            continue
        start_ms = max(0, int(round(float(item.get("start", 0.0)) * 1000)))
        end_ms = max(start_ms + 20, int(round(float(item.get("end", 0.0)) * 1000)))
        prob = float(item.get("probability") or 0.9)
        words_out.append(SpokenWord(word=tok, start_ms=start_ms, end_ms=end_ms, probability=prob))

    return words_out


class GroqTranscriber:
    def __init__(self, api_key: str, model: str = GROQ_MODEL, max_retries: int = 2) -> None:
        self.api_key = api_key
        self.model = model
        self.max_retries = max_retries

    def transcribe(
        self,
        asr_path: Path,
        duration_ms: int,
        language: str,
        prompt_hint: str,
        temp_dir: Path,
    ) -> TranscriptResult:
        if not self.api_key:
            raise RuntimeError("GROQ_API_KEY is not configured")

        chunks = split_audio_chunks(asr_path, duration_ms, temp_dir, chunk_sec=600, overlap_sec=2)
        chunk_outputs: List[Tuple[int, int, List[SpokenWord]]] = []

        prompt_words = sanitize_prompt_hint(prompt_hint, max_chars=MAX_PROMPT_CHARS)
        lang_code = (language or "en").split("-")[0].lower()

        with httpx.Client(timeout=180.0) as client:
            for idx, (offset_ms, chunk_path) in enumerate(chunks):
                chunk_words = self._transcribe_single_chunk(client, chunk_path, lang_code, prompt_words)
                seg_ms = min(602_000, max(1000, duration_ms - offset_ms))
                chunk_outputs.append((offset_ms, seg_ms, chunk_words))
                if idx + 1 < len(chunks):
                    time.sleep(0.5)

        joined = merge_chunk_words(chunk_outputs, overlap_ms=2000)
        return TranscriptResult(
            engine="groq",
            model=self.model,
            language=lang_code,
            words=joined,
        )

    def _transcribe_single_chunk(
        self,
        client: httpx.Client,
        chunk_path: Path,
        language: str,
        prompt_words: str,
    ) -> List[SpokenWord]:
        last_error: Exception | None = None
        for attempt in range(self.max_retries + 1):
            try:
                with chunk_path.open("rb") as fh:
                    form_data: dict[str, Any] = {
                        "model": self.model,
                        "response_format": "verbose_json",
                        "timestamp_granularities[]": ["word"],
                        "temperature": "0",
                    }
                    if language:
                        form_data["language"] = language
                    if prompt_words:
                        form_data["prompt"] = prompt_words

                    resp = client.post(
                        GROQ_TRANSCRIBE_URL,
                        headers={"Authorization": f"Bearer {self.api_key}"},
                        data=form_data,
                        files={"file": (chunk_path.name, fh, "audio/mpeg")},
                    )

                if resp.status_code == 429 or resp.status_code >= 500:
                    retry_after = float(resp.headers.get("retry-after") or (2**attempt))
                    if attempt < self.max_retries:
                        logger.warning("Groq returned HTTP %s; sleeping %.1fs before retry", resp.status_code, retry_after)
                        time.sleep(min(retry_after, 15.0))
                        continue
                    raise RuntimeError(f"Groq API error {resp.status_code}: {resp.text[:300]}")

                # If Groq rejected specifically due to prompt length/content, retry immediately without prompt
                if resp.status_code == 400 and "prompt" in resp.text.lower() and "prompt" in form_data:
                    logger.warning("Groq rejected prompt (%s); retrying without prompt hint", resp.text[:200])
                    form_data.pop("prompt", None)
                    with chunk_path.open("rb") as fh2:
                        resp = client.post(
                            GROQ_TRANSCRIBE_URL,
                            headers={"Authorization": f"Bearer {self.api_key}"},
                            data=form_data,
                            files={"file": (chunk_path.name, fh2, "audio/mpeg")},
                        )

                if resp.status_code >= 400:
                    raise RuntimeError(f"Groq rejected request ({resp.status_code}): {resp.text[:300]}")

                return _extract_words_from_groq_json(resp.json())
            except httpx.HTTPError as exc:
                last_error = exc
                if attempt < self.max_retries:
                    time.sleep(1.5 * (attempt + 1))
                    continue

        raise RuntimeError(f"Groq request failed after retries: {last_error}")


class LocalTranscriber:
    def __init__(self, model_name: str = "tiny.en") -> None:
        self.model_name = model_name

    def transcribe(
        self,
        asr_path: Path,
        duration_ms: int,
        language: str,
        prompt_hint: str,
        temp_dir: Path,
    ) -> TranscriptResult:
        from faster_whisper import WhisperModel
        import numpy as np

        lang_code = (language or "en").split("-")[0].lower()
        prompt_words = sanitize_prompt_hint(prompt_hint, max_chars=MAX_PROMPT_CHARS)

        # Decode with ffmpeg to 16kHz mono float32 PCM so faster-whisper avoids PyAV's
        # `av.open(..., metadata_errors="ignore")` incompatibility on newer PyAV releases.
        pcm = subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-nostdin",
                "-i",
                str(asr_path),
                "-f",
                "f32le",
                "-ac",
                "1",
                "-acodec",
                "pcm_f32le",
                "-ar",
                "16000",
                "-",
            ],
            capture_output=True,
            check=True,
        )
        audio_f32 = np.frombuffer(pcm.stdout, dtype=np.float32)

        model = WhisperModel(self.model_name, device="cpu", compute_type="int8")

        segments, info = model.transcribe(
            audio_f32,
            language=lang_code,
            initial_prompt=prompt_words or None,
            word_timestamps=True,
            vad_filter=True,
        )

        words_out: List[SpokenWord] = []
        for seg in segments:
            if seg.words:
                for w in seg.words:
                    tok = str(w.word or "").strip()
                    if not tok:
                        continue
                    s_ms = max(0, int(round(w.start * 1000)))
                    e_ms = max(s_ms + 20, int(round(w.end * 1000)))
                    prob = float(getattr(w, "probability", 0.8) or 0.8)
                    words_out.append(SpokenWord(word=tok, start_ms=s_ms, end_ms=e_ms, probability=prob))

        detected_lang = getattr(info, "language", lang_code) or lang_code
        return TranscriptResult(
            engine="local_faster_whisper",
            model=self.model_name,
            language=detected_lang,
            words=words_out,
        )


def transcribe_with_fallback(
    asr_path: Path,
    duration_ms: int,
    language: str,
    prompt_hint: str,
    temp_dir: Path,
    groq_api_key: str,
    local_model: str = "tiny.en",
    force_local: bool = False,
) -> TranscriptResult:
    """Use Groq Whisper first when enabled; fall back cleanly to local faster-whisper (ALN-02)."""
    if not force_local and groq_api_key:
        try:
            return GroqTranscriber(api_key=groq_api_key).transcribe(
                asr_path=asr_path,
                duration_ms=duration_ms,
                language=language,
                prompt_hint=prompt_hint,
                temp_dir=temp_dir,
            )
        except Exception as exc:
            logger.warning("Groq transcription failed (%s); falling back to local faster-whisper", exc)

    return LocalTranscriber(model_name=local_model).transcribe(
        asr_path=asr_path,
        duration_ms=duration_ms,
        language=language,
        prompt_hint=prompt_hint,
        temp_dir=temp_dir,
    )
