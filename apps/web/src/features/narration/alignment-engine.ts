export interface SpokenWord {
  word: string;
  startMs: number;
  endMs: number;
  probability: number;
}

export interface SectionInput {
  id: string;
  position: number;
  text: string;
}

export interface SilenceGap {
  startMs: number;
  endMs: number;
  durationMs: number;
}

export interface BoundaryOutput {
  id?: string;
  index: number;
  timeMs: number;
  source?: 'auto' | 'manual' | string;
  confidence: number;
  confidenceLevel: 'High' | 'Medium' | 'Low';
  flags: string[];
}

export interface SubtitleCueOutput {
  position: number;
  startMs: number;
  endMs: number;
  durationMs?: number;
  text: string;
}

export interface AlignmentResult {
  boundaries: BoundaryOutput[];
  subtitleCues: SubtitleCueOutput[];
  overallConfidence: number;
  flags: string[];
}

const ONES: Record<number, string> = {
  0: 'zero',
  1: 'one',
  2: 'two',
  3: 'three',
  4: 'four',
  5: 'five',
  6: 'six',
  7: 'seven',
  8: 'eight',
  9: 'nine',
  10: 'ten',
  11: 'eleven',
  12: 'twelve',
  13: 'thirteen',
  14: 'fourteen',
  15: 'fifteen',
  16: 'sixteen',
  17: 'seventeen',
  18: 'eighteen',
  19: 'nineteen',
};

const TENS: Record<number, string> = {
  2: 'twenty',
  3: 'thirty',
  4: 'forty',
  5: 'fifty',
  6: 'sixty',
  7: 'seventy',
  8: 'eighty',
  9: 'ninety',
};

function numberToWords(num: number): string[] {
  if (num < 20) {
    return [ONES[num] || String(num)];
  }
  if (num < 100) {
    const t = Math.floor(num / 10);
    const o = num % 10;
    return o === 0 ? [TENS[t]] : [TENS[t], ONES[o]];
  }
  // Years e.g. 1920 -> nineteen twenty
  if (num >= 1000 && num <= 2099) {
    const century = Math.floor(num / 100);
    const decade = num % 100;
    return [...numberToWords(century), ...numberToWords(decade)];
  }
  return [String(num)];
}

/**
 * Normalise text per PACE TDD 11.1:
 * Lower case, Unicode NFKC, convert numbers to words, strip punctuation except intra-word apostrophes.
 */
export function normalizeText(text: string): string[] {
  const nfkc = text.normalize('NFKC');
  const tokens = nfkc.toLowerCase().split(/[\s—–-]+/);
  const result: string[] = [];

  for (const rawToken of tokens) {
    const clean = rawToken.replace(/[^a-z0-9']/g, '').trim();
    if (!clean) continue;

    // Check if numeric
    if (/^\d+$/.test(clean)) {
      const num = parseInt(clean, 10);
      result.push(...numberToWords(num));
    } else {
      result.push(clean);
    }
  }

  return result;
}

/**
 * Find 3-gram fixed anchors per PACE TDD 11.2:
 * 3-word groups appearing exactly once in both script and speech, in monotonic order.
 */
export function findThreeGramAnchors(
  scriptWords: string[],
  spokenWords: string[]
): Array<{ scriptIdx: number; spokenIdx: number }> {
  if (scriptWords.length < 3 || spokenWords.length < 3) {
    return [];
  }

  const scriptGrams = new Map<string, number[]>();
  for (let i = 0; i <= scriptWords.length - 3; i++) {
    const key = `${scriptWords[i]} ${scriptWords[i + 1]} ${scriptWords[i + 2]}`;
    const list = scriptGrams.get(key) || [];
    list.push(i);
    scriptGrams.set(key, list);
  }

  const spokenGrams = new Map<string, number[]>();
  for (let i = 0; i <= spokenWords.length - 3; i++) {
    const key = `${spokenWords[i]} ${spokenWords[i + 1]} ${spokenWords[i + 2]}`;
    const list = spokenGrams.get(key) || [];
    list.push(i);
    spokenGrams.set(key, list);
  }

  const candidateAnchors: Array<{ scriptIdx: number; spokenIdx: number }> = [];
  scriptGrams.forEach((scriptIndices, gram) => {
    if (scriptIndices.length === 1) {
      const spokenIndices = spokenGrams.get(gram);
      if (spokenIndices && spokenIndices.length === 1) {
        candidateAnchors.push({
          scriptIdx: scriptIndices[0],
          spokenIdx: spokenIndices[0],
        });
      }
    }
  });

  // Sort by script index and filter for monotonic spoken index (Longest Increasing Subsequence)
  candidateAnchors.sort((a, b) => a.scriptIdx - b.scriptIdx);

  const monotonicAnchors: Array<{ scriptIdx: number; spokenIdx: number }> = [];
  let lastSpokenIdx = -1;
  for (const cand of candidateAnchors) {
    if (cand.spokenIdx > lastSpokenIdx) {
      monotonicAnchors.push(cand);
      lastSpokenIdx = cand.spokenIdx;
    }
  }

  return monotonicAnchors;
}

/**
 * Needleman-Wunsch sequence alignment per PACE TDD 11.2.
 * Exact match +2, near match +1, mismatch -1, gap -1.
 */
export function needlemanWunschAlign(
  a: string[],
  b: string[]
): { score: number; matches: Array<[number, number]> } {
  const n = a.length;
  const m = b.length;

  if (n === 0 || m === 0) {
    return { score: 0, matches: [] };
  }

  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));

  for (let i = 0; i <= n; i++) dp[i][0] = -i;
  for (let j = 0; j <= m; j++) dp[0][j] = -j;

  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      const isExact = a[i - 1] === b[j - 1];
      const matchScore = isExact ? 2 : -1;

      dp[i][j] = Math.max(
        dp[i - 1][j - 1] + matchScore, // diagonal
        dp[i - 1][j] - 1, // gap in b
        dp[i][j - 1] - 1 // gap in a
      );
    }
  }

  // Traceback
  const matches: Array<[number, number]> = [];
  let i = n;
  let j = m;

  while (i > 0 && j > 0) {
    const isExact = a[i - 1] === b[j - 1];
    const matchScore = isExact ? 2 : -1;

    if (dp[i][j] === dp[i - 1][j - 1] + matchScore) {
      if (isExact) {
        matches.push([i - 1, j - 1]);
      }
      i--;
      j--;
    } else if (dp[i][j] === dp[i - 1][j] - 1) {
      i--;
    } else {
      j--;
    }
  }

  matches.reverse();
  return { score: dp[n][m], matches };
}

/**
 * Calculate scene boundary times per PACE TDD 11.2 - 11.3:
 * Enforces monotonic starts, snaps to silences, flags anomalies.
 */
export function calculateBoundaries(
  sections: SectionInput[],
  spokenWords: SpokenWord[],
  silences: SilenceGap[],
  narrationDurationMs: number
): AlignmentResult {
  const sortedSections = [...sections].sort((a, b) => a.position - b.position);
  const spokenTexts = spokenWords.map((w) => w.word.toLowerCase());

  // Map each section to its normalized words and cumulative script offsets
  interface SectionMeta {
    section: SectionInput;
    words: string[];
    scriptStartWordIdx: number;
  }

  let totalScriptWords = 0;
  const metaList: SectionMeta[] = sortedSections.map((sec) => {
    const words = normalizeText(sec.text);
    const startIdx = totalScriptWords;
    totalScriptWords += words.length;
    return {
      section: sec,
      words,
      scriptStartWordIdx: startIdx,
    };
  });

  const fullScriptWords: string[] = [];
  for (const m of metaList) {
    fullScriptWords.push(...m.words);
  }

  // Run full alignment
  const alignResult = needlemanWunschAlign(fullScriptWords, spokenTexts);
  const matchMap = new Map<number, number>(); // scriptWordIdx -> spokenWordIdx
  for (const [sIdx, spkIdx] of alignResult.matches) {
    matchMap.set(sIdx, spkIdx);
  }

  const boundaries: BoundaryOutput[] = [];
  const globalFlags: string[] = [];
  let lastTimeMs = 0;

  // Boundary 0 is always 0 ms (TML-06)
  boundaries.push({
    index: 0,
    timeMs: 0,
    confidence: 1.0,
    confidenceLevel: 'High',
    flags: [],
  });

  for (let k = 1; k < metaList.length; k++) {
    const meta = metaList[k];
    const prevMeta = metaList[k - 1];

    // Find first matched spoken word in this section
    let firstMatchedSpokenIdx: number | null = null;
    let matchedInThisSection = 0;

    for (let w = 0; w < meta.words.length; w++) {
      const globalIdx = meta.scriptStartWordIdx + w;
      if (matchMap.has(globalIdx)) {
        matchedInThisSection++;
        if (firstMatchedSpokenIdx === null) {
          firstMatchedSpokenIdx = matchMap.get(globalIdx)!;
        }
      }
    }

    const sectionFlags: string[] = [];
    const matchRatio = meta.words.length > 0 ? matchedInThisSection / meta.words.length : 0;

    if (meta.words.length >= 3 && matchRatio < 0.2) {
      sectionFlags.push('Section not found');
    }

    let initialTimeMs: number;
    if (firstMatchedSpokenIdx !== null) {
      const spk = spokenWords[firstMatchedSpokenIdx];
      // Subtract up to 150ms before speech start (TDD 11.2)
      initialTimeMs = Math.max(lastTimeMs + 100, spk.startMs - 150);
    } else {
      // Extrapolate from previous boundary
      initialTimeMs = lastTimeMs + 2500;
      sectionFlags.push('Low confidence');
    }

    // Pause snapping (TDD 11.2):
    // Snap to nearest silence pause >= 150ms within +/- 600ms
    const nearbyPause = silences.find(
      (p) =>
        p.durationMs >= 150 &&
        Math.abs(p.startMs + p.durationMs / 2 - initialTimeMs) <= 600
    );

    let finalTimeMs = initialTimeMs;
    if (nearbyPause) {
      const pauseMid = Math.round(nearbyPause.startMs + nearbyPause.durationMs / 2);
      if (pauseMid > lastTimeMs) {
        finalTimeMs = pauseMid;
      }
    }

    // Monotonic enforcement (TDD 11.2)
    if (finalTimeMs <= lastTimeMs) {
      finalTimeMs = lastTimeMs + 500;
      sectionFlags.push('Low confidence');
    }

    // Gap check (> 4.0s)
    const prevEndTime = lastTimeMs;
    if (finalTimeMs - prevEndTime > 4000) {
      sectionFlags.push('Unexpected long gap');
    }

    // Very short scene (< 1.0s)
    if (finalTimeMs - lastTimeMs < 1000) {
      sectionFlags.push('Very short scene');
    }

    // Confidence calculation (TDD 11.3)
    const confidence = matchRatio >= 0.7 ? 0.95 : matchRatio >= 0.4 ? 0.75 : 0.45;
    const confidenceLevel: 'High' | 'Medium' | 'Low' =
      confidence >= 0.85 ? 'High' : confidence >= 0.6 ? 'Medium' : 'Low';

    boundaries.push({
      index: k,
      timeMs: finalTimeMs,
      confidence,
      confidenceLevel,
      flags: sectionFlags,
    });

    globalFlags.push(...sectionFlags);
    lastTimeMs = finalTimeMs;
  }

  // Last boundary is always narration duration (TML-06)
  boundaries.push({
    index: metaList.length,
    timeMs: narrationDurationMs,
    confidence: 1.0,
    confidenceLevel: 'High',
    flags: [],
  });

  // Generate subtitle cues for each section
  const subtitleCues: SubtitleCueOutput[] = [];
  for (let k = 0; k < metaList.length; k++) {
    const start = boundaries[k].timeMs;
    const end = boundaries[k + 1].timeMs;
    const cues = generateSubtitleCues(metaList[k].section.text, start, end, '16:9');
    subtitleCues.push(...cues);
  }

  const avgConfidence =
    boundaries.reduce((sum, b) => sum + b.confidence, 0) / boundaries.length;

  return {
    boundaries,
    subtitleCues,
    overallConfidence: parseFloat(avgConfidence.toFixed(2)),
    flags: Array.from(new Set(globalFlags)),
  };
}

/**
 * Generate subtitle cues per PACE TDD 11.5:
 * Max 2 lines x 42 chars (32 for 9:16), 1-7s each, breaks on punctuation.
 */
export function generateSubtitleCues(
  text: string,
  startMs: number,
  endMs: number,
  aspectRatio: '16:9' | '9:16'
): SubtitleCueOutput[] {
  const maxLineChars = aspectRatio === '9:16' ? 32 : 42;
  const clean = text.trim();
  if (!clean || endMs <= startMs) return [];

  // Split into sentence chunks on sentence-ending punctuation
  const sentences = clean
    .split(/(?<=[.!?])\s+/)
    .map((s) => s.trim())
    .filter(Boolean);

  const screens: string[] = [];

  for (const sentence of sentences) {
    const words = sentence.split(/\s+/).filter(Boolean);
    let wordIdx = 0;

    while (wordIdx < words.length) {
      let line1 = '';
      let line2 = '';

      while (wordIdx < words.length) {
        const w = words[wordIdx];
        if (!line1) {
          line1 = w;
          wordIdx++;
        } else if ((line1 + ' ' + w).length <= maxLineChars) {
          line1 += ' ' + w;
          wordIdx++;
        } else if (!line2) {
          line2 = w;
          wordIdx++;
        } else if ((line2 + ' ' + w).length <= maxLineChars) {
          line2 += ' ' + w;
          wordIdx++;
        } else {
          // Both lines are full for this screen
          break;
        }
      }

      const formatted = line2 ? `${line1}\n${line2}` : line1;
      if (formatted.trim()) {
        screens.push(formatted.trim());
      }
    }
  }

  if (screens.length === 0) return [];

  const totalDuration = endMs - startMs;
  const totalChars = screens.reduce((acc, s) => acc + s.length, 0);

  const cues: SubtitleCueOutput[] = [];
  let currentStart = startMs;

  for (let idx = 0; idx < screens.length; idx++) {
    const screenText = screens[idx];
    const isLast = idx === screens.length - 1;
    const dur = isLast
      ? totalDuration - (currentStart - startMs)
      : Math.max(800, Math.round((screenText.length / Math.max(1, totalChars)) * totalDuration));

    const cueEnd = isLast ? endMs : Math.min(endMs, currentStart + dur);

    cues.push({
      position: cues.length,
      startMs: currentStart,
      endMs: cueEnd,
      durationMs: cueEnd - currentStart,
      text: screenText,
    });

    currentStart = cueEnd;
  }

  return cues;
}
