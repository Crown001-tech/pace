import assert from "node:assert/strict";
import test from "node:test";
import { parsePromptsText } from "../dist/content/queue-runner.js";

test("parsePromptsText splits multi-paragraph script into ordered scene items", () => {
  const input = "Lavender blooms in sunlight.\n\nRosemary thrives in dry soil.\n\nBasil on a sunny sill.";
  const items = parsePromptsText(input, "image");
  assert.equal(items.length, 3);
  assert.equal(items[0].label, "S01");
  assert.equal(items[0].mode, "image");
  assert.equal(items[1].prompt, "Rosemary thrives in dry soil.");
  assert.equal(items[2].label, "S03");
});

test("parsePromptsText splits single-line prompts when no blank lines exist", () => {
  const input = "First video prompt\nSecond video prompt";
  const items = parsePromptsText(input, "video");
  assert.equal(items.length, 2);
  assert.equal(items[0].mode, "video");
  assert.equal(items[1].prompt, "Second video prompt");
});
