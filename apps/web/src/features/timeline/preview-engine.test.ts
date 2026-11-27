import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { initSync, WasmRenderEngine } from "@pace/engine-wasm";
import {
  MOTION_PRESETS,
  buildLocalCompileSpecInput,
  decodeFramePlanFlat,
  decodeStickerPlanFlat,
  formatPresetLabel,
  getPrefetchSceneIndices,
  normalizeBoundaryTransition,
  timeMsToFrameIndex,
} from "./preview-engine.ts";

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(currentDir, "../../../../..");
const wasmPath = path.join(
  repoRoot,
  "packages/engine-wasm/pace_engine_wasm_bg.wasm"
);

let wasmReady = false;
function ensureWasm() {
  if (!wasmReady) {
    const bytes = fs.readFileSync(wasmPath);
    initSync({ module: bytes });
    wasmReady = true;
  }
}

test("timeMsToFrameIndex maps timestamps to clamped 30 fps frame indices", () => {
  assert.equal(timeMsToFrameIndex(0, 30, 300), 0);
  assert.equal(timeMsToFrameIndex(1000, 30, 300), 30);
  assert.equal(timeMsToFrameIndex(5000, 30, 300), 150);
  assert.equal(timeMsToFrameIndex(12000, 30, 300), 299);
  assert.equal(timeMsToFrameIndex(-50, 30, 300), 0);
  assert.equal(timeMsToFrameIndex(500, 30, 0), 0);
});

test("decodeFramePlanFlat decodes header and multi-layer flat WASM buffer", () => {
  const flat = new Float32Array([
    0.45, // fadeBlackAlpha
    1, // primarySceneIndex
    2, // layerCount
    0.35, // flashWhiteAlpha
    // Layer 0: blurred background
    1, 1, 10, 20, 1000, 562.5, 0, 0, 1920, 1080, 1.0, 0,
    // Layer 1: foreground image with rotation
    1, 0, 0, 0, 1080, 1920, 656.25, 0, 607.5, 1080, 0.85, -0.05,
  ]);

  const decoded = decodeFramePlanFlat(flat);
  assert.ok(Math.abs(decoded.fadeBlackAlpha - 0.45) < 1e-5);
  assert.ok(Math.abs(decoded.flashWhiteAlpha - 0.35) < 1e-5);
  assert.equal(decoded.primarySceneIndex, 1);
  assert.equal(decoded.layers.length, 2);

  assert.equal(decoded.layers[0].sceneIndex, 1);
  assert.equal(decoded.layers[0].kind, "blurred_background");
  assert.deepEqual(decoded.layers[0].src, { x: 10, y: 20, w: 1000, h: 562.5 });
  assert.deepEqual(decoded.layers[0].dst, { x: 0, y: 0, w: 1920, h: 1080 });
  assert.equal(decoded.layers[0].opacity, 1.0);
  assert.equal(decoded.layers[0].rotationRad, 0);

  assert.equal(decoded.layers[1].kind, "image");
  assert.ok(Math.abs(decoded.layers[1].opacity - 0.85) < 1e-5);
  assert.ok(Math.abs(decoded.layers[1].rotationRad - -0.05) < 1e-5);
});

test("decodeFramePlanFlat handles empty or truncated buffers safely", () => {
  const empty = decodeFramePlanFlat(new Float32Array([]));
  assert.equal(empty.layers.length, 0);
  assert.equal(empty.fadeBlackAlpha, 0);
});

test("getPrefetchSceneIndices prioritizes current, upcoming, and previous scenes", () => {
  assert.deepEqual(getPrefetchSceneIndices(0, 5, 2), [0, 1, 2]);
  assert.deepEqual(getPrefetchSceneIndices(2, 5, 2), [2, 3, 4, 1]);
  assert.deepEqual(getPrefetchSceneIndices(4, 5, 2), [4, 3]);
  assert.deepEqual(getPrefetchSceneIndices(0, 0, 2), []);
});

test("normalizeBoundaryTransition enforces cut=0ms and clamps active transitions to 100..3000ms", () => {
  assert.deepEqual(normalizeBoundaryTransition("cut", 800), {
    kind: "cut",
    duration_ms: 0,
  });
  assert.deepEqual(normalizeBoundaryTransition("crossfade", 50), {
    kind: "crossfade",
    duration_ms: 100,
  });
  assert.deepEqual(normalizeBoundaryTransition("fade_black", 4500), {
    kind: "fade_black",
    duration_ms: 3000,
  });
  assert.deepEqual(normalizeBoundaryTransition("whip_pan_left", 0), {
    kind: "whip_pan_left",
    duration_ms: 450,
  });
  assert.deepEqual(normalizeBoundaryTransition("zoom_through", 600), {
    kind: "zoom_through",
    duration_ms: 600,
  });
  assert.deepEqual(normalizeBoundaryTransition("shutter_flash", 0), {
    kind: "shutter_flash",
    duration_ms: 350,
  });
  assert.deepEqual(normalizeBoundaryTransition("slide_left", 0), {
    kind: "slide_left",
    duration_ms: 450,
  });
  assert.deepEqual(normalizeBoundaryTransition("slide_right", 0), {
    kind: "slide_right",
    duration_ms: 450,
  });
  assert.deepEqual(normalizeBoundaryTransition("slide_down", 0), {
    kind: "slide_down",
    duration_ms: 450,
  });
});

test("formatPresetLabel resolves all 25 motion presets to readable labels", () => {
  assert.equal(MOTION_PRESETS.length, 25);
  for (const preset of MOTION_PRESETS) {
    assert.ok(formatPresetLabel(preset.id).length > 0);
  }
  assert.equal(formatPresetLabel("zoom_in_pan_right"), "Zoom In + Pan Right");
  assert.equal(formatPresetLabel("punch_zoom_in"), "Zoom 1 (Punch In)");
  assert.equal(formatPresetLabel("zoom_snap"), "Zoom 2 (Snap)");
  assert.equal(formatPresetLabel("pendulum"), "Pendulum");
  assert.equal(formatPresetLabel("handheld_drift"), "Handheld Float");
});

test("decodeStickerPlanFlat decodes active sticker buffers", () => {
  const stickers = decodeStickerPlanFlat(
    new Float32Array([1, 3, 0.5, 0.82, 1.15, 0.9, 0.02])
  );
  assert.equal(stickers.length, 1);
  assert.equal(stickers[0].kind, "bell");
  assert.ok(Math.abs(stickers[0].scale - 1.15) < 1e-5);
  assert.ok(Math.abs(stickers[0].rotationRad - 0.02) < 1e-5);
});

test("buildLocalCompileSpecInput compiles cleanly in WasmRenderEngine with transitions and overrides", () => {
  ensureWasm();

  const baseParams = {
    fps: 30,
    aspectRatio: "16:9" as const,
    durationMs: 10000,
    effectSeed: 42,
    sections: [
      { id: "sec-0", position: 0, text: "Scene 1" },
      { id: "sec-1", position: 1, text: "Scene 2" },
    ],
    boundaries: [
      { index: 0, timeMs: 0 },
      {
        index: 1,
        timeMs: 5000,
        transition: { kind: "crossfade", duration_ms: 800 },
      },
      { index: 2, timeMs: 10000 },
    ],
    assignments: [
      {
        sectionId: "sec-0",
        mediaId: "med-0",
        fitMode: "fit_blur" as const,
        focalX: 0.3,
        focalY: 0.4,
        effect: {
          manual_preset: "zoom_in_pan_right",
          intensity: "strong",
          easing: "steady",
          manual_shake: true,
        },
        effectOverridden: true,
      },
      {
        sectionId: "sec-1",
        mediaId: "med-1",
        fitMode: "fill" as const,
        focalX: 0.5,
        focalY: 0.5,
      },
    ],
    mediaDimensions: {
      "med-0": { w: 1080, h: 1920 },
      "med-1": { w: 2400, h: 1600 },
    },
  };

  const input1 = buildLocalCompileSpecInput(baseParams);
  const engine1 = new WasmRenderEngine(JSON.stringify(input1));
  try {
    assert.equal(engine1.total_frames, 300);
    assert.equal(engine1.fps, 30);
    assert.equal(engine1.frame_width, 1920);
    assert.equal(engine1.frame_height, 1080);

    // Frame 30 is strictly in Scene 0 (fit_blur -> blurred_background + image)
    const scene0Plan = decodeFramePlanFlat(engine1.frame_plan_flat(30));
    assert.equal(scene0Plan.layers.length, 2);
    assert.equal(scene0Plan.layers[0].kind, "blurred_background");
    assert.equal(scene0Plan.layers[1].kind, "image");

    // Frame 150 is center of 800ms crossfade between fit_blur Scene 0 and fill Scene 1
    const transitionPlan = decodeFramePlanFlat(engine1.frame_plan_flat(150));
    const imageLayers = transitionPlan.layers.filter((l) => l.kind === "image");
    assert.equal(imageLayers.length, 2);
    const opacitySum = imageLayers.reduce((acc, l) => acc + l.opacity, 0);
    assert.ok(Math.abs(opacitySum - 1.0) < 1e-4);

    // Re-rolling seed changes spec hash while keeping Scene 0 override intact
    const input2 = buildLocalCompileSpecInput({ ...baseParams, effectSeed: 43 });
    const engine2 = new WasmRenderEngine(JSON.stringify(input2));
    try {
      assert.notEqual(engine1.spec_hash, engine2.spec_hash);
      const spec2 = JSON.parse(engine2.compiled_spec_json());
      assert.equal(spec2.scenes[0].preset, "zoom_in_pan_right");
      assert.equal(spec2.scenes[0].has_shake, true);
      assert.equal(spec2.scenes[0].effect_overridden, true);
    } finally {
      engine2.free();
    }
  } finally {
    engine1.free();
  }
});
