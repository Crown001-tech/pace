//! Camera motion model and presets (`EFX-01`, `EFX-03`, `EFX-07`, TDD 12.2).
//!
//! Models a camera window moving across an image from a start state to an end state.
//! Supports presets (Still, Zooms, Pans, and Combinations), easing curves,
//! strength multipliers, zoom speed capping, and subtle camera shake.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Maximum zoom speed per second (`TDD 12.2`: `0.012` per second).
pub const MAX_ZOOM_SPEED_PER_SEC: f32 = 0.012;

/// Default zoom for `Still` preset to eliminate edge artifacts (`TDD 12.2`: `1.04`).
pub const STILL_ZOOM: f32 = 1.04;

/// Base target zoom for ZoomIn / ZoomOut (`TDD 12.2`: `1.18`).
pub const BASE_ZOOM_TARGET: f32 = 1.18;

/// Pan preset fixed zoom (`TDD 12.2`: `1.15`).
pub const PAN_ZOOM: f32 = 1.15;

/// Share of available room traversed in pure pans (`TDD 12.2`: `70%` = `0.70`).
pub const PAN_ROOM_FRACTION: f32 = 0.70;

/// Share of available room traversed in zoom + pan combinations (`TDD 12.2`: `40%` = `0.40`).
pub const COMBO_PAN_ROOM_FRACTION: f32 = 0.40;

/// Camera window state in normalized coordinates (`TDD 12.2`).
///
/// - `cx`, `cy`: Center of the camera window in image coordinates (`0.0..=1.0`).
/// - `zoom`: Zoom scale factor (`>= 1.0`). `1.0` exactly fills the frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CamState {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
}

impl CamState {
    pub fn new(cx: f32, cy: f32, zoom: f32) -> Self {
        Self {
            cx: cx.clamp(0.0, 1.0),
            cy: cy.clamp(0.0, 1.0),
            zoom: zoom.max(1.0),
        }
    }

    /// Clamps the center `(cx, cy)` within `[0.0, 1.0]`.
    pub fn clamp_within_bounds(self) -> Self {
        Self {
            cx: self.cx.clamp(0.0, 1.0),
            cy: self.cy.clamp(0.0, 1.0),
            zoom: self.zoom.max(1.0),
        }
    }
}

/// Normalized focal point clicked or assigned for an image (`EFX-05`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FocalPoint {
    pub x: f32,
    pub y: f32,
}

impl Default for FocalPoint {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Motion presets supported by PACE (`EFX-01`, TDD 12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionPreset {
    Still,
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomInPanLeft,
    ZoomInPanRight,
    ZoomInPanUp,
    ZoomInPanDown,
    ZoomOutPanLeft,
    ZoomOutPanRight,
    ZoomOutPanUp,
    ZoomOutPanDown,
}

/// Easing curve applied over normalized scene progress `[0.0, 1.0]` (`EFX-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    /// Linear progress: `t = progress`.
    Steady,
    /// Smooth ease in and ease out: `t = 3t^2 - 2t^3`.
    #[default]
    EaseInOut,
}

impl Easing {
    pub fn apply(self, progress: f32) -> f32 {
        let p = progress.clamp(0.0, 1.0);
        match self {
            Easing::Steady => p,
            Easing::EaseInOut => p * p * (3.0 - 2.0 * p),
        }
    }
}

/// Motion intensity multiplier (`EFX-03`, TDD 12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Intensity {
    Subtle,
    #[default]
    Normal,
    Strong,
}

impl Intensity {
    pub fn multiplier(self) -> f32 {
        match self {
            Intensity::Subtle => 0.6,
            Intensity::Normal => 1.0,
            Intensity::Strong => 1.5,
        }
    }
}

/// Subtle camera shake configuration (`EFX-07`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shake {
    /// Amplitude of displacement in normalized coordinates (e.g. `0.003`).
    pub amplitude: f32,
    /// Shake frequency in Hertz (e.g. `2.5 Hz`).
    pub frequency_hz: f32,
    /// Initial phase offset in radians.
    pub phase: f32,
}

impl Default for Shake {
    fn default() -> Self {
        Self {
            amplitude: 0.003,
            frequency_hz: 2.5,
            phase: 0.0,
        }
    }
}

/// Complete camera motion plan for a scene (`TDD 12.2`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub preset: MotionPreset,
    pub from: CamState,
    pub to: CamState,
    pub easing: Easing,
    pub intensity: Intensity,
    pub shake: Option<Shake>,
}

fn fit_range_in_bounds(start: f32, end: f32) -> (f32, f32) {
    let travel = (end - start).abs();
    let forward = end >= start;
    let min_b = 0.0;
    let max_b = 1.0;
    let center = ((start + end) * 0.5).clamp(min_b + travel * 0.5, max_b - travel * 0.5);
    if forward {
        (center - travel * 0.5, center + travel * 0.5)
    } else {
        (center + travel * 0.5, center - travel * 0.5)
    }
}

/// Builds a `Motion` instance given a preset, focal point, duration, intensity, easing, and optional shake.
pub fn build_motion(
    preset: MotionPreset,
    focal: FocalPoint,
    duration_ms: u64,
    intensity: Intensity,
    easing: Easing,
    shake: Option<Shake>,
) -> Motion {
    let duration_sec = (duration_ms as f32) / 1000.0;
    let mult = intensity.multiplier();

    let (from, to) = match preset {
        MotionPreset::Still => {
            let state = CamState::new(focal.x, focal.y, STILL_ZOOM);
            (state, state)
        }
        MotionPreset::ZoomIn => {
            let target_delta = (BASE_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, 1.0);
            let to = CamState::new(focal.x, focal.y, 1.0 + delta);
            (from, to)
        }
        MotionPreset::ZoomOut => {
            let target_delta = (BASE_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, 1.0 + delta);
            let to = CamState::new(focal.x, focal.y, 1.0);
            (from, to)
        }
        MotionPreset::PanLeft
        | MotionPreset::PanRight
        | MotionPreset::PanUp
        | MotionPreset::PanDown => {
            let zoom = PAN_ZOOM;
            let room = 1.0 - (1.0 / zoom);
            let travel = room * PAN_ROOM_FRACTION * mult;

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::PanRight => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x - travel * 0.5,
                        focal.x + travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::PanLeft => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x + travel * 0.5,
                        focal.x - travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::PanDown => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y - travel * 0.5,
                        focal.y + travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                MotionPreset::PanUp => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y + travel * 0.5,
                        focal.y - travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                _ => unreachable!(),
            };

            let from = CamState::new(from_cx, from_cy, zoom);
            let to = CamState::new(to_cx, to_cy, zoom);
            (from, to)
        }
        MotionPreset::ZoomInPanLeft
        | MotionPreset::ZoomInPanRight
        | MotionPreset::ZoomInPanUp
        | MotionPreset::ZoomInPanDown => {
            let target_delta = (PAN_ZOOM - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let end_zoom = 1.0 + delta;

            let room = 1.0 - (1.0 / PAN_ZOOM);
            let travel = room * COMBO_PAN_ROOM_FRACTION * mult;

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::ZoomInPanRight => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x - travel * 0.5,
                        focal.x + travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::ZoomInPanLeft => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x + travel * 0.5,
                        focal.x - travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::ZoomInPanDown => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y - travel * 0.5,
                        focal.y + travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                MotionPreset::ZoomInPanUp => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y + travel * 0.5,
                        focal.y - travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                _ => unreachable!(),
            };

            let from = CamState::new(from_cx, from_cy, 1.0);
            let to = CamState::new(to_cx, to_cy, end_zoom);
            (from, to)
        }
        MotionPreset::ZoomOutPanLeft
        | MotionPreset::ZoomOutPanRight
        | MotionPreset::ZoomOutPanUp
        | MotionPreset::ZoomOutPanDown => {
            let target_delta = (PAN_ZOOM - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let start_zoom = 1.0 + delta;

            let room = 1.0 - (1.0 / PAN_ZOOM);
            let travel = room * COMBO_PAN_ROOM_FRACTION * mult;

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::ZoomOutPanRight => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x - travel * 0.5,
                        focal.x + travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::ZoomOutPanLeft => {
                    let (fcx, tcx) = fit_range_in_bounds(
                        focal.x + travel * 0.5,
                        focal.x - travel * 0.5,
                    );
                    (fcx, tcx, focal.y, focal.y)
                }
                MotionPreset::ZoomOutPanDown => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y - travel * 0.5,
                        focal.y + travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                MotionPreset::ZoomOutPanUp => {
                    let (fcy, tcy) = fit_range_in_bounds(
                        focal.y + travel * 0.5,
                        focal.y - travel * 0.5,
                    );
                    (focal.x, focal.x, fcy, tcy)
                }
                _ => unreachable!(),
            };

            let from = CamState::new(from_cx, from_cy, start_zoom);
            let to = CamState::new(to_cx, to_cy, 1.0);
            (from, to)
        }
    };

    Motion {
        preset,
        from,
        to,
        easing,
        intensity,
        shake,
    }
}

/// Calculates the camera state at a given progress `progress` in `[0.0, 1.0]` (`TDD 12.2`).
pub fn cam_state_at(motion: &Motion, progress: f32) -> CamState {
    let t = motion.easing.apply(progress);
    let base_cx = motion.from.cx + t * (motion.to.cx - motion.from.cx);
    let base_cy = motion.from.cy + t * (motion.to.cy - motion.from.cy);
    let zoom = motion.from.zoom + t * (motion.to.zoom - motion.from.zoom);

    let (shake_x, shake_y) = if let Some(shake) = motion.shake {
        let angle_x = 2.0 * PI * shake.frequency_hz * progress + shake.phase;
        let angle_y = 2.0 * PI * (shake.frequency_hz * 1.3) * progress + shake.phase + (PI / 4.0);
        (
            shake.amplitude * angle_x.sin(),
            shake.amplitude * angle_y.sin(),
        )
    } else {
        (0.0, 0.0)
    };

    CamState {
        cx: base_cx + shake_x,
        cy: base_cy + shake_y,
        zoom,
    }
    .clamp_within_bounds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_preset_has_slight_crop_and_no_movement() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let motion = build_motion(
            MotionPreset::Still,
            focal,
            5_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert_eq!(motion.from.zoom, STILL_ZOOM);
        assert_eq!(motion.to.zoom, STILL_ZOOM);
        assert_eq!(motion.from.cx, motion.to.cx);
        assert_eq!(motion.from.cy, motion.to.cy);

        let mid = cam_state_at(&motion, 0.5);
        assert_eq!(mid.zoom, STILL_ZOOM);
        assert_eq!(mid.cx, motion.from.cx);
        assert_eq!(mid.cy, motion.from.cy);
    }

    #[test]
    fn zoom_in_and_zoom_out_centred_on_focal_point() {
        let focal = FocalPoint { x: 0.7, y: 0.3 };
        // 20s scene (long enough that 0.012/s cap does not reduce the 0.18 delta)
        let zoom_in = build_motion(
            MotionPreset::ZoomIn,
            focal,
            20_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert_eq!(zoom_in.from.zoom, 1.0);
        assert!((zoom_in.to.zoom - BASE_ZOOM_TARGET).abs() < 1e-4);
        assert_eq!(zoom_in.from.cx, zoom_in.to.cx);
        assert_eq!(zoom_in.from.cy, zoom_in.to.cy);

        let zoom_out = build_motion(
            MotionPreset::ZoomOut,
            focal,
            20_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert!((zoom_out.from.zoom - BASE_ZOOM_TARGET).abs() < 1e-4);
        assert_eq!(zoom_out.to.zoom, 1.0);
        assert_eq!(zoom_out.from.cx, zoom_out.to.cx);
        assert_eq!(zoom_out.from.cy, zoom_out.to.cy);
    }

    #[test]
    fn pan_presets_move_across_70_percent_of_available_room() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let pan_right = build_motion(
            MotionPreset::PanRight,
            focal,
            10_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert_eq!(pan_right.from.zoom, PAN_ZOOM);
        assert_eq!(pan_right.to.zoom, PAN_ZOOM);
        // Moving right means cx increases (from.cx < to.cx)
        assert!(pan_right.from.cx < pan_right.to.cx);
        assert_eq!(pan_right.from.cy, pan_right.to.cy);

        let pan_left = build_motion(
            MotionPreset::PanLeft,
            focal,
            10_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        // Moving left means cx decreases (from.cx > to.cx)
        assert!(pan_left.from.cx > pan_left.to.cx);

        let room = 1.0 - (1.0 / PAN_ZOOM);
        let expected_travel = room * PAN_ROOM_FRACTION;
        let actual_travel = (pan_right.to.cx - pan_right.from.cx).abs();
        assert!((actual_travel - expected_travel).abs() < 1e-4);
    }

    #[test]
    fn zoom_plus_pan_combinations_move_across_40_percent() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let combo = build_motion(
            MotionPreset::ZoomInPanLeft,
            focal,
            20_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert_eq!(combo.from.zoom, 1.0);
        assert!((combo.to.zoom - PAN_ZOOM).abs() < 1e-4);
        assert!(combo.from.cx > combo.to.cx); // pan left

        let room = 1.0 - (1.0 / PAN_ZOOM);
        let expected_travel = room * COMBO_PAN_ROOM_FRACTION;
        let actual_travel = (combo.to.cx - combo.from.cx).abs();
        assert!((actual_travel - expected_travel).abs() < 1e-4);
    }

    #[test]
    fn intensity_scales_movement() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        // 30s scene so strong delta (0.18 * 1.5 = 0.27) is not capped by 0.012 * 30 = 0.36
        let normal = build_motion(
            MotionPreset::ZoomIn,
            focal,
            30_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let subtle = build_motion(
            MotionPreset::ZoomIn,
            focal,
            30_000,
            Intensity::Subtle,
            Easing::Steady,
            None,
        );
        let strong = build_motion(
            MotionPreset::ZoomIn,
            focal,
            30_000,
            Intensity::Strong,
            Easing::Steady,
            None,
        );

        let normal_delta = normal.to.zoom - normal.from.zoom;
        let subtle_delta = subtle.to.zoom - subtle.from.zoom;
        let strong_delta = strong.to.zoom - strong.from.zoom;

        assert!((subtle_delta - normal_delta * 0.6).abs() < 1e-4);
        assert!((strong_delta - normal_delta * 1.5).abs() < 1e-4);
    }

    #[test]
    fn zoom_speed_cap_limits_short_scenes_and_never_stops_long_scenes() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        // 5-second scene: 5s * 0.012/s = 0.06 max delta (instead of 0.18)
        let short_scene = build_motion(
            MotionPreset::ZoomIn,
            focal,
            5_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let short_delta = short_scene.to.zoom - short_scene.from.zoom;
        assert!((short_delta - 0.06).abs() < 1e-3);

        // 60-second scene: zoom speed is 0.18 / 60 = 0.003/s <= 0.012/s, and delta > 0 (never stops)
        let long_scene = build_motion(
            MotionPreset::ZoomIn,
            focal,
            60_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let long_delta = long_scene.to.zoom - long_scene.from.zoom;
        assert!((long_delta - 0.18).abs() < 1e-3);
    }

    #[test]
    fn cam_state_at_interpolates_and_clamps() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let motion = build_motion(
            MotionPreset::ZoomIn,
            focal,
            20_000,
            Intensity::Normal,
            Easing::EaseInOut,
            None,
        );

        let start = cam_state_at(&motion, 0.0);
        assert_eq!(start.zoom, motion.from.zoom);

        let end = cam_state_at(&motion, 1.0);
        assert_eq!(end.zoom, motion.to.zoom);

        let mid = cam_state_at(&motion, 0.5);
        // At progress 0.5, EaseInOut smoothstep 0.5^2 * (3 - 1) = 0.5
        let expected_mid_zoom = (motion.from.zoom + motion.to.zoom) * 0.5;
        assert!((mid.zoom - expected_mid_zoom).abs() < 1e-4);
    }

    #[test]
    fn shake_adds_subtle_smooth_wobble() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let shake = Shake {
            amplitude: 0.005,
            frequency_hz: 2.0,
            phase: 0.0,
        };
        let motion_with_shake = build_motion(
            MotionPreset::Still,
            focal,
            5_000,
            Intensity::Normal,
            Easing::Steady,
            Some(shake),
        );
        let motion_without_shake = build_motion(
            MotionPreset::Still,
            focal,
            5_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );

        let mid_shake = cam_state_at(&motion_with_shake, 0.25);
        let mid_no_shake = cam_state_at(&motion_without_shake, 0.25);

        // Shake causes deviation, but stays within amplitude
        let diff_x = (mid_shake.cx - mid_no_shake.cx).abs();
        let diff_y = (mid_shake.cy - mid_no_shake.cy).abs();
        assert!(diff_x > 0.0001 || diff_y > 0.0001);
        assert!(diff_x <= shake.amplitude + 1e-5);
        assert!(diff_y <= shake.amplitude + 1e-5);
    }
}
