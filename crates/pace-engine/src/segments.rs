//! Pure render segment planning and deterministic content-addressed cache keys
//! (`EXP-01..06`, `M8-2`, `M8-4`).
//!
//! Partitions a compiled `RenderSpec` into contiguous, non-overlapping frame ranges
//! (scene bodies + transition windows) so individual segments can be cached by content hash
//! and resumed or re-used cleanly across edits.

use crate::fit::{FitMode, SizeF};
use crate::motion::{FocalPoint, Motion, MotionPreset, SceneVisualEffect};
use crate::spec::{CompiledScene, RenderSpec};
use crate::stickers::{StickerAnimation, StickerKind, StickerPlacementSpec};
use crate::subtitles::{SubtitleCueSpec, SubtitleStyleSpec};
use crate::timing::TransitionKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Supported MP4 export resolution tiers (`EXP-01`, `M8-7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExportResolution {
    #[serde(rename = "360p")]
    R360p,
    #[serde(rename = "480p")]
    R480p,
    #[serde(rename = "720p")]
    R720p,
    #[serde(rename = "1080p")]
    R1080p,
}

impl ExportResolution {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "360p" => Some(Self::R360p),
            "480p" => Some(Self::R480p),
            "720p" => Some(Self::R720p),
            "1080p" => Some(Self::R1080p),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::R360p => "360p",
            Self::R480p => "480p",
            Self::R720p => "720p",
            Self::R1080p => "1080p",
        }
    }

    /// Returns even `(width, height)` dimensions for `"16:9"` or `"9:16"`.
    pub fn dimensions(self, aspect_ratio: &str) -> (u32, u32) {
        let (w16, h16) = match self {
            Self::R360p => (640, 360),
            Self::R480p => (854, 480),
            Self::R720p => (1280, 720),
            Self::R1080p => (1920, 1080),
        };
        if aspect_ratio == "9:16" {
            (h16, w16)
        } else {
            (w16, h16)
        }
    }
}

/// Role of a contiguous segment inside a render job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "segment_type", rename_all = "snake_case")]
pub enum SegmentKind {
    Body {
        scene_index: usize,
    },
    Transition {
        boundary_index: usize,
        kind: TransitionKind,
    },
}

/// One cacheable video segment corresponding to frame range `[start_frame, end_frame)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSegment {
    pub index: usize,
    pub kind: SegmentKind,
    pub start_frame: u32,
    pub end_frame: u32,
    pub frame_count: u32,
    pub cache_key: String,
}

#[derive(Serialize)]
struct SceneRenderSignature<'a> {
    media_id: &'a Option<String>,
    image_size: SizeF,
    fit_mode: FitMode,
    focal: FocalPoint,
    preset: MotionPreset,
    has_shake: bool,
    visual_effect: SceneVisualEffect,
    motion: &'a Motion,
    local_start_frame: i64,
    local_end_frame: i64,
    scene_frame_count: u32,
}

#[derive(Serialize)]
struct SegmentStickerSignature {
    kind: StickerKind,
    local_start_frame: i64,
    local_end_frame: i64,
    x_milli: i32,
    y_milli: i32,
    scale_milli: i32,
    animation: StickerAnimation,
}

#[derive(Serialize)]
struct SegmentSubtitleSignature<'a> {
    local_start_frame: u32,
    local_end_frame: u32,
    text: &'a str,
}

#[derive(Serialize)]
struct SegmentSubtitlesOverlay<'a> {
    style: &'a SubtitleStyleSpec,
    cues: Vec<SegmentSubtitleSignature<'a>>,
}

#[derive(Serialize)]
struct BodyCachePayload<'a> {
    kind: &'static str,
    width: u32,
    height: u32,
    fps: u32,
    scene: SceneRenderSignature<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitles: Option<SegmentSubtitlesOverlay<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stickers: Vec<SegmentStickerSignature>,
}

#[derive(Serialize)]
struct TransitionCachePayload<'a> {
    kind: &'static str,
    width: u32,
    height: u32,
    fps: u32,
    transition_kind: TransitionKind,
    duration_frames: u32,
    outgoing: SceneRenderSignature<'a>,
    incoming: SceneRenderSignature<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitles: Option<SegmentSubtitlesOverlay<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stickers: Vec<SegmentStickerSignature>,
}

fn overlapping_stickers_for_segment(
    seg_start: u32,
    seg_end: u32,
    fps: u32,
    stickers: &[StickerPlacementSpec],
) -> Vec<SegmentStickerSignature> {
    if stickers.is_empty() || seg_end <= seg_start {
        return Vec::new();
    }
    let mut out = Vec::new();
    for s in stickers {
        if s.end_ms <= s.start_ms {
            continue;
        }
        let (s_start, s_end) = s.frame_range(fps);
        if s_start < seg_end && s_end > seg_start {
            out.push(SegmentStickerSignature {
                kind: s.kind,
                local_start_frame: s_start as i64 - seg_start as i64,
                local_end_frame: s_end as i64 - seg_start as i64,
                x_milli: (s.x * 1000.0).round() as i32,
                y_milli: (s.y * 1000.0).round() as i32,
                scale_milli: (s.scale * 1000.0).round() as i32,
                animation: s.animation,
            });
        }
    }
    out
}

fn overlapping_subtitles_for_segment<'a>(
    seg_start: u32,
    seg_end: u32,
    fps: u32,
    subtitles: &'a [SubtitleCueSpec],
    style: &'a SubtitleStyleSpec,
) -> Option<SegmentSubtitlesOverlay<'a>> {
    if !style.enabled || subtitles.is_empty() || seg_end <= seg_start {
        return None;
    }

    let mut cues = Vec::new();
    for cue in subtitles {
        if cue.text.trim().is_empty() || cue.end_ms <= cue.start_ms {
            continue;
        }
        let (c_start, c_end) = cue.frame_range(fps);
        if c_start < seg_end && c_end > seg_start {
            let clamped_start = c_start.max(seg_start) - seg_start;
            let clamped_end = c_end.min(seg_end) - seg_start;
            cues.push(SegmentSubtitleSignature {
                local_start_frame: clamped_start,
                local_end_frame: clamped_end,
                text: cue.text.trim(),
            });
        }
    }

    if cues.is_empty() {
        None
    } else {
        Some(SegmentSubtitlesOverlay { style, cues })
    }
}

fn sha256_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

fn scene_signature<'a>(
    scene: &'a CompiledScene,
    global_start: u32,
    global_end: u32,
) -> SceneRenderSignature<'a> {
    SceneRenderSignature {
        media_id: &scene.media_id,
        image_size: scene.image_size,
        fit_mode: scene.fit_mode,
        focal: scene.focal,
        preset: scene.preset,
        has_shake: scene.has_shake,
        visual_effect: scene.visual_effect,
        motion: &scene.motion,
        local_start_frame: global_start as i64 - scene.start_frame as i64,
        local_end_frame: global_end as i64 - scene.start_frame as i64,
        scene_frame_count: scene.frame_count,
    }
}

/// Divides `spec` into contiguous, non-overlapping `RenderSegment`s (`M8-2`, `M8-4`).
pub fn plan_render_segments(spec: &RenderSpec) -> Vec<RenderSegment> {
    plan_render_segments_with_overlays(spec, &[], &SubtitleStyleSpec::default(), &spec.stickers)
}

/// Divides `spec` into segments while hashing overlapping subtitle cues so editing a single
/// subtitle cue invalidates only the segment(s) covering its timeframe (`M9-4`).
pub fn plan_render_segments_with_subtitles(
    spec: &RenderSpec,
    subtitles: &[SubtitleCueSpec],
    subtitle_style: &SubtitleStyleSpec,
) -> Vec<RenderSegment> {
    plan_render_segments_with_overlays(spec, subtitles, subtitle_style, &spec.stickers)
}

/// Divides `spec` into segments while hashing overlapping subtitles and timeline stickers.
pub fn plan_render_segments_with_overlays(
    spec: &RenderSpec,
    subtitles: &[SubtitleCueSpec],
    subtitle_style: &SubtitleStyleSpec,
    stickers: &[StickerPlacementSpec],
) -> Vec<RenderSegment> {
    if spec.total_frames == 0 || spec.scenes.is_empty() {
        return Vec::new();
    }

    let width = spec.frame_size.w.round().max(1.0) as u32;
    let height = spec.frame_size.h.round().max(1.0) as u32;
    let mut out = Vec::with_capacity(spec.scenes.len() + spec.transitions.len());

    for (s_idx, scene) in spec.scenes.iter().enumerate() {
        let left_tw = spec
            .transitions
            .iter()
            .find(|tw| tw.boundary_index == s_idx && tw.duration_frames > 0);
        let right_tw = spec
            .transitions
            .iter()
            .find(|tw| tw.boundary_index == s_idx + 1 && tw.duration_frames > 0);

        let body_start = left_tw.map(|tw| tw.end_frame).unwrap_or(scene.start_frame);
        let body_end = right_tw.map(|tw| tw.start_frame).unwrap_or(scene.end_frame);

        if body_end > body_start {
            let cache_key = sha256_json(&BodyCachePayload {
                kind: "body",
                width,
                height,
                fps: spec.fps,
                scene: scene_signature(scene, body_start, body_end),
                subtitles: overlapping_subtitles_for_segment(
                    body_start,
                    body_end,
                    spec.fps,
                    subtitles,
                    subtitle_style,
                ),
                stickers: overlapping_stickers_for_segment(
                    body_start,
                    body_end,
                    spec.fps,
                    stickers,
                ),
            });
            out.push(RenderSegment {
                index: out.len(),
                kind: SegmentKind::Body { scene_index: s_idx },
                start_frame: body_start,
                end_frame: body_end,
                frame_count: body_end - body_start,
                cache_key,
            });
        }

        if let Some(tw) = right_tw {
            if let Some(in_scene) = spec.scenes.get(s_idx + 1) {
                let cache_key = sha256_json(&TransitionCachePayload {
                    kind: "transition",
                    width,
                    height,
                    fps: spec.fps,
                    transition_kind: tw.kind,
                    duration_frames: tw.duration_frames,
                    outgoing: scene_signature(scene, tw.start_frame, tw.end_frame),
                    incoming: scene_signature(in_scene, tw.start_frame, tw.end_frame),
                    subtitles: overlapping_subtitles_for_segment(
                        tw.start_frame,
                        tw.end_frame,
                        spec.fps,
                        subtitles,
                        subtitle_style,
                    ),
                    stickers: overlapping_stickers_for_segment(
                        tw.start_frame,
                        tw.end_frame,
                        spec.fps,
                        stickers,
                    ),
                });
                out.push(RenderSegment {
                    index: out.len(),
                    kind: SegmentKind::Transition {
                        boundary_index: tw.boundary_index,
                        kind: tw.kind,
                    },
                    start_frame: tw.start_frame,
                    end_frame: tw.end_frame,
                    frame_count: tw.duration_frames,
                    cache_key,
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{compile_spec, CompileSpecInput, SceneEffectSettings, SceneSpecInput};
    use crate::timing::Boundary;

    fn make_ten_scene_input(scene7_preset: Option<MotionPreset>) -> CompileSpecInput {
        let mut boundaries = Vec::with_capacity(11);
        boundaries.push(Boundary::new(0));
        for i in 1..10 {
            let t_ms = (i as u64) * 3_000;
            if i == 7 || i == 8 {
                boundaries.push(Boundary::with_transition(t_ms, TransitionKind::Crossfade, 600));
            } else if i % 2 == 0 {
                boundaries.push(Boundary::with_transition(t_ms, TransitionKind::FadeBlack, 400));
            } else {
                boundaries.push(Boundary::new(t_ms));
            }
        }
        boundaries.push(Boundary::new(30_000));

        let scenes = (0..10)
            .map(|i| SceneSpecInput {
                scene_index: i,
                section_id: format!("sec-{i}"),
                media_id: Some(format!("med-{i}")),
                image_size: SizeF::new(1920.0, 1080.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings {
                    manual_preset: if i == 7 { scene7_preset } else { Some(MotionPreset::ZoomIn) },
                    ..Default::default()
                },
                effect_overridden: true,
            })
            .collect();

        CompileSpecInput {
            fps: 30,
            frame_size: SizeF::new(1280.0, 720.0),
            narration_ms: 30_000,
            effect_seed: 42,
            include_tilt_in_auto: false,
            boundaries,
            scenes,
            stickers: Vec::new(),
        }
    }

    #[test]
    fn resolution_dimensions_cover_all_eight_combinations_with_even_sizes() {
        let combos = [
            ("360p", "16:9", (640, 360)),
            ("480p", "16:9", (854, 480)),
            ("720p", "16:9", (1280, 720)),
            ("1080p", "16:9", (1920, 1080)),
            ("360p", "9:16", (360, 640)),
            ("480p", "9:16", (480, 854)),
            ("720p", "9:16", (720, 1280)),
            ("1080p", "9:16", (1080, 1920)),
        ];
        for (res_str, aspect, expected) in combos {
            let res = ExportResolution::parse(res_str).expect("valid resolution");
            let dims = res.dimensions(aspect);
            assert_eq!(dims, expected);
            assert_eq!(dims.0 % 2, 0);
            assert_eq!(dims.1 % 2, 0);
        }
    }

    #[test]
    fn segments_partition_timeline_without_gaps_or_overlaps() {
        let spec = compile_spec(&make_ten_scene_input(Some(MotionPreset::ZoomIn))).unwrap();
        let segs = plan_render_segments(&spec);

        assert!(!segs.is_empty());
        assert_eq!(segs.first().unwrap().start_frame, 0);
        assert_eq!(segs.last().unwrap().end_frame, spec.total_frames);

        let mut total_frames = 0_u32;
        for pair in segs.windows(2) {
            assert_eq!(pair[0].end_frame, pair[1].start_frame);
            assert_eq!(pair[0].index + 1, pair[1].index);
        }
        for s in &segs {
            assert!(s.frame_count > 0);
            assert_eq!(s.frame_count, s.end_frame - s.start_frame);
            assert_eq!(s.cache_key.len(), 64);
            total_frames += s.frame_count;
        }
        assert_eq!(total_frames, spec.total_frames);
    }

    #[test]
    fn editing_scene_7_invalidates_only_three_segments() {
        let spec_a = compile_spec(&make_ten_scene_input(Some(MotionPreset::ZoomIn))).unwrap();
        let spec_b = compile_spec(&make_ten_scene_input(Some(MotionPreset::PanLeft))).unwrap();

        let segs_a = plan_render_segments(&spec_a);
        let segs_b = plan_render_segments(&spec_b);

        assert_eq!(segs_a.len(), segs_b.len());
        let changed: Vec<SegmentKind> = segs_a
            .iter()
            .zip(&segs_b)
            .filter(|(a, b)| a.cache_key != b.cache_key)
            .map(|(a, _)| a.kind)
            .collect();

        // M8 TDD invariant: editing scene 7 invalidates ONLY Body(7) + boundary 7 & 8 transitions
        assert_eq!(changed.len(), 3);
        assert!(changed.contains(&SegmentKind::Transition {
            boundary_index: 7,
            kind: TransitionKind::Crossfade,
        }));
        assert!(changed.contains(&SegmentKind::Body { scene_index: 7 }));
        assert!(changed.contains(&SegmentKind::Transition {
            boundary_index: 8,
            kind: TransitionKind::Crossfade,
        }));
    }

    #[test]
    fn editing_one_subtitle_cue_only_invalidates_segments_covering_its_timeframe() {
        let spec = compile_spec(&make_ten_scene_input(Some(MotionPreset::ZoomIn))).unwrap();
        let style = SubtitleStyleSpec::default();

        let cues_a = vec![
            SubtitleCueSpec {
                id: "cue-0".to_string(),
                position: 0,
                start_ms: 500,
                end_ms: 2_200,
                text: "First subtitle in scene zero.".to_string(),
            },
            SubtitleCueSpec {
                id: "cue-1".to_string(),
                position: 1,
                start_ms: 9_500,
                end_ms: 11_200,
                text: "Second subtitle strictly inside scene three.".to_string(),
            },
        ];

        let mut cues_b = cues_a.clone();
        cues_b[1].text = "Edited subtitle strictly inside scene three.".to_string();

        let segs_a = plan_render_segments_with_subtitles(&spec, &cues_a, &style);
        let segs_b = plan_render_segments_with_subtitles(&spec, &cues_b, &style);

        let changed: Vec<SegmentKind> = segs_a
            .iter()
            .zip(&segs_b)
            .filter(|(a, b)| a.cache_key != b.cache_key)
            .map(|(a, _)| a.kind)
            .collect();

        assert_eq!(changed, vec![SegmentKind::Body { scene_index: 3 }]);
    }
}
