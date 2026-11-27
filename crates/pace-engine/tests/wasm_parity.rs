//! Native parity generator & verifier for 100 sample frames (`M7-2`).
//!
//! Ensures native Rust (`pace-engine`) and WebAssembly (`pace-engine-wasm`)
//! evaluate the exact same `RenderSpec` SHA-256 hash and per-frame float arrays.

use pace_engine::fit::{FitMode, SizeF};
use pace_engine::motion::{Easing, FocalPoint, Intensity, MotionPreset, SceneVisualEffect};
use pace_engine::spec::{
    compile_spec, encode_frame_plan_flat, frame_plan, CompileSpecInput, SceneEffectSettings,
    SceneSpecInput,
};
use pace_engine::timing::{Boundary, TransitionKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct ParityFrame {
    frame_index: u32,
    flat: Vec<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ParityFixture {
    spec_hash: String,
    total_frames: u32,
    input: CompileSpecInput,
    frames: Vec<ParityFrame>,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("parity_100_frames.json")
}

fn build_parity_input() -> CompileSpecInput {
    CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(1920.0, 1080.0),
        narration_ms: 10_000,
        effect_seed: 2026,
        include_tilt_in_auto: false,
        boundaries: vec![
            Boundary::new(0),
            Boundary::with_transition(3_200, TransitionKind::Crossfade, 800),
            Boundary::with_transition(6_800, TransitionKind::FadeBlack, 600),
            Boundary::new(10_000),
        ],
        stickers: Vec::new(),
        scenes: vec![
            SceneSpecInput {
                scene_index: 0,
                section_id: "parity-sec-0".to_string(),
                media_id: Some("parity-med-0".to_string()),
                image_size: SizeF::new(3840.0, 2160.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint { x: 0.3, y: 0.45 },
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::ZoomInPanRight),
                    intensity: Intensity::Strong,
                    easing: Easing::EaseInOut,
                    manual_shake: Some(true),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: true,
            },
            SceneSpecInput {
                scene_index: 1,
                section_id: "parity-sec-1".to_string(),
                media_id: Some("parity-med-1".to_string()),
                image_size: SizeF::new(1200.0, 1800.0),
                fit_mode: FitMode::FitBlur,
                focal: FocalPoint { x: 0.5, y: 0.25 },
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::ZoomOut),
                    intensity: Intensity::Normal,
                    easing: Easing::Steady,
                    manual_shake: Some(false),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: true,
            },
            SceneSpecInput {
                scene_index: 2,
                section_id: "parity-sec-2".to_string(),
                media_id: Some("parity-med-2".to_string()),
                image_size: SizeF::new(2400.0, 1600.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint { x: 0.72, y: 0.55 },
                effect: SceneEffectSettings {
                    manual_preset: None,
                    intensity: Intensity::Subtle,
                    easing: Easing::EaseInOut,
                    manual_shake: Some(false),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: false,
            },
        ],
    }
}

#[test]
fn native_matches_100_frame_parity_fixture() {
    let input = build_parity_input();
    let spec = compile_spec(&input).expect("compile parity spec");
    assert_eq!(spec.total_frames, 300);

    let frames: Vec<ParityFrame> = (0..100)
        .map(|i| {
            let frame_index = i * 3;
            let plan = frame_plan(&spec, frame_index);
            ParityFrame {
                frame_index,
                flat: encode_frame_plan_flat(&plan),
            }
        })
        .collect();

    let path = fixture_path();
    if std::env::var("UPDATE_PARITY_FIXTURE").as_deref() == Ok("1") || !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture dir");
        }
        let fixture = ParityFixture {
            spec_hash: spec.spec_hash.clone(),
            total_frames: spec.total_frames,
            input,
            frames,
        };
        let json = serde_json::to_string_pretty(&fixture).expect("serialize fixture");
        std::fs::write(&path, json).expect("write fixture");
        return;
    }

    let raw = std::fs::read_to_string(&path).expect("read parity fixture");
    let expected: ParityFixture = serde_json::from_str(&raw).expect("parse parity fixture");
    assert_eq!(spec.spec_hash, expected.spec_hash);
    assert_eq!(spec.total_frames, expected.total_frames);
    assert_eq!(frames.len(), 100);
    assert_eq!(expected.frames.len(), 100);

    for (actual, exp) in frames.iter().zip(&expected.frames) {
        assert_eq!(actual.frame_index, exp.frame_index);
        assert_eq!(actual.flat.len(), exp.flat.len());
        for (idx, (a, e)) in actual.flat.iter().zip(&exp.flat).enumerate() {
            assert_eq!(
                a.to_bits(),
                e.to_bits(),
                "frame {} element {idx} mismatch: actual={a} expected={e}",
                actual.frame_index
            );
        }
    }
}
