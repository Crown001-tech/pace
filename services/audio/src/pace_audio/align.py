"""
PACE Audio Worker — Needleman-Wunsch Alignment Engine (TDD Section 11)
Aligns spoken transcription words to script sections using 3-gram anchors + sub-window DP,
near-match phonetic/edit tolerance, pause snapping, confidence scoring, manual boundary preservation,
and script-accurate subtitle cue drafting.
"""

from __future__ import annotations

from bisect import bisect_left
from dataclasses import dataclass, field
import re
import unicodedata
from typing import Dict, List, Literal, Optional, Tuple

try:
    from num2words import num2words as _num2words
except ImportError:  # pragma: no cover - fallback when num2words isn't installed
    _num2words = None


@dataclass
class SpokenWord:
    word: str
    start_ms: int
    end_ms: int
    probability: float


@dataclass
class SectionInput:
    id: str
    position: int
    text: str


@dataclass
class SilenceGap:
    start_ms: int
    end_ms: int
    duration_ms: int


@dataclass
class BoundaryOutput:
    index: int
    time_ms: int
    confidence: float
    confidence_level: Literal["High", "Medium", "Low"]
    source: Literal["auto", "manual"] = "auto"
    flags: List[str] = field(default_factory=list)


@dataclass
class SubtitleCue:
    position: int
    start_ms: int
    end_ms: int
    text: str


@dataclass
class AlignmentOutput:
    boundaries: List[BoundaryOutput]
    subtitle_cues: List[SubtitleCue]
    overall_confidence: float
    flags: List[str]


NUMBER_WORDS = {
    0: "zero", 1: "one", 2: "two", 3: "three", 4: "four",
    5: "five", 6: "six", 7: "seven", 8: "eight", 9: "nine",
    10: "ten", 11: "eleven", 12: "twelve", 13: "thirteen", 14: "fourteen",
    15: "fifteen", 16: "sixteen", 17: "seventeen", 18: "eighteen", 19: "nineteen",
    20: "twenty", 30: "thirty", 40: "forty", 50: "fifty",
    60: "sixty", 70: "seventy", 80: "eighty", 90: "ninety",
}


def number_to_words(n: int) -> List[str]:
    if _num2words is not None and 0 <= n <= 1_000_000_000:
        try:
            if 1000 <= n <= 2099 and not (n % 1000 == 0):
                raw = _num2words(n, to="year")
            else:
                raw = _num2words(n)
            cleaned = re.sub(r"[^a-z0-9\s]+", " ", raw.lower())
            toks = [t for t in cleaned.split() if t and t != "and"]
            if toks:
                return toks
        except Exception:
            pass

    if n in NUMBER_WORDS:
        return [NUMBER_WORDS[n]]
    if n < 100:
        tens = (n // 10) * 10
        ones = n % 10
        return [NUMBER_WORDS[tens], NUMBER_WORDS[ones]]
    if 1000 <= n <= 2099:
        century = n // 100
        decade = n % 100
        return number_to_words(century) + number_to_words(decade)
    return [str(n)]


def normalize_text(text: str) -> List[str]:
    """Clean text per TDD 11.1: NFKC, plain quotes/dashes, lowercase, numbers to words."""
    nfkc = unicodedata.normalize("NFKC", text)
    nfkc = (
        nfkc.replace("’", "'")
        .replace("‘", "'")
        .replace("“", '"')
        .replace("”", '"')
    )
    tokens = re.split(r"[\s—–-]+", nfkc.lower())
    words: List[str] = []
    for token in tokens:
        clean = "".join(ch for ch in token if ch.isalnum() or ch == "'").strip("'")
        if not clean:
            continue
        if clean.isdigit():
            words.extend(number_to_words(int(clean)))
        else:
            words.append(clean)
    return words


def _sound_key(word: str) -> str:
    """Compact phonetic key so similar-sounding names/words earn near-match +1 (TDD 11.2)."""
    w = re.sub(r"[^a-z]", "", word.lower())
    if not w:
        return word
    w = re.sub(r"^kn", "n", w)
    w = re.sub(r"^wr", "r", w)
    w = w.replace("ph", "f").replace("ck", "k").replace("qu", "kw")
    first = w[0]
    rest = re.sub(r"[aeiouyh]+", "", w[1:])
    # Collapse doubled consonants
    collapsed = first + re.sub(r"(.)\1+", r"\1", rest)
    return collapsed[:6]


def _one_edit_away(a: str, b: str) -> bool:
    if len(a) < 4 or len(b) < 4 or abs(len(a) - len(b)) > 1:
        return False
    if len(a) == len(b):
        diffs = sum(1 for x, y in zip(a, b) if x != y)
        return diffs == 1
    shorter, longer = (a, b) if len(a) < len(b) else (b, a)
    i = j = diffs = 0
    while i < len(shorter) and j < len(longer):
        if shorter[i] == longer[j]:
            i += 1
            j += 1
        else:
            diffs += 1
            if diffs > 1:
                return False
            j += 1
    return True


def _pair_score(a: str, b: str) -> int:
    """TDD 11.2: exact +2, near-match +1, mismatch -1."""
    if a == b:
        return 2
    if _one_edit_away(a, b) or (_sound_key(a) == _sound_key(b) and len(a) >= 3 and len(b) >= 3):
        return 1
    return -1


def find_3gram_anchors(script: List[str], spoken: List[str]) -> List[Tuple[int, int]]:
    """Find unique 3-grams in both lists, ordered via Longest Increasing Subsequence (TDD 11.2)."""
    if len(script) < 3 or len(spoken) < 3:
        return []

    script_grams: Dict[str, List[int]] = {}
    for i in range(len(script) - 2):
        gram = " ".join(script[i : i + 3])
        script_grams.setdefault(gram, []).append(i)

    spoken_grams: Dict[str, List[int]] = {}
    for j in range(len(spoken) - 2):
        gram = " ".join(spoken[j : j + 3])
        spoken_grams.setdefault(gram, []).append(j)

    candidates: List[Tuple[int, int]] = []
    for gram, s_idx_list in script_grams.items():
        if len(s_idx_list) == 1:
            spk_list = spoken_grams.get(gram, [])
            if len(spk_list) == 1:
                candidates.append((s_idx_list[0], spk_list[0]))

    if not candidates:
        return []

    candidates.sort(key=lambda x: x[0])

    # Standard O(N log N) Longest Increasing Subsequence on spoken indices
    tails: List[int] = []
    tail_pos: List[int] = []
    prev = [-1] * len(candidates)

    for idx, (_, spk_idx) in enumerate(candidates):
        pos = bisect_left(tails, spk_idx)
        if pos == len(tails):
            tails.append(spk_idx)
            tail_pos.append(idx)
        else:
            tails[pos] = spk_idx
            tail_pos[pos] = idx
        if pos > 0:
            prev[idx] = tail_pos[pos - 1]

    lis_reversed: List[Tuple[int, int]] = []
    curr = tail_pos[-1]
    while curr != -1:
        lis_reversed.append(candidates[curr])
        curr = prev[curr]
    lis_reversed.reverse()
    return lis_reversed


def needleman_wunsch(a: List[str], b: List[str]) -> List[Tuple[int, int]]:
    """Needleman-Wunsch alignment returning matched `(idx_a, idx_b)` pairs."""
    n, m = len(a), len(b)
    if n == 0 or m == 0:
        return []

    # Guard against degenerate huge DP tables if a huge window lacks 3-gram anchors.
    if n * m > 400_000:
        return _banded_fallback_match(a, b)

    dp = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n + 1):
        dp[i][0] = -i
    for j in range(m + 1):
        dp[0][j] = -j

    for i in range(1, n + 1):
        ai = a[i - 1]
        row = dp[i]
        prev_row = dp[i - 1]
        for j in range(1, m + 1):
            sc = _pair_score(ai, b[j - 1])
            row[j] = max(prev_row[j - 1] + sc, prev_row[j] - 1, row[j - 1] - 1)

    matches: List[Tuple[int, int]] = []
    i, j = n, m
    while i > 0 and j > 0:
        sc = _pair_score(a[i - 1], b[j - 1])
        if dp[i][j] == dp[i - 1][j - 1] + sc:
            if sc > 0:
                matches.append((i - 1, j - 1))
            i -= 1
            j -= 1
        elif dp[i][j] == dp[i - 1][j] - 1:
            i -= 1
        else:
            j -= 1

    matches.reverse()
    return matches


def _banded_fallback_match(a: List[str], b: List[str], chunk: int = 120) -> List[Tuple[int, int]]:
    """Piecewise alignment for giant unanchored spans to keep memory and latency low."""
    matches: List[Tuple[int, int]] = []
    ratio = len(b) / max(1, len(a))
    for sa in range(0, len(a), chunk):
        ea = min(len(a), sa + chunk)
        sb = max(0, int(sa * ratio) - 40)
        eb = min(len(b), int(ea * ratio) + 40)
        for li, lj in needleman_wunsch(a[sa:ea], b[sb:eb]):
            matches.append((sa + li, sb + lj))
    return matches


def align_words_anchored(script_words: List[str], spoken_words: List[str]) -> Tuple[Dict[int, int], List[int]]:
    """
    Use 3-gram anchors to divide the problem into small pieces, then run Needleman-Wunsch between them.
    Returns `(script_idx -> norm_spoken_idx, anchor_script_indices)`.
    """
    anchors = find_3gram_anchors(script_words, spoken_words)
    match_map: Dict[int, int] = {}
    anchor_positions: List[int] = []

    prev_s = 0
    prev_spk = 0

    for sa, spka in anchors:
        if sa < prev_s or spka < prev_spk:
            continue
        if sa > prev_s and spka > prev_spk:
            for li, lj in needleman_wunsch(script_words[prev_s:sa], spoken_words[prev_spk:spka]):
                match_map[prev_s + li] = prev_spk + lj
        for offset in range(3):
            if sa + offset < len(script_words) and spka + offset < len(spoken_words):
                match_map[sa + offset] = spka + offset
                anchor_positions.append(sa + offset)
        prev_s = sa + 3
        prev_spk = spka + 3

    if prev_s < len(script_words) and prev_spk < len(spoken_words):
        for li, lj in needleman_wunsch(script_words[prev_s:], spoken_words[prev_spk:]):
            match_map[prev_s + li] = prev_spk + lj

    return match_map, anchor_positions


def _nearest_anchor_distance(idx: int, anchor_positions: List[int]) -> int:
    if not anchor_positions:
        return 300
    pos = bisect_left(anchor_positions, idx)
    best = 300
    if pos < len(anchor_positions):
        best = min(best, abs(anchor_positions[pos] - idx))
    if pos > 0:
        best = min(best, abs(anchor_positions[pos - 1] - idx))
    return best


def _local_speaking_ms_per_word(
    spoken_words: List[SpokenWord],
    center_idx: int,
    default_ms: float = 340.0,
) -> float:
    lo = max(0, center_idx - 5)
    hi = min(len(spoken_words), center_idx + 5)
    window = spoken_words[lo:hi]
    if len(window) < 2:
        return default_ms
    span = max(200, window[-1].end_ms - window[0].start_ms)
    return max(150.0, min(650.0, span / len(window)))


def calculate_boundaries(
    sections: List[SectionInput],
    spoken_words: List[SpokenWord],
    silences: List[SilenceGap],
    narration_duration_ms: int,
    aspect_ratio: str = "16:9",
    manual_boundaries: Optional[Dict[int, int]] = None,
) -> AlignmentOutput:
    """Calculate exact boundary cut times, snap to pauses, flag anomalies, and draft subtitles."""
    sorted_sections = sorted(sections, key=lambda s: s.position)
    if not sorted_sections:
        return AlignmentOutput(
            boundaries=[
                BoundaryOutput(index=0, time_ms=0, confidence=1.0, confidence_level="High"),
                BoundaryOutput(index=1, time_ms=max(0, narration_duration_ms), confidence=1.0, confidence_level="High"),
            ],
            subtitle_cues=[],
            overall_confidence=1.0,
            flags=[],
        )

    # Normalize spoken words while preserving exact index mapping back to `spoken_words`.
    norm_spoken: List[str] = []
    norm_to_orig_spk: List[int] = []
    for spk_idx, spk in enumerate(spoken_words):
        for tok in normalize_text(spk.word):
            norm_spoken.append(tok)
            norm_to_orig_spk.append(spk_idx)

    section_words_map: List[Tuple[SectionInput, List[str], int]] = []
    full_script_words: List[str] = []
    for sec in sorted_sections:
        words = normalize_text(sec.text)
        start_idx = len(full_script_words)
        full_script_words.extend(words)
        section_words_map.append((sec, words, start_idx))

    match_norm_map, anchor_positions = align_words_anchored(full_script_words, norm_spoken)
    # Map script_idx -> original SpokenWord index
    match_map: Dict[int, int] = {
        s_idx: norm_to_orig_spk[n_idx] for s_idx, n_idx in match_norm_map.items()
    }

    manual_map = manual_boundaries or {}
    num_sections = len(section_words_map)
    boundaries: List[BoundaryOutput] = [
        BoundaryOutput(index=0, time_ms=0, confidence=1.0, confidence_level="High", source="auto")
    ]
    global_flags: List[str] = []
    last_time_ms = 0
    prev_sec_last_spk_idx: Optional[int] = None

    # Track matched word boundaries for section 0 as well (for subtitle & prev_sec_last_spk_idx).
    _, sec0_words, sec0_start = section_words_map[0]
    for w_idx in range(len(sec0_words)):
        g_idx = sec0_start + w_idx
        if g_idx in match_map:
            prev_sec_last_spk_idx = match_map[g_idx]

    for k in range(1, num_sections):
        sec, words, start_idx = section_words_map[k]
        first_matched_local: Optional[int] = None
        first_spk_idx: Optional[int] = None
        last_spk_in_sec: Optional[int] = None
        matched_in_sec = 0

        for w_idx in range(len(words)):
            g_idx = start_idx + w_idx
            if g_idx in match_map:
                matched_in_sec += 1
                spk_i = match_map[g_idx]
                if first_spk_idx is None:
                    first_spk_idx = spk_i
                    first_matched_local = w_idx
                last_spk_in_sec = spk_i

        sec_flags: List[str] = []
        match_ratio = matched_in_sec / len(words) if words else 0.0
        unspoken = len(words) >= 3 and match_ratio < 0.2
        if unspoken:
            sec_flags.append("Section not found")

        # Determine initial cut time or keep user's manual cut as fixed point (TDD 11.4).
        is_manual = k in manual_map
        if is_manual:
            final_time_ms = max(0, min(narration_duration_ms, int(manual_map[k])))
            source: Literal["auto", "manual"] = "manual"
        elif unspoken:
            # Zero-length fallback at current cursor unless already after speech
            final_time_ms = last_time_ms
            source = "auto"
        elif first_spk_idx is not None and spoken_words:
            spk = spoken_words[first_spk_idx]
            lead_words = first_matched_local or 0
            ms_per_word = _local_speaking_ms_per_word(spoken_words, first_spk_idx)
            estimated_lead = min(1500, int(lead_words * ms_per_word)) if lead_words > 0 else 0
            initial_time_ms = max(0, spk.start_ms - estimated_lead - 150)

            # Gap before this section's first spoken word (> 4.0s -> Unexpected long gap)
            gap_before_ms = 0
            if prev_sec_last_spk_idx is not None and prev_sec_last_spk_idx < len(spoken_words):
                prev_end = spoken_words[prev_sec_last_spk_idx].end_ms
                gap_before_ms = max(0, spk.start_ms - prev_end)
            if gap_before_ms > 4000:
                sec_flags.append("Unexpected long gap")

            # Snap to nearest pause >= 150ms within +/- 600ms, never past section's first word (ALN-04)
            final_time_ms = initial_time_ms
            best_dist = 601
            for p in silences:
                if p.duration_ms >= 150:
                    p_mid = p.start_ms + p.duration_ms // 2
                    dist = abs(p_mid - initial_time_ms)
                    if dist <= 600 and p_mid <= spk.start_ms and p_mid > last_time_ms and dist < best_dist:
                        best_dist = dist
                        final_time_ms = p_mid
            source = "auto"
        else:
            final_time_ms = min(narration_duration_ms, last_time_ms + 2500)
            source = "auto"
            sec_flags.append("Low confidence")

        # Enforce strictly non-decreasing monotonic order (keeping manual boundaries intact when possible)
        if not unspoken and final_time_ms <= last_time_ms and not is_manual:
            final_time_ms = min(narration_duration_ms, last_time_ms + 500)
            if "Low confidence" not in sec_flags:
                sec_flags.append("Low confidence")
        elif is_manual and final_time_ms < last_time_ms:
            sec_flags.append("Low confidence")

        # Update previous scene flag if scene (k-1) is < 1.0s
        if 0 < final_time_ms - last_time_ms < 1000 and not unspoken:
            if "Very short scene" not in sec_flags:
                sec_flags.append("Very short scene")

        # TDD 11.3 confidence formula
        win_lo = max(0, start_idx - 8)
        win_hi = min(len(full_script_words), start_idx + 8)
        win_len = max(1, win_hi - win_lo)
        density = sum(1 for gi in range(win_lo, win_hi) if gi in match_map) / win_len
        anchor_dist = _nearest_anchor_distance(start_idx, anchor_positions)
        fixed_point_score = max(0.0, 1.0 - (anchor_dist / 300.0))

        conf = round(0.7 * density + 0.3 * fixed_point_score, 3)
        if is_manual:
            conf = max(conf, 0.95)
        elif density < 0.6 or anchor_dist > 150:
            if "Low confidence" not in sec_flags:
                sec_flags.append("Low confidence")

        conf_level: Literal["High", "Medium", "Low"] = (
            "High" if conf >= 0.85 else ("Medium" if conf >= 0.6 else "Low")
        )

        boundaries.append(
            BoundaryOutput(
                index=k,
                time_ms=final_time_ms,
                confidence=conf,
                confidence_level=conf_level,
                source=source,
                flags=sec_flags,
            )
        )
        global_flags.extend(sec_flags)
        last_time_ms = max(last_time_ms, final_time_ms)
        if last_spk_in_sec is not None:
            prev_sec_last_spk_idx = last_spk_in_sec

    # Final boundary is narration duration (TML-06)
    boundaries.append(
        BoundaryOutput(
            index=num_sections,
            time_ms=max(last_time_ms, narration_duration_ms),
            confidence=1.0,
            confidence_level="High",
            source="auto",
            flags=[],
        )
    )

    # Draft subtitle cues using script spelling + matched spoken word timestamps (TDD 11.5)
    subtitles: List[SubtitleCue] = []
    for k, (sec, words, start_idx) in enumerate(section_words_map):
        sec_start = boundaries[k].time_ms
        sec_end = boundaries[k + 1].time_ms
        if sec_end <= sec_start or not sec.text.strip():
            continue

        sec_word_times: List[Optional[Tuple[int, int]]] = []
        for w_idx in range(len(words)):
            spk_idx = match_map.get(start_idx + w_idx)
            if spk_idx is not None and spk_idx < len(spoken_words):
                sw = spoken_words[spk_idx]
                s_t = max(sec_start, min(sec_end, sw.start_ms))
                e_t = max(s_t + 50, min(sec_end, sw.end_ms))
                sec_word_times.append((s_t, e_t))
            else:
                sec_word_times.append(None)

        section_cues = generate_subtitles(
            sec.text,
            sec_start,
            sec_end,
            aspect_ratio=aspect_ratio,
            word_timings=sec_word_times,
            position_offset=len(subtitles),
        )
        subtitles.extend(section_cues)

    avg_conf = sum(b.confidence for b in boundaries) / len(boundaries)
    return AlignmentOutput(
        boundaries=boundaries,
        subtitle_cues=subtitles,
        overall_confidence=round(avg_conf, 3),
        flags=sorted(set(global_flags)),
    )


def _wrap_into_cue_texts(words: List[str], max_line: int) -> List[str]:
    """Split text into blocks of <= 2 lines x max_line chars without ever dropping words."""
    blocks: List[str] = []
    idx = 0
    while idx < len(words):
        line1 = ""
        line2 = ""
        while idx < len(words):
            w = words[idx]
            cand = f"{line1} {w}".strip()
            if len(cand) <= max_line or not line1:
                line1 = cand[:max_line] if not line1 and len(w) > max_line else cand
                idx += 1
                # Prefer natural punctuation break on line 1 if reasonable
                if len(line1) >= max_line * 0.65 and line1[-1] in ",;:.!?":
                    break
            else:
                break

        while idx < len(words):
            w = words[idx]
            cand = f"{line2} {w}".strip()
            if len(cand) <= max_line or not line2:
                line2 = cand[:max_line] if not line2 and len(w) > max_line else cand
                idx += 1
                if len(line2) >= max_line * 0.6 and line2[-1] in ".!?;:":
                    break
            else:
                break

        blocks.append(f"{line1}\n{line2}" if line2 else line1)
    return blocks


def generate_subtitles(
    text: str,
    start_ms: int,
    end_ms: int,
    aspect_ratio: str = "16:9",
    word_timings: Optional[List[Optional[Tuple[int, int]]]] = None,
    position_offset: int = 0,
) -> List[SubtitleCue]:
    """Generate subtitle cues per TDD 11.5: <= 2x42 chars (32 for 9:16), 1-7s each, script spelling."""
    max_line = 32 if aspect_ratio == "9:16" else 42
    raw_words = text.strip().split()
    if not raw_words or end_ms <= start_ms:
        return []

    # First split into sentence-level spans, then wrap each sentence into <= 2-line blocks.
    sentence_groups: List[List[str]] = []
    cur: List[str] = []
    for w in raw_words:
        cur.append(w)
        if w[-1] in ".!?" and len(cur) >= 3:
            sentence_groups.append(cur)
            cur = []
    if cur:
        sentence_groups.append(cur)

    cue_word_blocks: List[List[str]] = []
    for sent_words in sentence_groups:
        wrapped_strs = _wrap_into_cue_texts(sent_words, max_line)
        for block_str in wrapped_strs:
            cue_word_blocks.append(block_str.replace("\n", " ").split())

    # Assign start/end times per cue using aligned word timings when available, falling back smoothly.
    total_words = max(1, sum(len(b) for b in cue_word_blocks))
    total_dur = max(1, end_ms - start_ms)

    cues: List[SubtitleCue] = []
    norm_cursor = 0
    prev_cue_end = start_ms

    for block_idx, block_words in enumerate(cue_word_blocks):
        block_str = _wrap_into_cue_texts(block_words, max_line)[0]
        norm_count = sum(len(normalize_text(w)) for w in block_words) or len(block_words)

        matched_starts: List[int] = []
        matched_ends: List[int] = []
        if word_timings:
            for offset in range(norm_cursor, min(len(word_timings), norm_cursor + norm_count)):
                wt = word_timings[offset]
                if wt is not None:
                    matched_starts.append(wt[0])
                    matched_ends.append(wt[1])

        norm_cursor += norm_count

        if matched_starts and matched_ends:
            cue_start = max(prev_cue_end, matched_starts[0])
            cue_end = min(end_ms, max(cue_start + 800, matched_ends[-1] + 150))
        else:
            frac_s = block_idx / len(cue_word_blocks)
            frac_e = (block_idx + 1) / len(cue_word_blocks)
            cue_start = max(prev_cue_end, start_ms + int(round(frac_s * total_dur)))
            cue_end = min(end_ms, max(cue_start + 500, start_ms + int(round(frac_e * total_dur))))

        # Enforce max 7s duration unless last cue stretches slightly less
        if cue_end - cue_start > 7000:
            cue_end = cue_start + 7000

        if block_idx + 1 == len(cue_word_blocks) and cue_end < end_ms and (end_ms - cue_start) <= 7000:
            cue_end = max(cue_end, min(end_ms, cue_start + max(1000, int((len(block_words) / total_words) * total_dur))))

        if cue_end > cue_start:
            cues.append(
                SubtitleCue(
                    position=position_offset + len(cues),
                    start_ms=cue_start,
                    end_ms=cue_end,
                    text=block_str,
                )
            )
            prev_cue_end = cue_end

    return cues
