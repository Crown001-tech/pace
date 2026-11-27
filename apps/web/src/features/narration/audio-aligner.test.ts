import test from 'node:test';
import assert from 'node:assert/strict';
import { alignScriptToAudio } from './audio-aligner.ts';

test('alignScriptToAudio correctly proportions cuts and subtitle cues across script sections', () => {
  const sections = [
    {
      id: 'sec-1',
      position: 0,
      text: 'Below the surface of the ocean lies total darkness. Water pressure is immense.',
    },
    {
      id: 'sec-2',
      position: 1,
      text: 'Despite the harsh conditions life thrives. Over ninety percent of creatures glow.',
    },
    {
      id: 'sec-3',
      position: 2,
      text: 'Towering hydrothermal vents spew superheated water into the sea.',
    },
  ];

  const totalDurationMs = 60000; // 60 seconds

  const result = alignScriptToAudio(sections, totalDurationMs);

  // Boundaries check
  assert.equal(result.boundaries.length, 4); // 0, Cut 1, Cut 2, End
  assert.equal(result.boundaries[0].timeMs, 0);
  assert.equal(result.boundaries[3].timeMs, 60000);

  // Monotonic cut times
  assert.ok(result.boundaries[1].timeMs > 10000 && result.boundaries[1].timeMs < 30000);
  assert.ok(result.boundaries[2].timeMs > result.boundaries[1].timeMs);

  // All boundaries should have High confidence
  assert.ok(result.boundaries.every((b) => b.confidenceLevel === 'High'));

  // Subtitle cues check
  assert.ok(result.subtitleCues.length >= 3);
  // Every cue must have positive duration
  result.subtitleCues.forEach((cue) => {
    assert.ok(cue.endMs > cue.startMs);
    assert.ok(cue.durationMs > 0);
  });
});

