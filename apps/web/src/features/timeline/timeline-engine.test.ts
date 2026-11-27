import test from 'node:test';
import assert from 'node:assert/strict';
import {
  snapToFrame,
  snapToPause,
  moveBoundary,
  parseTimestamp,
  formatTimestampPrecise,
  calculateSceneDurations,
  getEffectivePauseSnap,
  extractFlaggedCuts,
} from './timeline-engine.ts';

test('snapToFrame rounds milliseconds to exact 30 fps frame grid ticks', () => {
  // At 30 fps, frame period is 1000/30 = 33.3333... ms
  // Frame 0: 0ms, Frame 1: 33ms, Frame 2: 67ms, Frame 3: 100ms, Frame 30: 1000ms
  assert.equal(snapToFrame(0, 30), 0);
  assert.equal(snapToFrame(10, 30), 0); // 10ms is closer to Frame 0 (0ms) than Frame 1 (33ms)
  assert.equal(snapToFrame(20, 30), 33); // 20ms is closer to Frame 1 (33ms)
  assert.equal(snapToFrame(45, 30), 33);
  assert.equal(snapToFrame(55, 30), 67);
  assert.equal(snapToFrame(100, 30), 100);
  assert.equal(snapToFrame(1000, 30), 1000);
});

test('snapToPause snaps to silence midpoint when within threshold', () => {
  const silences = [
    { startMs: 2000, endMs: 2600, durationMs: 600 }, // Midpoint = 2300ms
    { startMs: 5000, endMs: 5400, durationMs: 400 }, // Midpoint = 5200ms
  ];

  // Within 150ms of 2300ms -> snaps to 2300ms
  assert.equal(snapToPause(2250, silences, 150), 2300);
  assert.equal(snapToPause(2380, silences, 150), 2300);

  // Beyond 150ms of 2300ms -> remains unchanged
  assert.equal(snapToPause(2100, silences, 150), 2100);
  assert.equal(snapToPause(2500, silences, 150), 2500);

  // Near second silence (5200ms)
  assert.equal(snapToPause(5230, silences, 150), 5200);
});

test('moveBoundary maintains local boundary invariant: only modifies target boundary', () => {
  // 4 scenes, 5 boundaries: 0ms (start), 5000ms (cut 1), 12000ms (cut 2), 18000ms (cut 3), 25000ms (end)
  const initialBoundaries = [
    { index: 0, timeMs: 0 },
    { index: 1, timeMs: 5000 },
    { index: 2, timeMs: 12000 },
    { index: 3, timeMs: 18000 },
    { index: 4, timeMs: 25000 },
  ];

  // Drag cut 2 (boundary index 2) from 12000ms to 14000ms
  const result = moveBoundary({
    boundaries: initialBoundaries,
    index: 2,
    targetTimeMs: 14000,
    totalDurationMs: 25000,
    fps: 30,
    minDurationMs: 500,
  });

  // Boundary 2 updated
  assert.ok(result.boundaries[2].timeMs > 13900 && result.boundaries[2].timeMs <= 14033);

  // Boundary 0, 1, 3, 4 are UNTOUCHED
  assert.equal(result.boundaries[0].timeMs, 0);
  assert.equal(result.boundaries[1].timeMs, 5000);
  assert.equal(result.boundaries[3].timeMs, 18000);
  assert.equal(result.boundaries[4].timeMs, 25000);

  // Scene 1 (0 to 5000) and Scene 4 (18000 to 25000) durations are UNCHANGED
  const durations = calculateSceneDurations(result.boundaries);
  assert.equal(durations[0], 5000);
  assert.equal(durations[3], 7000);

  // Total duration remains strictly 25000ms (zero ripple drift)
  const totalDuration = durations.reduce((a, b) => a + b, 0);
  assert.equal(totalDuration, 25000);
});

test('moveBoundary strictly enforces 0.5s (500ms) minimum scene duration invariant', () => {
  const boundaries = [
    { index: 0, timeMs: 0 },
    { index: 1, timeMs: 4000 },
    { index: 2, timeMs: 8000 },
    { index: 3, timeMs: 12000 },
  ];

  // Try to drag cut 1 backwards into cut 0 (past 500ms)
  const clampedLeft = moveBoundary({
    boundaries,
    index: 1,
    targetTimeMs: 200, // < 500ms
    totalDurationMs: 12000,
    fps: 30,
    minDurationMs: 500,
  });
  // Must clamp to >= 500ms
  assert.ok(clampedLeft.boundaries[1].timeMs >= 500);

  // Try to drag cut 1 forward into cut 2 (past 8000 - 500 = 7500ms)
  const clampedRight = moveBoundary({
    boundaries,
    index: 1,
    targetTimeMs: 7900, // > 7500ms
    totalDurationMs: 12000,
    fps: 30,
    minDurationMs: 500,
  });
  // Must clamp to <= 7500ms
  assert.ok(clampedRight.boundaries[1].timeMs <= 7500);
});

test('parseTimestamp accurately parses mm:ss and mm:ss.mmm into milliseconds', () => {
  assert.equal(parseTimestamp('00:05'), 5000);
  assert.equal(parseTimestamp('01:30'), 90000);
  assert.equal(parseTimestamp('01:30.500'), 90500);
  assert.equal(parseTimestamp('02:15.25'), 135250);
  assert.equal(parseTimestamp('10:00.033'), 600033);

  // Invalid formats return null
  assert.equal(parseTimestamp('invalid'), null);
  assert.equal(parseTimestamp('12:88'), null); // Invalid seconds
  assert.equal(parseTimestamp('-01:00'), null);
});

test('formatTimestampPrecise formats milliseconds to exact mm:ss.mmm string', () => {
  assert.equal(formatTimestampPrecise(0), '00:00.000');
  assert.equal(formatTimestampPrecise(5000), '00:05.000');
  assert.equal(formatTimestampPrecise(65432), '01:05.432');
  assert.equal(formatTimestampPrecise(600000), '10:00.000');
});

test('getEffectivePauseSnap inverts pause snapping while Alt is held', () => {
  assert.equal(getEffectivePauseSnap(true, false), true);
  assert.equal(getEffectivePauseSnap(true, true), false);
  assert.equal(getEffectivePauseSnap(false, false), false);
  assert.equal(getEffectivePauseSnap(false, true), true);
});

test('extractFlaggedCuts collects flagged cuts in chronological boundary order', () => {
  const flags = extractFlaggedCuts(
    [
      { index: 0, timeMs: 0, flags: [] },
      { index: 1, timeMs: 4200, flags: ['Low confidence', 'Unexpected long gap'], confidence: 0.35 },
      { index: 2, timeMs: 9100, flags: [], confidence: 0.92 },
      { index: 3, timeMs: 12000, flags: ['Very short scene'], confidence: 0.78 },
      { index: 4, timeMs: 16000, flags: [] },
    ],
    ['Intro text', 'Second scene narration', 'Third scene narration', 'Fourth scene']
  );

  assert.equal(flags.length, 2);
  assert.equal(flags[0].boundaryIndex, 1);
  assert.equal(flags[0].sceneIndex, 1);
  assert.equal(flags[0].timeMs, 4200);
  assert.deepEqual(flags[0].flags, ['Low confidence', 'Unexpected long gap']);
  assert.equal(flags[1].boundaryIndex, 3);
  assert.equal(flags[1].sceneIndex, 3);
});

test('250-scene 40-minute timeline moveBoundary preserves exact duration and zero drift', () => {
  const totalDurationMs = 2_400_000; // 40 minutes
  const sceneCount = 250;
  const slice = totalDurationMs / sceneCount;
  const boundaries = Array.from({ length: sceneCount + 1 }, (_, i) => ({
    index: i,
    timeMs: Math.round(i * slice),
  }));

  const targetIdx = 124;
  const moved = moveBoundary({
    boundaries,
    index: targetIdx,
    targetTimeMs: boundaries[targetIdx].timeMs + 1400,
    totalDurationMs,
    fps: 30,
    minDurationMs: 500,
  });

  const beforeDurations = calculateSceneDurations(boundaries);
  const afterDurations = calculateSceneDurations(moved.boundaries);
  assert.equal(
    afterDurations.reduce((a, b) => a + b, 0),
    totalDurationMs
  );

  for (let i = 0; i < sceneCount; i++) {
    if (i !== targetIdx - 1 && i !== targetIdx) {
      assert.equal(afterDurations[i], beforeDurations[i]);
    }
  }
});
