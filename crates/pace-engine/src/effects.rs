//! Automatic effect variety and assignment (`EFX-02`, `EFX-07`, `EFX-08`, TDD 12.3).
//!
//! Uses a seeded pseudo-random number generator (`rand_pcg::Pcg64`) to assign
//! camera motion presets and subtle camera shake across scenes according to PRD rules:
//! - Deterministic: the same seed always gives identical results.
//! - Never one of the previous two presets (`EFX-02`).
//! - No preset exceeds 35% of total scenes (`EFX-02`).
//! - Pans lean towards the side with more image room beyond the focal point (`TDD 12.3`).
//! - Subtle camera shake is applied with 1/8 probability and never on adjacent scenes (`EFX-07`).
//! - Preserves manual overrides.

use crate::motion::{build_motion, Easing, FocalPoint, Intensity, Motion, MotionPreset, Shake};
use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg64;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum percentage of scenes any single preset can occupy (`EFX-02`: `35%`).
pub const MAX_PRESET_FREQUENCY_RATIO: f32 = 0.35;

/// Probability denominator for automatic camera shake (`EFX-07`: `1 in 8`).
pub const SHAKE_PROBABILITY_DENOMINATOR: u32 = 8;

const ALL_PRESETS: [MotionPreset; 15] = [
    MotionPreset::Still,
    MotionPreset::ZoomIn,
    MotionPreset::ZoomOut,
    MotionPreset::PanLeft,
    MotionPreset::PanRight,
    MotionPreset::PanUp,
    MotionPreset::PanDown,
    MotionPreset::ZoomInPanLeft,
    MotionPreset::ZoomInPanRight,
    MotionPreset::ZoomInPanUp,
    MotionPreset::ZoomInPanDown,
    MotionPreset::ZoomOutPanLeft,
    MotionPreset::ZoomOutPanRight,
    MotionPreset::ZoomOutPanUp,
    MotionPreset::ZoomOutPanDown,
];

/// Input specification for a scene when assigning effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectInput {
    pub scene_index: usize,
    pub duration_ms: u64,
    pub focal: FocalPoint,
    /// Manual override preset if set by hand (`EFX-03`).
    pub manual_preset: Option<MotionPreset>,
    /// Manual override shake if set by hand (`EFX-07`).
    pub manual_shake: Option<bool>,
}

impl SceneEffectInput {
    pub fn new(scene_index: usize, duration_ms: u64, focal: FocalPoint) -> Self {
        Self {
            scene_index,
            duration_ms,
            focal,
            manual_preset: None,
            manual_shake: None,
        }
    }
}

/// Assigned camera motion and shake for a scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectAssignment {
    pub scene_index: usize,
    pub preset: MotionPreset,
    pub has_shake: bool,
    pub motion: Motion,
}

/// Re-rolls the effect seed to produce a new deterministic variety pass (`EFX-08`).
pub fn reroll_seed(seed: u64) -> u64 {
    seed.wrapping_add(1)
}

fn calculate_preset_weight(preset: MotionPreset, focal: FocalPoint) -> u32 {
    let mut weight = 10u32;

    // Horizontal room bias
    if focal.x < 0.45 {
        // Focal on left -> more image on right -> lean towards PanRight
        match preset {
            MotionPreset::PanRight
            | MotionPreset::ZoomInPanRight
            | MotionPreset::ZoomOutPanRight => weight += 25,
            MotionPreset::PanLeft | MotionPreset::ZoomInPanLeft | MotionPreset::ZoomOutPanLeft => {
                weight = weight.saturating_sub(8).max(2)
            }
            _ => {}
        }
    } else if focal.x > 0.55 {
        // Focal on right -> more image on left -> lean towards PanLeft
        match preset {
            MotionPreset::PanLeft | MotionPreset::ZoomInPanLeft | MotionPreset::ZoomOutPanLeft => {
                weight += 25
            }
            MotionPreset::PanRight
            | MotionPreset::ZoomInPanRight
            | MotionPreset::ZoomOutPanRight => weight = weight.saturating_sub(8).max(2),
            _ => {}
        }
    }

    // Vertical room bias
    if focal.y < 0.45 {
        // Focal near top -> more image below -> lean towards PanDown
        match preset {
            MotionPreset::PanDown | MotionPreset::ZoomInPanDown | MotionPreset::ZoomOutPanDown => {
                weight += 25
            }
            MotionPreset::PanUp | MotionPreset::ZoomInPanUp | MotionPreset::ZoomOutPanUp => {
                weight = weight.saturating_sub(8).max(2)
            }
            _ => {}
        }
    } else if focal.y > 0.55 {
        // Focal near bottom -> more image above -> lean towards PanUp
        match preset {
            MotionPreset::PanUp | MotionPreset::ZoomInPanUp | MotionPreset::ZoomOutPanUp => {
                weight += 25
            }
            MotionPreset::PanDown | MotionPreset::ZoomInPanDown | MotionPreset::ZoomOutPanDown => {
                weight = weight.saturating_sub(8).max(2)
            }
            _ => {}
        }
    }

    weight
}

/// Assigns automatic motion presets and camera shake to scenes (`EFX-02`, `EFX-07`, `TDD 12.3`).
pub fn assign_automatic_effects(
    scenes: &[SceneEffectInput],
    seed: u64,
) -> Vec<SceneEffectAssignment> {
    let mut rng = Pcg64::seed_from_u64(seed);
    let mut assignments = Vec::with_capacity(scenes.len());

    let mut preset_counts: HashMap<MotionPreset, usize> = HashMap::new();
    let mut prev_preset_1: Option<MotionPreset> = None;
    let mut prev_preset_2: Option<MotionPreset> = None;
    let mut prev_had_shake = false;

    let max_count_allowed = ((scenes.len() as f32) * MAX_PRESET_FREQUENCY_RATIO).ceil() as usize;
    let max_count = max_count_allowed.max(1);

    for scene in scenes {
        let chosen_preset = if let Some(manual) = scene.manual_preset {
            manual
        } else {
            // Filter 1: not one of previous two, and within 35% cap
            let mut candidates: Vec<MotionPreset> = ALL_PRESETS
                .iter()
                .copied()
                .filter(|&p| {
                    Some(p) != prev_preset_1
                        && Some(p) != prev_preset_2
                        && *preset_counts.get(&p).unwrap_or(&0) < max_count
                })
                .collect();

            // Fallback 1: if frequency cap exhausted all, relax count limit but keep previous 2 exclusion
            if candidates.is_empty() {
                candidates = ALL_PRESETS
                    .iter()
                    .copied()
                    .filter(|&p| Some(p) != prev_preset_1 && Some(p) != prev_preset_2)
                    .collect();
            }

            // Fallback 2: if still empty (e.g. tiny pool), relax prev_preset_2
            if candidates.is_empty() {
                candidates = ALL_PRESETS
                    .iter()
                    .copied()
                    .filter(|&p| Some(p) != prev_preset_1)
                    .collect();
            }

            // Fallback 3: all presets
            if candidates.is_empty() {
                candidates = ALL_PRESETS.to_vec();
            }

            // Weighted choice
            let weights: Vec<u32> = candidates
                .iter()
                .map(|&p| calculate_preset_weight(p, scene.focal))
                .collect();
            let total_weight: u32 = weights.iter().sum();
            let mut pick = (rng.next_u32() % total_weight) + 1;
            let mut chosen = candidates[0];
            for (candidate, &w) in candidates.iter().zip(&weights) {
                if pick <= w {
                    chosen = *candidate;
                    break;
                }
                pick -= w;
            }
            chosen
        };

        let has_shake = if let Some(manual_shake) = scene.manual_shake {
            manual_shake
        } else if prev_had_shake {
            false
        } else {
            (rng.next_u32() % SHAKE_PROBABILITY_DENOMINATOR) == 0
        };

        *preset_counts.entry(chosen_preset).or_insert(0) += 1;
        prev_preset_2 = prev_preset_1;
        prev_preset_1 = Some(chosen_preset);
        prev_had_shake = has_shake;

        let shake_opt = if has_shake {
            Some(Shake::default())
        } else {
            None
        };
        let motion = build_motion(
            chosen_preset,
            scene.focal,
            scene.duration_ms,
            Intensity::Normal,
            Easing::EaseInOut,
            shake_opt,
        );

        assignments.push(SceneEffectAssignment {
            scene_index: scene.scene_index,
            preset: chosen_preset,
            has_shake,
            motion,
        });
    }

    assignments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_scenes(count: usize) -> Vec<SceneEffectInput> {
        (0..count)
            .map(|i| SceneEffectInput::new(i, 5_000, FocalPoint::default()))
            .collect()
    }

    #[test]
    fn same_seed_gives_identical_result() {
        let scenes = make_test_scenes(30);
        let run1 = assign_automatic_effects(&scenes, 42);
        let run2 = assign_automatic_effects(&scenes, 42);
        assert_eq!(run1, run2);
    }

    #[test]
    fn reroll_changes_assignment() {
        let scenes = make_test_scenes(30);
        let seed = 100;
        let run1 = assign_automatic_effects(&scenes, seed);
        let run2 = assign_automatic_effects(&scenes, reroll_seed(seed));
        assert_ne!(run1, run2);
    }

    #[test]
    fn never_same_preset_more_than_twice_in_a_row() {
        let scenes = make_test_scenes(60);
        let assignments = assign_automatic_effects(&scenes, 12345);

        for i in 2..assignments.len() {
            let p0 = assignments[i - 2].preset;
            let p1 = assignments[i - 1].preset;
            let p2 = assignments[i].preset;
            // EFX-02 rule: never the same preset on more than 2 scenes in a row
            // (And TDD 12.3 rule: never one of the previous two)
            assert!(
                !(p2 == p1 && p1 == p0),
                "Preset {:?} appeared 3 times in a row at scene {}",
                p2,
                i
            );
            assert_ne!(p2, p1, "Preset {:?} matched previous scene at {}", p2, i);
            assert_ne!(
                p2, p0,
                "Preset {:?} matched scene 2 steps back at {}",
                p2, i
            );
        }
    }

    #[test]
    fn no_preset_exceeds_35_percent_of_scenes() {
        let scenes = make_test_scenes(50);
        let assignments = assign_automatic_effects(&scenes, 999);

        let mut counts = std::collections::HashMap::new();
        for a in &assignments {
            *counts.entry(a.preset).or_insert(0usize) += 1;
        }

        let max_allowed = (50.0_f32 * MAX_PRESET_FREQUENCY_RATIO).ceil() as usize;
        for (preset, count) in counts {
            assert!(
                count <= max_allowed,
                "Preset {:?} appeared {} times, exceeding 35% cap ({})",
                preset,
                count,
                max_allowed
            );
        }
    }

    #[test]
    fn shake_never_on_adjacent_scenes_and_frequency_bounded() {
        let scenes = make_test_scenes(100);
        let assignments = assign_automatic_effects(&scenes, 777);

        let mut shake_count = 0;
        for i in 0..assignments.len() {
            if assignments[i].has_shake {
                shake_count += 1;
                if i > 0 {
                    assert!(
                        !assignments[i - 1].has_shake,
                        "Adjacent scenes {} and {} both had shake",
                        i - 1,
                        i
                    );
                }
            }
        }
        // Shake is 1/8 (~12.5%). Over 100 scenes, should be roughly 8..20.
        assert!(
            (5..=25).contains(&shake_count),
            "Shake count was {shake_count}"
        );
    }

    #[test]
    fn manual_overrides_are_respected() {
        let mut scenes = make_test_scenes(20);
        scenes[5].manual_preset = Some(MotionPreset::Still);
        scenes[5].manual_shake = Some(true);
        scenes[10].manual_preset = Some(MotionPreset::ZoomOut);
        scenes[10].manual_shake = Some(false);

        let assignments = assign_automatic_effects(&scenes, 555);

        assert_eq!(assignments[5].preset, MotionPreset::Still);
        assert!(assignments[5].has_shake);

        assert_eq!(assignments[10].preset, MotionPreset::ZoomOut);
        assert!(!assignments[10].has_shake);
    }

    #[test]
    fn pan_direction_biases_toward_available_room() {
        // All scenes have focal point near the left (x = 0.15) -> more room on the right
        let scenes: Vec<_> = (0..60)
            .map(|i| SceneEffectInput::new(i, 5_000, FocalPoint { x: 0.15, y: 0.5 }))
            .collect();

        let assignments = assign_automatic_effects(&scenes, 4321);

        let mut right_pans = 0;
        let mut left_pans = 0;
        for a in assignments {
            match a.preset {
                MotionPreset::PanRight
                | MotionPreset::ZoomInPanRight
                | MotionPreset::ZoomOutPanRight => right_pans += 1,
                MotionPreset::PanLeft
                | MotionPreset::ZoomInPanLeft
                | MotionPreset::ZoomOutPanLeft => left_pans += 1,
                _ => {}
            }
        }

        assert!(
            right_pans > left_pans,
            "Expected right pans ({right_pans}) > left pans ({left_pans}) when focal point is on the left"
        );
    }
}
