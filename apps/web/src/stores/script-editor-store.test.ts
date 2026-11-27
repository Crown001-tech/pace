import test from "node:test";
import assert from "node:assert/strict";
import {
  calculateWordCount,
  calculateEstimatedSeconds,
  splitTextIntoSections,
  useScriptEditorStore,
} from "./script-editor-store.ts";

test("calculateWordCount correctly counts words", () => {
  assert.equal(calculateWordCount("Hello world"), 2);
  assert.equal(calculateWordCount("   Multiple   spaces   between  words  "), 4);
  assert.equal(calculateWordCount(""), 0);
  assert.equal(calculateWordCount("\n\n\t  "), 0);
});

test("calculateEstimatedSeconds calculates duration at 150 WPM", () => {
  // 150 words = 60s
  assert.equal(calculateEstimatedSeconds(150), 60);
  // 75 words = 30s
  assert.equal(calculateEstimatedSeconds(75), 30);
  // 300 words = 120s
  assert.equal(calculateEstimatedSeconds(300), 120);
});

test("splitTextIntoSections splits paragraphs on blank lines and preserves text", () => {
  const script = `First scene describing the opening landscape of Venice.

Second scene with narration about the canals and ancient history.


Third scene concluding the introduction.`;

  const sections = splitTextIntoSections(script);
  assert.equal(sections.length, 3);
  assert.equal(
    sections[0].text,
    "First scene describing the opening landscape of Venice."
  );
  assert.equal(
    sections[1].text,
    "Second scene with narration about the canals and ancient history."
  );
  assert.equal(sections[2].text, "Third scene concluding the introduction.");

  // Text integrity check (PRD SCR-02 / TDD M3)
  const recombined = sections.map((s) => s.text).join("\n\n");
  const expected = script.replace(/\n\s*\n+/g, "\n\n");
  assert.equal(recombined, expected);
});

test("splitTextIntoSections handles CRLF line endings", () => {
  const crlfScript = "First paragraph.\r\n\r\nSecond paragraph.";
  const sections = splitTextIntoSections(crlfScript);
  assert.equal(sections.length, 2);
  assert.equal(sections[0].text, "First paragraph.");
  assert.equal(sections[1].text, "Second paragraph.");
});

test("Zustand + Zundo store preserves 50+ undo/redo operations and text integrity", () => {
  const store = useScriptEditorStore;
  const temporal = useScriptEditorStore.temporal;

  // Initialize with 2 scenes
  const initial = [
    { id: "s1", text: "Scene one text", wordCount: 3, estimatedDurationSecs: 1 },
    { id: "s2", text: "Scene two text", wordCount: 3, estimatedDurationSecs: 1 },
  ];
  store.getState().setSections(initial);
  temporal.getState().clear();

  // Perform 50 sequential text updates
  for (let i = 1; i <= 50; i++) {
    store.getState().updateSectionText("s1", `Scene one edit #${i}`);
  }

  assert.equal(store.getState().sections[0].text, "Scene one edit #50");

  // Undo 25 steps
  for (let i = 0; i < 25; i++) {
    temporal.getState().undo();
  }
  assert.equal(store.getState().sections[0].text, "Scene one edit #25");

  // Redo 10 steps
  for (let i = 0; i < 10; i++) {
    temporal.getState().redo();
  }
  assert.equal(store.getState().sections[0].text, "Scene one edit #35");
});

test("splitSectionByParagraphs splits multi-paragraph text in a single card into separate scene cards", () => {
  const store = useScriptEditorStore;
  const initial = [
    {
      id: "card-1",
      text: "Paragraph 1 text.\n\nParagraph 2 text.\n\nParagraph 3 text.",
      wordCount: 9,
      estimatedDurationSecs: 4,
    },
  ];
  store.getState().setSections(initial);

  store.getState().splitSectionByParagraphs("card-1");

  const sections = store.getState().sections;
  assert.equal(sections.length, 3);
  assert.equal(sections[0].text, "Paragraph 1 text.");
  assert.equal(sections[1].text, "Paragraph 2 text.");
  assert.equal(sections[2].text, "Paragraph 3 text.");
});


