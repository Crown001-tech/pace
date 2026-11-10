//! Timing and frame calculations (`TML-05..07`, `EFX-06`, TDD 12.1).
//!
//! Converts absolute scene-change boundary times (`time_ms`) into frame numbers
//! independently so rounding errors never accumulate (zero drift over 40 minutes).

use serde::{Deserialize, Serialize};

/// Standard frame rate for PACE v1 (`30` frames per second).
pub const DEFAULT_FPS: u32 = 30;

/// Minimum allowed scene duration in milliseconds (`TML-05`: `0.5 s` = `500 ms`).
pub const MIN_SCENE_DURATION_MS: u64 = 500;

/// Minimum number of non-transition ("body") frames a scene must keep on its own (TDD 12.1).
pub const MIN_SCENE_BODY_FRAMES: u32 = 6;

/// Visual transition type at an interior boundary (`EFX-06`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    Cut,
    Crossfade,
    FadeBlack,
}

/// Transition configuration attached to a boundary (`EFX-06`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryTransition {
    pub kind: TransitionKind,
    /// Requested transition duration in milliseconds (default `500 ms` for Crossfade).
    pub duration_ms: u64,
}

impl Default for BoundaryTransition {
    fn default() -> Self {
        Self {
            kind: TransitionKind::Crossfade,
            duration_ms: 500,
        }
    }
}

/// A scene-change point on the timeline (`TDD 5.1`, `12.1`).
///
/// For `N` scenes, there are `N + 1` boundaries:
/// - `boundaries[0]` is the start of scene `0` (`0 ms`, `TML-06`).
/// - `boundaries[i]` (`1 <= i < N`) sits between scene `i - 1` and scene `i`.
/// - `boundaries[N]` is the end of scene `N - 1` (`narration_ms`, `TML-06`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boundary {
    pub time_ms: u64,
    /// If true, the scene starting at this boundary was flagged as never spoken (`ALN-06`, `TML-05`)
    /// and is allowed to have `0` length.
    pub allow_zero_length: bool,
    /// Optional transition centred on this interior boundary.
    pub transition: Option<BoundaryTransition>,
}

impl Boundary {
    pub fn new(time_ms: u64) -> Self {
        Self {
            time_ms,
            allow_zero_length: false,
            transition: None,
        }
    }

    pub fn with_transition(time_ms: u64, kind: TransitionKind, duration_ms: u64) -> Self {
        Self {
            time_ms,
            allow_zero_length: false,
            transition: Some(BoundaryTransition { kind, duration_ms }),
        }
    }
}

/// Exact frame range for a single scene (`TDD 12.1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneFrames {
    pub scene_index: usize,
    /// Inclusive start frame (`frame(b[i])`).
    pub start_frame: u32,
    /// Exclusive end frame (`frame(b[i + 1])`).
    pub end_frame: u32,
    /// Total frames in this scene (`end_frame - start_frame`).
    pub frame_count: u32,
    /// True if a transition touching this scene had to be shortened so the scene
    /// could keep at least `MIN_SCENE_BODY_FRAMES` (6) frames on its own (`TDD 12.1`).
    pub transition_shortened: bool,
}

/// Computed frame window for a transition centred on interior boundary `boundary_index` (`TDD 12.1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionWindow {
    /// Interior boundary index (`1..N`).
    pub boundary_index: usize,
    pub kind: TransitionKind,
    /// Frame number of the boundary (`center_frame = frame(b[i])`).
    pub center_frame: u32,
    /// Even number of frames (`d`) covered by the transition after any shortening.
    pub duration_frames: u32,
    /// Inclusive start frame (`center_frame - d / 2`).
    pub start_frame: u32,
    /// Exclusive end frame (`center_frame + d / 2`).
    pub end_frame: u32,
    /// True if `duration_frames` was shortened below the requested even frame count.
    pub shortened: bool,
}

/// Validation errors for timeline boundaries (`TML-05`, `TML-06`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimingError {
    /// At least 2 boundaries (1 scene) are required.
    TooFewBoundaries { count: usize },
    /// Boundary 0 must be at `0 ms` (`TML-06`).
    FirstBoundaryNotZero { actual_ms: u64 },
    /// The final boundary must equal `narration_ms` (`TML-06`).
    LastBoundaryMismatch { actual_ms: u64, narration_ms: u64 },
    /// Boundaries must be in non-decreasing time order.
    OutOfOrder {
        index: usize,
        prev_ms: u64,
        current_ms: u64,
    },
    /// Scene is shorter than `MIN_SCENE_DURATION_MS` (`500 ms`) and not marked unspoken (`TML-05`).
    SceneTooShort {
        scene_index: usize,
        duration_ms: u64,
        min_ms: u64,
    },
    /// Attempted to drag/move the first (`0`) or last (`N`) boundary (`TML-06`).
    CannotMoveEndpointBoundary { index: usize },
}

/// Converts a single time in milliseconds to its nearest frame number at `fps` (`TDD 12.1`).
///
/// Formula: `round(time_ms * fps / 1000)` computed per boundary so rounding errors never accumulate.
pub fn time_ms_to_frame(time_ms: u64, fps: u32) -> u32 {
    ((time_ms * (fps as u64) + 500) / 1_000) as u32
}

/// Converts a list of boundaries into their individual frame numbers (`TDD 12.1`).
pub fn boundaries_to_frames(boundaries: &[Boundary], fps: u32) -> Vec<u32> {
    boundaries
        .iter()
        .map(|b| time_ms_to_frame(b.time_ms, fps))
        .collect()
}

/// Validates boundary rules (`TML-05`, `TML-06`):
/// - At least 2 boundaries (`>= 1` scene).
/// - `boundaries[0].time_ms == 0`.
/// - `boundaries.last().time_ms == narration_ms`.
/// - Monotonically non-decreasing times.
/// - Every scene is at least `MIN_SCENE_DURATION_MS` (`500 ms`) unless `allow_zero_length` is true.
pub fn validate_boundaries(boundaries: &[Boundary], narration_ms: u64) -> Result<(), TimingError> {
    if boundaries.len() < 2 {
        return Err(TimingError::TooFewBoundaries {
            count: boundaries.len(),
        });
    }

    if boundaries[0].time_ms != 0 {
        return Err(TimingError::FirstBoundaryNotZero {
            actual_ms: boundaries[0].time_ms,
        });
    }

    let last_ms = boundaries.last().unwrap().time_ms;
    if last_ms != narration_ms {
        return Err(TimingError::LastBoundaryMismatch {
            actual_ms: last_ms,
            narration_ms,
        });
    }

    for i in 0..(boundaries.len() - 1) {
        let start_ms = boundaries[i].time_ms;
        let end_ms = boundaries[i + 1].time_ms;
        if end_ms < start_ms {
            return Err(TimingError::OutOfOrder {
                index: i + 1,
                prev_ms: start_ms,
                current_ms: end_ms,
            });
        }

        let duration_ms = end_ms - start_ms;
        if duration_ms < MIN_SCENE_DURATION_MS && !boundaries[i].allow_zero_length {
            return Err(TimingError::SceneTooShort {
                scene_index: i,
                duration_ms,
                min_ms: MIN_SCENE_DURATION_MS,
            });
        }
    }

    Ok(())
}

/// Validates `TML-05..06` and calculates each scene's frame range and transition windows (`TDD 12.1`).
pub fn compute_timeline_frames(
    boundaries: &[Boundary],
    narration_ms: u64,
    fps: u32,
) -> Result<(Vec<SceneFrames>, Vec<TransitionWindow>), TimingError> {
    validate_boundaries(boundaries, narration_ms)?;

    let frames = boundaries_to_frames(boundaries, fps);
    let scene_count = boundaries.len() - 1;

    let mut scenes: Vec<SceneFrames> = (0..scene_count)
        .map(|i| {
            let start_frame = frames[i];
            let end_frame = frames[i + 1];
            SceneFrames {
                scene_index: i,
                start_frame,
                end_frame,
                frame_count: end_frame - start_frame,
                transition_shortened: false,
            }
        })
        .collect();

    // Determine which interior boundaries (1..scene_count) have an active non-cut transition
    let is_active_transition = |b: &Boundary| -> bool {
        matches!(
            b.transition,
            Some(BoundaryTransition {
                kind: TransitionKind::Crossfade | TransitionKind::FadeBlack,
                duration_ms
            }) if duration_ms > 0
        )
    };

    // For each scene, count how many active transitions touch its left/right edges
    let mut max_half_for_scene = vec![0u32; scene_count];
    for s in 0..scene_count {
        let left_active = s > 0 && is_active_transition(&boundaries[s]);
        let right_active = (s + 1) < scene_count && is_active_transition(&boundaries[s + 1]);
        let touching = (left_active as u32) + (right_active as u32);
        let available = scenes[s].frame_count.saturating_sub(MIN_SCENE_BODY_FRAMES);
        max_half_for_scene[s] = available.checked_div(touching).unwrap_or(available);
    }

    let mut transitions = Vec::new();
    for boundary_index in 1..scene_count {
        if let Some(t) = boundaries[boundary_index].transition {
            if t.kind == TransitionKind::Cut || t.duration_ms == 0 {
                continue;
            }
            let raw_frames = time_ms_to_frame(t.duration_ms, fps);
            // Round requested frames down to an even number (d = 2 * req_half)
            let req_half = raw_frames / 2;
            let left_max = max_half_for_scene[boundary_index - 1];
            let right_max = max_half_for_scene[boundary_index];

            let actual_half = req_half.min(left_max).min(right_max);
            let duration_frames = actual_half * 2;
            let shortened = actual_half < req_half;

            if req_half > left_max {
                scenes[boundary_index - 1].transition_shortened = true;
            }
            if req_half > right_max {
                scenes[boundary_index].transition_shortened = true;
            }

            let center_frame = frames[boundary_index];
            transitions.push(TransitionWindow {
                boundary_index,
                kind: t.kind,
                center_frame,
                duration_frames,
                start_frame: center_frame - actual_half,
                end_frame: center_frame + actual_half,
                shortened,
            });
        }
    }

    Ok((scenes, transitions))
}

/// Moves interior boundary `index` (`1..N`) to `desired_time_ms`, clamping it so
/// neither neighbouring scene becomes shorter than `MIN_SCENE_DURATION_MS` (`500 ms`, `TML-02`, `TML-05`).
pub fn move_boundary(
    boundaries: &mut [Boundary],
    index: usize,
    desired_time_ms: u64,
) -> Result<u64, TimingError> {
    if boundaries.len() < 2 {
        return Err(TimingError::TooFewBoundaries {
            count: boundaries.len(),
        });
    }
    if index == 0 || index >= boundaries.len() - 1 {
        return Err(TimingError::CannotMoveEndpointBoundary { index });
    }

    let min_allowed = boundaries[index - 1].time_ms + MIN_SCENE_DURATION_MS;
    let max_allowed = boundaries[index + 1]
        .time_ms
        .saturating_sub(MIN_SCENE_DURATION_MS);

    let clamped = desired_time_ms.clamp(min_allowed, max_allowed);
    boundaries[index].time_ms = clamped;
    Ok(clamped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn converts_single_boundary_to_nearest_frame_at_30fps() {
        assert_eq!(time_ms_to_frame(0, 30), 0);
        assert_eq!(time_ms_to_frame(1_000, 30), 30);
        // 16 ms * 30 / 1000 = 0.48 -> rounds to 0
        assert_eq!(time_ms_to_frame(16, 30), 0);
        // 17 ms * 30 / 1000 = 0.51 -> rounds to 1
        assert_eq!(time_ms_to_frame(17, 30), 1);
        // 40 minutes = 2,400,000 ms -> 72,000 frames
        assert_eq!(time_ms_to_frame(2_400_000, 30), 72_000);
    }

    #[test]
    fn enforces_first_zero_last_narration_and_min_scene_length() {
        let narration_ms = 10_000;

        // First boundary not 0 -> error
        let bad_first = vec![Boundary::new(100), Boundary::new(10_000)];
        assert_eq!(
            validate_boundaries(&bad_first, narration_ms),
            Err(TimingError::FirstBoundaryNotZero { actual_ms: 100 })
        );

        // Last boundary not narration_ms -> error
        let bad_last = vec![Boundary::new(0), Boundary::new(9_500)];
        assert_eq!(
            validate_boundaries(&bad_last, narration_ms),
            Err(TimingError::LastBoundaryMismatch {
                actual_ms: 9_500,
                narration_ms: 10_000,
            })
        );

        // Scene shorter than 500 ms (and not unspoken) -> error
        let too_short = vec![Boundary::new(0), Boundary::new(400), Boundary::new(10_000)];
        assert_eq!(
            validate_boundaries(&too_short, narration_ms),
            Err(TimingError::SceneTooShort {
                scene_index: 0,
                duration_ms: 400,
                min_ms: 500,
            })
        );

        // Unspoken section (allow_zero_length = true) with 0 ms length -> allowed (ALN-06, TML-05)
        let mut unspoken = Boundary::new(2_000);
        unspoken.allow_zero_length = true;
        let with_unspoken = vec![
            Boundary::new(0),
            unspoken,
            Boundary::new(2_000),
            Boundary::new(10_000),
        ];
        assert!(validate_boundaries(&with_unspoken, narration_ms).is_ok());
    }

    #[test]
    fn moving_one_boundary_changes_only_two_scenes() {
        let narration_ms = 20_000;
        let mut boundaries = vec![
            Boundary::new(0),
            Boundary::new(4_000),
            Boundary::new(9_000),
            Boundary::new(14_000),
            Boundary::new(20_000),
        ];

        let (before_scenes, _) =
            compute_timeline_frames(&boundaries, narration_ms, 30).expect("valid before");

        // Move interior boundary 2 (between scene 1 and scene 2) from 9,000 ms to 10,500 ms
        let applied_ms = move_boundary(&mut boundaries, 2, 10_500).expect("move ok");
        assert_eq!(applied_ms, 10_500);

        let (after_scenes, _) =
            compute_timeline_frames(&boundaries, narration_ms, 30).expect("valid after");

        // Scene 0 and Scene 3 must be completely untouched
        assert_eq!(before_scenes[0], after_scenes[0]);
        assert_eq!(before_scenes[3], after_scenes[3]);

        // Only Scene 1 and Scene 2 changed
        assert_ne!(before_scenes[1], after_scenes[1]);
        assert_ne!(before_scenes[2], after_scenes[2]);
        assert_eq!(after_scenes[1].end_frame, time_ms_to_frame(10_500, 30));
        assert_eq!(after_scenes[2].start_frame, time_ms_to_frame(10_500, 30));

        // Dragging past minimum scene length (500 ms from boundary 3 at 14,000 ms) clamps at 13,500 ms
        let clamped_ms = move_boundary(&mut boundaries, 2, 13_900).expect("clamped move ok");
        assert_eq!(clamped_ms, 13_500);
    }

    #[test]
    fn transition_windows_are_centred_even_and_shortened_for_short_scenes() {
        // Scene 0: 0..500 ms (15 frames: 0..15)
        // Scene 1: 500..1000 ms (15 frames: 15..30)
        // Scene 2: 1000..5000 ms (120 frames: 30..150)
        let boundaries = vec![
            Boundary::new(0),
            Boundary::with_transition(500, TransitionKind::Crossfade, 1_000),
            Boundary::with_transition(1_000, TransitionKind::Crossfade, 1_000),
            Boundary::new(5_000),
        ];

        let (scenes, transitions) =
            compute_timeline_frames(&boundaries, 5_000, 30).expect("valid timeline");

        assert_eq!(transitions.len(), 2);
        for t in &transitions {
            assert_eq!(t.duration_frames % 2, 0, "transition frames must be even");
            assert_eq!(t.center_frame - t.start_frame, t.duration_frames / 2);
            assert_eq!(t.end_frame - t.center_frame, t.duration_frames / 2);
            assert!(
                t.shortened,
                "30-frame transition must be shortened for 15-frame scene"
            );
        }

        // Verify Scene 1 keeps at least 6 body frames on its own
        let scene1_left_half = transitions[0].duration_frames / 2;
        let scene1_right_half = transitions[1].duration_frames / 2;
        let scene1_body = scenes[1].frame_count - scene1_left_half - scene1_right_half;
        assert!(
            scene1_body >= MIN_SCENE_BODY_FRAMES,
            "scene 1 body frames ({scene1_body}) must be >= 6"
        );
        assert!(scenes[1].transition_shortened);
    }

    proptest! {
        #[test]
        fn no_drift_over_40_minutes(
            // Generate 300 scene durations in [500 ms, 12_000 ms] (~30 to 40 minutes total)
            durations in proptest::collection::vec(500u64..=12_000u64, 300)
        ) {
            let mut boundaries = Vec::with_capacity(durations.len() + 1);
            let mut acc_ms = 0u64;
            boundaries.push(Boundary::new(0));
            for d in durations {
                acc_ms += d;
                boundaries.push(Boundary::new(acc_ms));
            }
            let narration_ms = acc_ms;

            let (scenes, _) = compute_timeline_frames(&boundaries, narration_ms, DEFAULT_FPS)
                .expect("generated boundaries must be valid");

            let total_frames = time_ms_to_frame(narration_ms, DEFAULT_FPS);

            // 1. Every boundary frame is within 0.5 frames of its exact time
            for (i, b) in boundaries.iter().enumerate() {
                let exact_frame = (b.time_ms as f64) * (DEFAULT_FPS as f64) / 1000.0;
                let frame = if i < scenes.len() {
                    scenes[i].start_frame
                } else {
                    scenes.last().unwrap().end_frame
                };
                let diff = (frame as f64 - exact_frame).abs();
                prop_assert!(
                    diff <= 0.5 + 1e-9,
                    "Boundary {i} at {} ms drifted by {diff} frames",
                    b.time_ms
                );
            }

            // 2. No negative/zero lengths and scene frame counts sum to total_frames
            let mut sum_frames = 0u32;
            for s in &scenes {
                prop_assert!(s.end_frame >= s.start_frame);
                prop_assert_eq!(s.frame_count, s.end_frame - s.start_frame);
                sum_frames += s.frame_count;
            }
            prop_assert_eq!(sum_frames, total_frames);
        }
    }
}
