import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { initSync, WasmRenderEngine } from "@pace/engine-wasm";

interface ParityFixture {
  spec_hash: string;
  total_frames: number;
  input: unknown;
  frames: Array<{
    frame_index: number;
    flat: number[];
  }>;
}

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(currentDir, "../../../../..");
const fixturePath = path.join(
  repoRoot,
  "crates/pace-engine/tests/fixtures/parity_100_frames.json"
);
const wasmPath = path.join(
  repoRoot,
  "packages/engine-wasm/pace_engine_wasm_bg.wasm"
);

test("M7-2: WASM and native pace-engine produce matching FramePlans across 100 frames", () => {
  const fixture = JSON.parse(
    fs.readFileSync(fixturePath, "utf8")
  ) as ParityFixture;
  const wasmBytes = fs.readFileSync(wasmPath);

  initSync({ module: wasmBytes });

  const engine = new WasmRenderEngine(JSON.stringify(fixture.input));
  try {
    assert.equal(engine.spec_hash, fixture.spec_hash);
    assert.equal(engine.total_frames, fixture.total_frames);
    assert.equal(fixture.frames.length, 100);

    for (const sample of fixture.frames) {
      const actual = engine.frame_plan_flat(sample.frame_index);
      assert.equal(
        actual.length,
        sample.flat.length,
        `layer buffer length mismatch at frame ${sample.frame_index}`
      );

      for (let i = 0; i < actual.length; i++) {
        const expectedF32 = Math.fround(sample.flat[i]);
        const diff = Math.abs(actual[i] - expectedF32);
        assert.ok(
          diff <= 1e-5,
          `frame ${sample.frame_index} float[${i}] mismatch: wasm=${actual[i]} native=${expectedF32} diff=${diff}`
        );
      }
    }
  } finally {
    engine.free();
  }
});
