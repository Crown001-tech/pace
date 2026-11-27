//! Frozen render specification, deterministic hashing, and per-frame plan evaluator
//! (`EFX-01..08`, `PRV-01..05`, `EXP-01..06`, TDD Sections 12 & 14).
//!
//! Zero file, network, clock, or database I/O: compiles timeline boundaries,
//! scene media geometry, fit mode, focal point, and effect overrides into an
//! immutable `RenderSpec` whose `frame_plan(spec, frame_index)` output is
//! guaranteed identical across native Rust (`pace-render`) and WASM (`pace-engine-wasm`).

use crate::effects::{
    assign_automatic_effects_with_options, default_easing_for_preset, SceneEffectInput,
};
use crate::fit::{calculate_fit_layers, FitMode, LayerKind, RectF, SizeF};
use crate::motion::{
    build_motion_with_effect, cam_state_at, Easing, FocalPoint, Intensity, Motion, MotionPreset,
    SceneVisualEffect,
};
use crate::stickers::{evaluate_stickers_at, FrameSticker, StickerPlacementSpec};
use crate::timing::{
    compute_timeline_frames, Boundary, TimingError, TransitionKind, TransitionWindow,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write as _};

/// Errors produced when compiling a `RenderSpec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    Timing(TimingError),
    SceneCountMismatch { expected: usize, actual: usize },
    InvalidFps { fps: u32 },
    InvalidFrameSize { w: u32, h: u32 },
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecError::Timing(err) => write!(f, "timeline timing error: {err:?}"),
            SpecError::SceneCountMismatch { expected, actual } => {
                write!(
                    f,
                    "scene count mismatch: boundaries define {expected} scenes, got {actual}"
                )
            }
            SpecError::InvalidFps { fps } => write!(f, "fps must be positive, got {fps}"),
            SpecError::InvalidFrameSize { w, h } => {
                write!(f, "frame dimensions must be positive, got {w}x{h}")
            }
        }
    }
}

impl std::error::Error for SpecError {}

impl From<TimingError> for SpecError {
    fn from(value: TimingError) -> Self {
        SpecError::Timing(value)
    }
}

/// Per-scene effect customization persisted in `scene_media.effect` (`JSONB`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SceneEffectSettings {
    #[serde(default)]
    pub manual_preset: Option<MotionPreset>,
    #[serde(default)]
    pub intensity: Intensity,
    #[serde(default)]
    pub easing: Easing,
    #[serde(default)]
    pub manual_shake: Option<bool>,
    #[serde(default)]
    pub visual_effect: SceneVisualEffect,
}

/// Input parameters for a single scene entering `compile_spec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSpecInput {
    pub scene_index: usize,
    #[serde(default)]
    pub section_id: String,
    #[serde(default)]
    pub media_id: Option<String>,
    pub image_size: SizeF,
    #[serde(default)]
    pub fit_mode: FitMode,
    #[serde(default)]
    pub focal: FocalPoint,
    #[serde(default)]
    pub effect: SceneEffectSettings,
    #[serde(default)]
    pub effect_overridden: bool,
}

/// Complete project inputs needed to deterministically compile a `RenderSpec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompileSpecInput {
    pub fps: u32,
    pub frame_size: SizeF,
    pub narration_ms: u64,
    pub effect_seed: u64,
    #[serde(default)]
    pub include_tilt_in_auto: bool,
    pub boundaries: Vec<Boundary>,
    pub scenes: Vec<SceneSpecInput>,
    #[serde(default)]
    pub stickers: Vec<StickerPlacementSpec>,
}

/// Resolved, immutable scene plan within a compiled `RenderSpec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledScene {
    pub scene_index: usize,
    pub section_id: String,
    pub media_id: Option<String>,
    pub image_size: SizeF,
    pub start_frame: u32,
    pub end_frame: u32,
    pub frame_count: u32,
    pub transition_shortened: bool,
    pub fit_mode: FitMode,
    pub focal: FocalPoint,
    pub preset: MotionPreset,
    pub has_shake: bool,
    #[serde(default)]
    pub visual_effect: SceneVisualEffect,
    pub motion: Motion,
    pub effect_overridden: bool,
}

/// Frozen, deterministic render blueprint shared across native export and browser preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSpec {
    pub spec_hash: String,
    pub fps: u32,
    pub frame_size: SizeF,
    pub narration_ms: u64,
    pub effect_seed: u64,
    pub total_frames: u32,
    pub scenes: Vec<CompiledScene>,
    pub transitions: Vec<TransitionWindow>,
    #[serde(default)]
    pub stickers: Vec<StickerPlacementSpec>,
}

/// Metadata describing an active transition overlapping a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveTransition {
    pub boundary_index: usize,
    pub kind: TransitionKind,
    pub progress: f32,
}

/// A single composited draw instruction emitted for a frame (`TDD 14.3`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameLayer {
    pub scene_index: usize,
    pub kind: LayerKind,
    pub src: RectF,
    pub dst: RectF,
    pub opacity: f32,
    #[serde(default)]
    pub rotation_rad: f32,
}

/// Exact instructions required to draw frame `frame_index`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FramePlan {
    pub frame_index: u32,
    pub primary_scene_index: usize,
    pub fade_black_alpha: f32,
    #[serde(default)]
    pub flash_white_alpha: f32,
    pub active_transition: Option<ActiveTransition>,
    pub layers: Vec<FrameLayer>,
    #[serde(default)]
    pub stickers: Vec<FrameSticker>,
}

/// Number of header floats before layer records in flat WASM encoding.
pub const FLAT_HEADER_LEN: usize = 4;
/// Number of `f32` values per layer in flat WASM encoding.
pub const FLAT_LAYER_STRIDE: usize = 12;

/// Encodes a `FramePlan` into a compact flat `Vec<f32>` for zero-JSON WASM/JS transfer.
pub fn encode_frame_plan_flat(plan: &FramePlan) -> Vec<f32> {
    let mut out = Vec::with_capacity(FLAT_HEADER_LEN + plan.layers.len() * FLAT_LAYER_STRIDE);
    encode_frame_plan_into(plan, &mut out);
    out
}

/// Encodes a `FramePlan` into a caller-supplied buffer (`clear()`ed first).
pub fn encode_frame_plan_into(plan: &FramePlan, out: &mut Vec<f32>) {
    out.clear();
    out.push(plan.fade_black_alpha);
    out.push(plan.primary_scene_index as f32);
    out.push(plan.layers.len() as f32);
    out.push(plan.flash_white_alpha);

    for layer in &plan.layers {
        let kind_code = match layer.kind {
            LayerKind::Image => 0.0,
            LayerKind::BlurredBackground => 1.0,
        };
        out.extend_from_slice(&[
            layer.scene_index as f32,
            kind_code,
            layer.src.x,
            layer.src.y,
            layer.src.w,
            layer.src.h,
            layer.dst.x,
            layer.dst.y,
            layer.dst.w,
            layer.dst.h,
            layer.opacity,
            layer.rotation_rad,
        ]);
    }
}

#[derive(Serialize)]
struct CanonicalSpecPayload<'a> {
    fps: u32,
    frame_size: SizeF,
    narration_ms: u64,
    effect_seed: u64,
    total_frames: u32,
    scenes: &'a [CompiledScene],
    transitions: &'a [TransitionWindow],
    #[serde(skip_serializing_if = "<[StickerPlacementSpec]>::is_empty")]
    stickers: &'a [StickerPlacementSpec],
}

/// Computes canonical lowercase SHA-256 hex digest of a `RenderSpec` (excluding `spec_hash`).
pub fn compute_spec_hash(spec: &RenderSpec) -> String {
    let payload = CanonicalSpecPayload {
        fps: spec.fps,
        frame_size: spec.frame_size,
        narration_ms: spec.narration_ms,
        effect_seed: spec.effect_seed,
        total_frames: spec.total_frames,
        scenes: &spec.scenes,
        transitions: &spec.transitions,
        stickers: &spec.stickers,
    };

    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Compiles timeline boundaries, image geometry, and effect overrides into a frozen `RenderSpec`.
pub fn compile_spec(input: &CompileSpecInput) -> Result<RenderSpec, SpecError> {
    if input.fps == 0 {
        return Err(SpecError::InvalidFps { fps: input.fps });
    }
    if !input.frame_size.w.is_finite()
        || !input.frame_size.h.is_finite()
        || input.frame_size.w <= 0.0
        || input.frame_size.h <= 0.0
    {
        return Err(SpecError::InvalidFrameSize {
            w: input.frame_size.w.max(0.0) as u32,
            h: input.frame_size.h.max(0.0) as u32,
        });
    }

    let frame_size = SizeF::new(input.frame_size.w, input.frame_size.h);
    let (scenes_frames, transitions) =
        compute_timeline_frames(&input.boundaries, input.narration_ms, input.fps)?;

    if input.scenes.len() != scenes_frames.len() {
        return Err(SpecError::SceneCountMismatch {
            expected: scenes_frames.len(),
            actual: input.scenes.len(),
        });
    }

    let effect_inputs: Vec<SceneEffectInput> = input
        .scenes
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let duration_ms = input.boundaries[idx + 1].time_ms - input.boundaries[idx].time_ms;
            let focal = FocalPoint {
                x: s.focal.x.clamp(0.0, 1.0),
                y: s.focal.y.clamp(0.0, 1.0),
            };
            SceneEffectInput {
                scene_index: idx,
                duration_ms,
                focal,
                manual_preset: s.effect.manual_preset,
                manual_shake: s.effect.manual_shake,
            }
        })
        .collect();

    let assigned = assign_automatic_effects_with_options(
        &effect_inputs,
        input.effect_seed,
        input.include_tilt_in_auto,
    );

    let scenes: Vec<CompiledScene> = input
        .scenes
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let sf = &scenes_frames[idx];
            let eff = &assigned[idx];
            let duration_ms = input.boundaries[idx + 1].time_ms - input.boundaries[idx].time_ms;
            let focal = FocalPoint {
                x: s.focal.x.clamp(0.0, 1.0),
                y: s.focal.y.clamp(0.0, 1.0),
            };
            let effective_easing =
                if !s.effect_overridden && s.effect.easing == Easing::EaseInOut {
                    default_easing_for_preset(eff.preset)
                } else {
                    s.effect.easing
                };
            let motion = build_motion_with_effect(
                eff.preset,
                focal,
                duration_ms,
                s.effect.intensity,
                effective_easing,
                eff.motion.shake,
                s.effect.visual_effect,
            );
            let image_size = SizeF::new(s.image_size.w, s.image_size.h);

            CompiledScene {
                scene_index: idx,
                section_id: s.section_id.clone(),
                media_id: s.media_id.clone(),
                image_size,
                start_frame: sf.start_frame,
                end_frame: sf.end_frame,
                frame_count: sf.frame_count,
                transition_shortened: sf.transition_shortened,
                fit_mode: s.fit_mode,
                focal,
                preset: eff.preset,
                has_shake: eff.has_shake,
                visual_effect: s.effect.visual_effect,
                motion,
                effect_overridden: s.effect_overridden,
            }
        })
        .collect();

    let total_frames = scenes.last().map(|s| s.end_frame).unwrap_or(0);
    let mut spec = RenderSpec {
        spec_hash: String::new(),
        fps: input.fps,
        frame_size,
        narration_ms: input.narration_ms,
        effect_seed: input.effect_seed,
        total_frames,
        scenes,
        transitions,
        stickers: input.stickers.clone(),
    };
    spec.spec_hash = compute_spec_hash(&spec);
    Ok(spec)
}

fn scene_progress(scene: &CompiledScene, frame: u32) -> f32 {
    if scene.frame_count <= 1 || frame <= scene.start_frame {
        return 0.0;
    }
    let last_frame = scene.end_frame.saturating_sub(1);
    if frame >= last_frame {
        return 1.0;
    }
    ((frame - scene.start_frame) as f32 / (scene.frame_count - 1) as f32).clamp(0.0, 1.0)
}

fn push_scene_layers_transformed(
    scene: &CompiledScene,
    frame_size: SizeF,
    frame: u32,
    opacity: f32,
    zoom_mult: f32,
    offset_x: f32,
    offset_y: f32,
    out: &mut Vec<FrameLayer>,
) {
    let progress = scene_progress(scene, frame);
    let mut cam = cam_state_at(&scene.motion, progress);
    cam.zoom = (cam.zoom * zoom_mult.max(1.0)).max(1.0);
    for layer in calculate_fit_layers(scene.fit_mode, scene.image_size, frame_size, scene.focal, cam)
    {
        out.push(FrameLayer {
            scene_index: scene.scene_index,
            kind: layer.kind,
            src: layer.src,
            dst: RectF::new(
                layer.dst.x + offset_x,
                layer.dst.y + offset_y,
                layer.dst.w,
                layer.dst.h,
            ),
            opacity: layer.opacity * opacity,
            rotation_rad: layer.rotation_rad,
        });
    }
}

fn push_scene_layers(
    scene: &CompiledScene,
    frame_size: SizeF,
    frame: u32,
    opacity: f32,
    out: &mut Vec<FrameLayer>,
) {
    push_scene_layers_transformed(scene, frame_size, frame, opacity, 1.0, 0.0, 0.0, out);
}

fn find_scene_index(scenes: &[CompiledScene], frame: u32) -> usize {
    let candidate = scenes.partition_point(|s| s.end_frame <= frame);
    if candidate < scenes.len() {
        candidate
    } else {
        scenes.len().saturating_sub(1)
    }
}

/// Evaluates the exact draw plan for frame `frame_index` (`TDD 12.1..12.6`).
pub fn frame_plan(spec: &RenderSpec, frame_index: u32) -> FramePlan {
    if spec.total_frames == 0 || spec.scenes.is_empty() {
        return FramePlan {
            frame_index,
            primary_scene_index: 0,
            fade_black_alpha: 0.0,
            flash_white_alpha: 0.0,
            active_transition: None,
            layers: Vec::new(),
            stickers: Vec::new(),
        };
    }

    let f = frame_index.min(spec.total_frames - 1);
    let mut layers = Vec::with_capacity(4);
    let stickers = evaluate_stickers_at(&spec.stickers, f, spec.fps);

    if let Some(tw) = spec
        .transitions
        .iter()
        .find(|tw| tw.duration_frames > 0 && f >= tw.start_frame && f < tw.end_frame)
    {
        let out_idx = tw.boundary_index.saturating_sub(1);
        let in_idx = tw.boundary_index.min(spec.scenes.len().saturating_sub(1));
        let u = (((f - tw.start_frame) as f32 + 0.5) / tw.duration_frames as f32).clamp(0.0, 1.0);
        let active = ActiveTransition {
            boundary_index: tw.boundary_index,
            kind: tw.kind,
            progress: u,
        };

        match tw.kind {
            TransitionKind::Crossfade => {
                let out_opacity = 1.0 - u;
                let in_opacity = u;
                push_scene_layers(
                    &spec.scenes[out_idx],
                    spec.frame_size,
                    f,
                    out_opacity,
                    &mut layers,
                );
                if in_idx != out_idx {
                    push_scene_layers(
                        &spec.scenes[in_idx],
                        spec.frame_size,
                        f,
                        in_opacity,
                        &mut layers,
                    );
                }
                let primary_scene_index = if f < tw.center_frame { out_idx } else { in_idx };
                return FramePlan {
                    frame_index: f,
                    primary_scene_index,
                    fade_black_alpha: 0.0,
                    flash_white_alpha: 0.0,
                    active_transition: Some(active),
                    layers,
                    stickers,
                };
            }
            TransitionKind::FadeBlack => {
                let fade_black_alpha = (1.0 - (2.0 * u - 1.0).abs()).clamp(0.0, 1.0);
                let scene_idx = if f < tw.center_frame { out_idx } else { in_idx };
                push_scene_layers(&spec.scenes[scene_idx], spec.frame_size, f, 1.0, &mut layers);
                return FramePlan {
                    frame_index: f,
                    primary_scene_index: scene_idx,
                    fade_black_alpha,
                    flash_white_alpha: 0.0,
                    active_transition: Some(active),
                    layers,
                    stickers,
                };
            }
            TransitionKind::ShutterFlash => {
                let flash_white_alpha =
                    (1.0 - (2.0 * u - 1.0).abs()).powf(1.5).clamp(0.0, 1.0);
                let scene_idx = if f < tw.center_frame { out_idx } else { in_idx };
                push_scene_layers(&spec.scenes[scene_idx], spec.frame_size, f, 1.0, &mut layers);
                return FramePlan {
                    frame_index: f,
                    primary_scene_index: scene_idx,
                    fade_black_alpha: 0.0,
                    flash_white_alpha,
                    active_transition: Some(active),
                    layers,
                    stickers,
                };
            }
            TransitionKind::ZoomThrough => {
                let out_zoom_mult = 1.0 + 0.55 * (u * u);
                let in_zoom_mult = 1.0 + 0.32 * ((1.0 - u) * (1.0 - u));
                let in_opacity = Easing::EaseInOut.apply(u);

                push_scene_layers_transformed(
                    &spec.scenes[out_idx],
                    spec.frame_size,
                    f,
                    1.0,
                    out_zoom_mult,
                    0.0,
                    0.0,
                    &mut layers,
                );
                if in_idx != out_idx {
                    push_scene_layers_transformed(
                        &spec.scenes[in_idx],
                        spec.frame_size,
                        f,
                        in_opacity,
                        in_zoom_mult,
                        0.0,
                        0.0,
                        &mut layers,
                    );
                }
                let primary_scene_index = if f < tw.center_frame { out_idx } else { in_idx };
                return FramePlan {
                    frame_index: f,
                    primary_scene_index,
                    fade_black_alpha: 0.0,
                    flash_white_alpha: 0.0,
                    active_transition: Some(active),
                    layers,
                    stickers,
                };
            }
            TransitionKind::WhipPanLeft
            | TransitionKind::WhipPanRight
            | TransitionKind::SlideLeft
            | TransitionKind::SlideRight
            | TransitionKind::SlideUp
            | TransitionKind::SlideDown => {
                let w = Easing::DynamicCurve.apply(u);
                let fw = spec.frame_size.w;
                let fh = spec.frame_size.h;
                let (out_dx, out_dy, in_dx, in_dy) = match tw.kind {
                    TransitionKind::WhipPanLeft | TransitionKind::SlideLeft => {
                        (-w * fw, 0.0, (1.0 - w) * fw, 0.0)
                    }
                    TransitionKind::WhipPanRight | TransitionKind::SlideRight => {
                        (w * fw, 0.0, -(1.0 - w) * fw, 0.0)
                    }
                    TransitionKind::SlideUp => (0.0, -w * fh, 0.0, (1.0 - w) * fh),
                    TransitionKind::SlideDown => (0.0, w * fh, 0.0, -(1.0 - w) * fh),
                    _ => (0.0, 0.0, 0.0, 0.0),
                };

                push_scene_layers_transformed(
                    &spec.scenes[out_idx],
                    spec.frame_size,
                    f,
                    1.0,
                    1.0,
                    out_dx,
                    out_dy,
                    &mut layers,
                );
                if in_idx != out_idx {
                    push_scene_layers_transformed(
                        &spec.scenes[in_idx],
                        spec.frame_size,
                        f,
                        1.0,
                        1.0,
                        in_dx,
                        in_dy,
                        &mut layers,
                    );
                }
                let primary_scene_index = if f < tw.center_frame { out_idx } else { in_idx };
                return FramePlan {
                    frame_index: f,
                    primary_scene_index,
                    fade_black_alpha: 0.0,
                    flash_white_alpha: 0.0,
                    active_transition: Some(active),
                    layers,
                    stickers,
                };
            }
            TransitionKind::Cut => {}
        }
    }

    let scene_idx = find_scene_index(&spec.scenes, f);
    push_scene_layers(&spec.scenes[scene_idx], spec.frame_size, f, 1.0, &mut layers);

    FramePlan {
        frame_index: f,
        primary_scene_index: scene_idx,
        fade_black_alpha: 0.0,
        flash_white_alpha: 0.0,
        active_transition: None,
        layers,
        stickers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input(seed: u64, boundaries: Vec<Boundary>, fit0: FitMode, fit1: FitMode) -> CompileSpecInput {
        CompileSpecInput {
            fps: 30,
            frame_size: SizeF::new(1920.0, 1080.0),
            narration_ms: 10_000,
            effect_seed: seed,
            include_tilt_in_auto: false,
            boundaries,
            stickers: Vec::new(),
            scenes: vec![
                SceneSpecInput {
                    scene_index: 0,
                    section_id: "sec-0".to_string(),
                    media_id: Some("med-0".to_string()),
                    image_size: SizeF::new(2400.0, 1600.0),
                    fit_mode: fit0,
                    focal: FocalPoint { x: 0.35, y: 0.5 },
                    effect: SceneEffectSettings::default(),
                    effect_overridden: false,
                },
                SceneSpecInput {
                    scene_index: 1,
                    section_id: "sec-1".to_string(),
                    media_id: Some("med-1".to_string()),
                    image_size: SizeF::new(1600.0, 2400.0),
                    fit_mode: fit1,
                    focal: FocalPoint { x: 0.5, y: 0.25 },
                    effect: SceneEffectSettings::default(),
                    effect_overridden: false,
                },
            ],
        }
    }

    #[test]
    fn spec_hash_is_deterministic_and_sensitive_to_changes() {
        let b1 = vec![
            Boundary::new(0),
            Boundary::new(5_000),
            Boundary::new(10_000),
        ];
        let spec_a = compile_spec(&sample_input(42, b1.clone(), FitMode::Fill, FitMode::Fill))
            .expect("compile spec A");
        let spec_b = compile_spec(&sample_input(42, b1.clone(), FitMode::Fill, FitMode::Fill))
            .expect("compile spec B");

        assert_eq!(spec_a.spec_hash.len(), 64);
        assert_eq!(spec_a.spec_hash, spec_b.spec_hash);
        assert_eq!(spec_a.spec_hash, compute_spec_hash(&spec_a));

        // Changing seed produces a different hash
        let spec_seed =
            compile_spec(&sample_input(99, b1.clone(), FitMode::Fill, FitMode::Fill))
                .expect("compile seed change");
        assert_ne!(spec_a.spec_hash, spec_seed.spec_hash);

        // Changing fit mode produces a different hash
        let spec_fit = compile_spec(&sample_input(42, b1, FitMode::Fill, FitMode::FitBlur))
            .expect("compile fit change");
        assert_ne!(spec_a.spec_hash, spec_fit.spec_hash);
    }

    #[test]
    fn crossfade_transition_opacities_sum_to_one() {
        let boundaries = vec![
            Boundary::new(0),
            Boundary::with_transition(5_000, TransitionKind::Crossfade, 800),
            Boundary::new(10_000),
        ];

        let spec = compile_spec(&sample_input(7, boundaries, FitMode::Fill, FitMode::Fill))
            .expect("compile crossfade");
        assert_eq!(spec.total_frames, 300);
        assert_eq!(spec.transitions.len(), 1);
        let tw = &spec.transitions[0];
        assert_eq!(tw.center_frame, 150);
        assert_eq!(tw.duration_frames, 24);

        // Outside transition: single scene at opacity 1.0
        let outside = frame_plan(&spec, 20);
        assert_eq!(outside.fade_black_alpha, 0.0);
        assert!(outside.active_transition.is_none());
        assert_eq!(outside.layers.len(), 1);
        assert_eq!(outside.layers[0].scene_index, 0);
        assert!((outside.layers[0].opacity - 1.0).abs() < 1e-6);

        // Every frame inside transition blends outgoing + incoming with opacities summing to 1.0
        let mut prev_in_opacity = 0.0_f32;
        for f in tw.start_frame..tw.end_frame {
            let plan = frame_plan(&spec, f);
            assert_eq!(plan.fade_black_alpha, 0.0);
            assert!(plan.active_transition.is_some());
            assert_eq!(plan.layers.len(), 2);
            assert_eq!(plan.layers[0].scene_index, 0);
            assert_eq!(plan.layers[1].scene_index, 1);

            let out_alpha = plan.layers[0].opacity;
            let in_alpha = plan.layers[1].opacity;
            assert!((out_alpha + in_alpha - 1.0).abs() < 1e-5);
            assert!(out_alpha > 0.0 && out_alpha < 1.0);
            assert!(in_alpha > prev_in_opacity);
            prev_in_opacity = in_alpha;
        }
    }

    #[test]
    fn fade_black_switches_at_center_and_computes_symmetric_alpha() {
        let boundaries = vec![
            Boundary::new(0),
            Boundary::with_transition(5_000, TransitionKind::FadeBlack, 800),
            Boundary::new(10_000),
        ];

        let spec = compile_spec(&sample_input(7, boundaries, FitMode::Fill, FitMode::Fill))
            .expect("compile fade_black");
        let tw = &spec.transitions[0];

        // First half renders outgoing scene only, fading out toward black
        let first = frame_plan(&spec, tw.start_frame);
        let before_center = frame_plan(&spec, tw.center_frame - 1);
        assert_eq!(first.layers.len(), 1);
        assert_eq!(first.layers[0].scene_index, 0);
        assert_eq!(before_center.layers.len(), 1);
        assert_eq!(before_center.layers[0].scene_index, 0);
        assert!(first.fade_black_alpha > 0.0);
        assert!(before_center.fade_black_alpha > first.fade_black_alpha);
        assert!(before_center.fade_black_alpha > 0.9);

        // Second half switches to incoming scene only, fading back in from black
        let at_center = frame_plan(&spec, tw.center_frame);
        let last = frame_plan(&spec, tw.end_frame - 1);
        assert_eq!(at_center.layers.len(), 1);
        assert_eq!(at_center.layers[0].scene_index, 1);
        assert_eq!(last.layers.len(), 1);
        assert_eq!(last.layers[0].scene_index, 1);
        assert!((first.fade_black_alpha - last.fade_black_alpha).abs() < 1e-5);
        assert!((before_center.fade_black_alpha - at_center.fade_black_alpha).abs() < 1e-5);
    }

    #[test]
    fn fit_blur_and_manual_effect_settings_are_applied() {
        let boundaries = vec![
            Boundary::new(0),
            Boundary::new(5_000),
            Boundary::new(10_000),
        ];
        let mut input = sample_input(42, boundaries, FitMode::FitBlur, FitMode::Fill);
        input.scenes[0].effect = SceneEffectSettings {
            manual_preset: Some(MotionPreset::ZoomIn),
            intensity: Intensity::Strong,
            easing: Easing::Steady,
            manual_shake: Some(true),
            visual_effect: SceneVisualEffect::None,
        };
        input.scenes[0].effect_overridden = true;

        let spec = compile_spec(&input).expect("compile fit blur");
        assert_eq!(spec.scenes[0].preset, MotionPreset::ZoomIn);
        assert_eq!(spec.scenes[0].motion.intensity, Intensity::Strong);
        assert_eq!(spec.scenes[0].motion.easing, Easing::Steady);
        assert!(spec.scenes[0].has_shake);
        assert!(spec.scenes[0].effect_overridden);

        let plan = frame_plan(&spec, 30);
        assert_eq!(plan.layers.len(), 2);
        assert_eq!(plan.layers[0].kind, LayerKind::BlurredBackground);
        assert_eq!(plan.layers[1].kind, LayerKind::Image);
        assert!((plan.layers[0].opacity - 1.0).abs() < 1e-6);
        assert!((plan.layers[1].opacity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn flat_frame_plan_encoding_matches_layers() {
        let boundaries = vec![
            Boundary::new(0),
            Boundary::with_transition(5_000, TransitionKind::Crossfade, 800),
            Boundary::new(10_000),
        ];
        let spec = compile_spec(&sample_input(42, boundaries, FitMode::FitBlur, FitMode::Fill))
            .expect("compile crossfade with fit blur");
        let plan = frame_plan(&spec, 150);
        let flat = encode_frame_plan_flat(&plan);

        assert_eq!(flat.len(), FLAT_HEADER_LEN + plan.layers.len() * FLAT_LAYER_STRIDE);
        assert_eq!(flat[0], plan.fade_black_alpha);
        assert_eq!(flat[1], plan.primary_scene_index as f32);
        assert_eq!(flat[2], plan.layers.len() as f32);

        let image_opacity_sum: f32 = plan
            .layers
            .iter()
            .filter(|l| l.kind == LayerKind::Image)
            .map(|l| l.opacity)
            .sum();
        assert!((image_opacity_sum - 1.0).abs() < 1e-5);
    }
}
