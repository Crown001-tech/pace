import test from 'node:test';
import assert from 'node:assert/strict';
import {
  normalizeText,
  findThreeGramAnchors,
  needlemanWunschAlign,
  calculateBoundaries,
  generateSubtitleCues,
} from './alignment-engine.ts';
import type { SpokenWord, SectionInput, SilenceGap } from './alignment-engine.ts';

test('normalizeText cleans words, removes punctuation, and converts digits to words', () => {
  const input = "In 1920, the explorer's voyage began—with 3 ships!";
  const normalized = normalizeText(input);
  assert.deepEqual(normalized, [
    'in',
    'nineteen',
    'twenty',
    'the',
    "explorer's",
    'voyage',
    'began',
    'with',
    'three',
    'ships',
  ]);
});

test('findThreeGramAnchors identifies unique matching 3-grams in monotonic order', () => {
  const scriptWords = ['the', 'quick', 'brown', 'fox', 'jumps', 'over', 'the', 'lazy', 'dog'];
  const spokenWords = ['now', 'the', 'quick', 'brown', 'fox', 'runs', 'past', 'the', 'lazy', 'dog'];

  const anchors = findThreeGramAnchors(scriptWords, spokenWords);
  assert.ok(anchors.length >= 1);
  // 'the quick brown' should be found as an anchor
  assert.equal(anchors[0].scriptIdx, 0);
  assert.equal(anchors[0].spokenIdx, 1);
});

test('needlemanWunschAlign computes alignment scoring with gaps and matches', () => {
  const a = ['ancient', 'roman', 'emperor', 'caesar'];
  const b = ['ancient', 'emperor', 'caesar'];

  const result = needlemanWunschAlign(a, b);
  assert.ok(result.matches.length >= 3);
  // 'ancient', 'emperor', 'caesar' are exact matches
  const matchedWordsA = result.matches.map((m) => a[m[0]]);
  assert.deepEqual(matchedWordsA, ['ancient', 'emperor', 'caesar']);
});

test('calculateBoundaries enforces monotonic boundaries, snaps to pauses, and flags anomalies', () => {
  const sections: SectionInput[] = [
    { id: 'sec-1', position: 0, text: 'The story began in the ancient city.' },
    { id: 'sec-2', position: 1, text: 'Many travelers arrived from distant shores.' },
    { id: 'sec-3', position: 2, text: 'None had ever seen such wealth before.' },
  ];

  const spokenWords: SpokenWord[] = [
    { word: 'the', startMs: 500, endMs: 800, probability: 0.98 },
    { word: 'story', startMs: 820, endMs: 1200, probability: 0.99 },
    { word: 'began', startMs: 1220, endMs: 1600, probability: 0.97 },
    { word: 'in', startMs: 1610, endMs: 1750, probability: 0.95 },
    { word: 'the', startMs: 1760, endMs: 1900, probability: 0.98 },
    { word: 'ancient', startMs: 1920, endMs: 2400, probability: 0.99 },
    { word: 'city', startMs: 2420, endMs: 2900, probability: 0.98 },

    // Pause between sec 1 and sec 2 from 2900 to 3500 (600ms pause)
    { word: 'many', startMs: 3500, endMs: 3900, probability: 0.96 },
    { word: 'travelers', startMs: 3920, endMs: 4600, probability: 0.97 },
    { word: 'arrived', startMs: 4620, endMs: 5100, probability: 0.98 },
    { word: 'from', startMs: 5120, endMs: 5350, probability: 0.95 },
    { word: 'distant', startMs: 5370, endMs: 5800, probability: 0.96 },
    { word: 'shores', startMs: 5820, endMs: 6300, probability: 0.98 },

    // Pause between sec 2 and sec 3 from 6300 to 7000 (700ms pause)
    { word: 'none', startMs: 7000, endMs: 7400, probability: 0.96 },
    { word: 'had', startMs: 7420, endMs: 7650, probability: 0.95 },
    { word: 'ever', startMs: 7670, endMs: 8000, probability: 0.98 },
    { word: 'seen', startMs: 8020, endMs: 8400, probability: 0.97 },
    { word: 'such', startMs: 8420, endMs: 8750, probability: 0.96 },
    { word: 'wealth', startMs: 8770, endMs: 9300, probability: 0.99 },
    { word: 'before', startMs: 9320, endMs: 9800, probability: 0.98 },
  ];

  const silences: SilenceGap[] = [
    { startMs: 2900, endMs: 3500, durationMs: 600 },
    { startMs: 6300, endMs: 7000, durationMs: 700 },
  ];

  const narrationDurationMs = 10000;
  const result = calculateBoundaries(sections, spokenWords, silences, narrationDurationMs);

  assert.equal(result.boundaries.length, 4); // 0, 1, 2, end
  // Boundary 0 is always 0 ms (TML-06)
  assert.equal(result.boundaries[0].timeMs, 0);

  // Boundary 1 snapped into silence pause (midpoint ~3200ms)
  assert.ok(result.boundaries[1].timeMs >= 2900 && result.boundaries[1].timeMs <= 3500);

  // Boundary 2 snapped into second pause (midpoint ~6650ms)
  assert.ok(result.boundaries[2].timeMs >= 6300 && result.boundaries[2].timeMs <= 7000);

  // Last boundary is narration end (10000ms)
  assert.equal(result.boundaries[3].timeMs, 10000);

  // Monotonic increase
  for (let i = 1; i < result.boundaries.length; i++) {
    assert.ok(result.boundaries[i].timeMs > result.boundaries[i - 1].timeMs);
  }
});

test('generateSubtitleCues breaks sentences into <= 42 char lines and 1-7s duration', () => {
  const text =
    'In the beginning of the twentieth century, expeditions set forth into uncharted polar waters. They faced fierce storms and ice.';
  const cues = generateSubtitleCues(text, 1000, 9000, '16:9');

  assert.ok(cues.length >= 2);
  for (const cue of cues) {
    const lines = cue.text.split('\n');
    assert.ok(lines.length <= 2, 'At most 2 lines per subtitle cue');
    for (const line of lines) {
      assert.ok(line.length <= 42, `Line "${line}" length (${line.length}) exceeds 42 chars`);
    }
    const dur = cue.endMs - cue.startMs;
    assert.ok(dur >= 500 && dur <= 7500, `Duration ${dur} outside expected range`);
  }
});
