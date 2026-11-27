import test from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_MUSIC_SETTINGS,
  DEFAULT_SUBTITLE_STYLE,
  evaluateSubtitleCueRules,
  maxCharsForAspectRatio,
  wrapSubtitlePreviewLines,
} from "./subtitle-rules.ts";

test("enforces 42 chars/line for 16:9 and 32 chars/line for 9:16", () => {
  assert.equal(maxCharsForAspectRatio("16:9"), 42);
  assert.equal(maxCharsForAspectRatio("9:16"), 32);

  const lines16x9 = wrapSubtitlePreviewLines(
    "Before a single beam is lifted into place engineers map how wind and light move",
    "16:9"
  );
  assert.equal(lines16x9.length, 2);
  assert.equal(
    lines16x9.every((l) => l.length <= 42),
    true
  );

  const lines9x16 = wrapSubtitlePreviewLines(
    "Before a single beam is lifted into place engineers map",
    "9:16"
  );
  assert.equal(lines9x16.length, 2);
  assert.equal(
    lines9x16.every((l) => l.length <= 32),
    true
  );
});

test("flags subtitle cues that violate line count, char limits, or 1-7s duration bounds", () => {
  const valid = evaluateSubtitleCueRules(
    {
      id: "c1",
      position: 0,
      start_ms: 1000,
      end_ms: 4200,
      text: "At dawn the skyline changes one steel girder at a time.",
      edited: false,
    },
    "16:9"
  );
  assert.deepEqual(valid.violations, []);

  const tooShort = evaluateSubtitleCueRules(
    {
      id: "c2",
      position: 1,
      start_ms: 1000,
      end_ms: 1600,
      text: "Quick flash.",
      edited: false,
    },
    "16:9"
  );
  assert.equal(
    tooShort.violations.some((v) => v.includes("< 1.0s min")),
    true
  );

  const tooLong = evaluateSubtitleCueRules(
    {
      id: "c3",
      position: 2,
      start_ms: 1000,
      end_ms: 9500,
      text: "A very long held subtitle cue across nine seconds.",
      edited: false,
    },
    "16:9"
  );
  assert.equal(
    tooLong.violations.some((v) => v.includes("> 7.0s max")),
    true
  );
});

test("provides broadcast-safe default subtitle style and music ducking settings", () => {
  assert.equal(DEFAULT_SUBTITLE_STYLE.enabled, true);
  assert.equal(DEFAULT_SUBTITLE_STYLE.font, "sans");
  assert.equal(DEFAULT_SUBTITLE_STYLE.position, "bottom");
  assert.equal(DEFAULT_MUSIC_SETTINGS.ducking_enabled, true);
  assert.equal(DEFAULT_MUSIC_SETTINGS.volume_db, -18);
  assert.equal(DEFAULT_MUSIC_SETTINGS.duck_db, -10);
});
