import test from "node:test";
import assert from "node:assert/strict";
import {
  estimateExportStats,
  formatDurationShort,
  formatRenderStage,
  getResolutionOptions,
} from "./export-utils.ts";

test("getResolutionOptions maps all 8 resolution and orientation combinations with even pixels", () => {
  const landscape = getResolutionOptions("16:9");
  assert.equal(landscape.length, 4);
  assert.deepEqual(
    landscape.map((r) => [r.id, r.width, r.height]),
    [
      ["360p", 640, 360],
      ["480p", 854, 480],
      ["720p", 1280, 720],
      ["1080p", 1920, 1080],
    ]
  );

  const portrait = getResolutionOptions("9:16");
  assert.deepEqual(
    portrait.map((r) => [r.id, r.width, r.height]),
    [
      ["360p", 360, 640],
      ["480p", 480, 854],
      ["720p", 720, 1280],
      ["1080p", 1080, 1920],
    ]
  );
});

test("estimateExportStats scales size and render time with resolution and duration", () => {
  const est720 = estimateExportStats("720p", 60_000);
  const est1080 = estimateExportStats("1080p", 60_000);
  assert.ok(est720.estimatedBytes > 10_000_000);
  assert.ok(est1080.estimatedBytes > est720.estimatedBytes);
  assert.ok(est1080.estimatedRenderSeconds > est720.estimatedRenderSeconds);
  assert.equal(formatDurationShort(125), "2m 5s");
});

test("formatRenderStage formats worker stage identifiers cleanly", () => {
  assert.equal(formatRenderStage("planning_segments"), "Planning scene segments");
  assert.equal(
    formatRenderStage("rendering_segment_3_of_9"),
    "Rendering segment 3 of 9"
  );
  assert.equal(
    formatRenderStage("validating_export"),
    "Probing streams & frame timing"
  );
});
