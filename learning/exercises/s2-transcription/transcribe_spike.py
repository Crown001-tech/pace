"""
Experiment S2 — Transcription Spike (Milestone 0)
1. Generates a ~60-second spoken English MP3 narration fixture (if not present).
2. Sends it to Groq Whisper (whisper-large-v3-turbo) for word-level timestamps.
3. Transcribes the same file locally on CPU using faster-whisper (small.en, int8).
4. Prints word timings and compares speed between Groq and local CPU.
"""

import os
import subprocess
import time
from pathlib import Path
import httpx
import numpy as np

SAMPLE_SCRIPT = (
    "Welcome to PACE, the automated narrated video production engine. "
    "When a solo creator writes a documentary script, they break their story into clear paragraphs. "
    "Each paragraph becomes one visual scene on the timeline, paired with a single illustration. "
    "Instead of dragging dozens of clips by hand in a complex video editor, "
    "PACE listens to the voiceover recording and finds the exact moment each paragraph begins. "
    "When a natural pause of at least one hundred and fifty milliseconds appears near a boundary, "
    "the scene change snaps cleanly into the middle of that silence. "
    "Under the hood, a pure Rust engine calculates thirty frames per second of smooth camera motion, "
    "compiled both natively for the render worker and to WebAssembly for the browser preview. "
    "This guarantees that what you see in the browser matches your final exported video frame for frame."
)


def load_env(env_path: Path) -> None:
    if not env_path.exists():
        return
    for line in env_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        os.environ.setdefault(k.strip(), v.strip())


def ensure_sample_mp3(mp3_path: Path) -> None:
    if mp3_path.exists() and mp3_path.stat().st_size > 10_000:
        return
    mp3_path.parent.mkdir(parents=True, exist_ok=True)
    from gtts import gTTS

    raw_mp3 = mp3_path.with_suffix(".raw.mp3")
    tts = gTTS(text=SAMPLE_SCRIPT, lang="en", slow=False)
    tts.save(str(raw_mp3))
    subprocess.run(
        [
            "ffmpeg", "-y", "-i", str(raw_mp3),
            "-ac", "1", "-ar", "16000", "-b:a", "32k",
            str(mp3_path),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    raw_mp3.unlink(missing_ok=True)


def decode_pcm_16k_mono(mp3_path: Path) -> np.ndarray:
    """Decode audio via FFmpeg into a 16 kHz mono float32 numpy array."""
    proc = subprocess.run(
        [
            "ffmpeg", "-v", "error", "-i", str(mp3_path),
            "-f", "f32le", "-acodec", "pcm_f32le", "-ac", "1", "-ar", "16000", "-",
        ],
        check=True,
        capture_output=True,
    )
    return np.frombuffer(proc.stdout, dtype=np.float32)


def run_groq_transcription(mp3_path: Path, api_key: str) -> tuple[float, list[dict], float]:
    t0 = time.perf_counter()
    with mp3_path.open("rb") as f:
        response = httpx.post(
            "https://api.groq.com/openai/v1/audio/transcriptions",
            headers={
                "Authorization": f"Bearer {api_key}",
                "User-Agent": "pace-audio-worker/0.1",
            },
            data={
                "model": "whisper-large-v3-turbo",
                "response_format": "verbose_json",
                "timestamp_granularities[]": "word",
                "language": "en",
            },
            files={"file": (mp3_path.name, f, "audio/mpeg")},
            timeout=60.0,
        )
    if response.status_code != 200:
        raise RuntimeError(f"Groq HTTP {response.status_code}: {response.text[:200]}")
    elapsed = time.perf_counter() - t0
    payload = response.json()
    return elapsed, payload.get("words", []), float(payload.get("duration", 0.0))


def run_local_faster_whisper(mp3_path: Path) -> tuple[float, float, list[dict], float]:
    from faster_whisper import WhisperModel

    t_load0 = time.perf_counter()
    model = WhisperModel("small.en", device="cpu", compute_type="int8", cpu_threads=4)
    load_s = time.perf_counter() - t_load0

    t0 = time.perf_counter()
    audio_pcm = decode_pcm_16k_mono(mp3_path)
    duration_s = len(audio_pcm) / 16000.0
    segments, _info = model.transcribe(
        audio_pcm,
        language="en",
        word_timestamps=True,
        vad_filter=True,
    )
    words = []
    for seg in segments:
        for w in seg.words or []:
            words.append({"word": w.word.strip(), "start": w.start, "end": w.end, "prob": w.probability})
    elapsed = time.perf_counter() - t0
    return load_s, elapsed, words, duration_s


def main() -> None:
    repo_root = Path(__file__).resolve().parents[3]
    load_env(repo_root / ".env")
    api_key = os.environ.get("GROQ_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("GROQ_API_KEY is not set in .env")

    mp3_path = repo_root / "fixtures" / "audio" / "sample_1min_asr.mp3"
    print("1) Preparing ~60s spoken MP3 fixture (16 kHz mono 32 kbps)...")
    ensure_sample_mp3(mp3_path)
    print(f"   Audio ready: {mp3_path.name} ({mp3_path.stat().st_size} bytes)")

    print("\n2) Transcribing via Groq API (whisper-large-v3-turbo)...")
    groq_s = None
    duration_s = 79.78
    try:
        groq_s, groq_words, duration_s = run_groq_transcription(mp3_path, api_key)
        print(f"   Audio duration : {duration_s:.2f}s")
        print(f"   Groq time      : {groq_s:.2f}s ({len(groq_words)} words)")
        print("   First 8 words from Groq:")
        for w in groq_words[:8]:
            print(f"     [{w['start']:5.2f}s -> {w['end']:5.2f}s] {w['word']}")
    except Exception as exc:
        print(f"   Groq call error (falling back to local per ALN-02): {exc}")

    print("\n3) Transcribing locally on CPU via faster-whisper (small.en, int8)...")
    load_s, local_s, local_words, duration_s = run_local_faster_whisper(mp3_path)
    print(f"   Model load/init: {load_s:.2f}s")
    print(f"   Local transcribe: {local_s:.2f}s ({len(local_words)} words)")
    print("   First 8 words from local faster-whisper:")
    for w in local_words[:8]:
        print(f"     [{w['start']:5.2f}s -> {w['end']:5.2f}s] {w['word']} (p={w['prob']:.2f})")

    print("\n=== Experiment S2 Summary ===")
    print(f"Audio length       : {duration_s:.1f}s")
    if groq_s is not None:
        print(f"Groq Whisper API   : {groq_s:.2f}s ({duration_s / max(groq_s, 0.01):.1f}x real-time)")
    print(f"Local faster-whisper: {local_s:.2f}s ({duration_s / max(local_s, 0.01):.1f}x real-time)")


if __name__ == "__main__":
    main()
