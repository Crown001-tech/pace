//! Camera motion model and presets (`EFX-01`, `EFX-03`, `EFX-07`, TDD 12.2, ADR-004).
//!
//! Models a camera window moving across an image from a start state to an end state.
//! Supports 20 presets (Hold, Classic & Punch Zooms, Pans, Combinations, Diagonal Ken Burns,
//! and Handheld Drift), 5 velocity easing curves, intensity-scaled crop headroom so pans
//! never freeze against image edges, and real-time multi-harmonic handheld camera shake.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Maximum zoom speed per second (`ADR-004`: `0.14` = `14%/sec` ceiling for ultra-short cuts).
pub const MAX_ZOOM_SPEED_PER_SEC: f32 = 0.14;

/// Maximum zoom speed per second for high-impact punch zooms (`ADR-004`: `0.24` = `24%/sec`).
pub const MAX_PUNCH_ZOOM_SPEED_PER_SEC: f32 = 0.24;

/// Default zoom for `Still` preset to eliminate edge artifacts (`TDD 12.2`: `1.04`).
pub const STILL_ZOOM: f32 = 1.04;

/// Base target zoom for classic `ZoomIn` / `ZoomOut` at `Intensity::Normal` (`ADR-004`: `1.24`).
pub const BASE_ZOOM_TARGET: f32 = 1.24;

/// Target zoom for `PunchZoomIn` / `PunchZoomOut` at `Intensity::Normal` (`ADR-004`: `1.34`).
pub const PUNCH_ZOOM_TARGET: f32 = 1.34;

/// Base pan zoom at `Intensity::Normal` (`ADR-004`: `1.18`, dynamically scaled by intensity).
pub const PAN_ZOOM: f32 = 1.18;

/// Share of available zoom margin traversed in pure pans (`ADR-004`: `80%` = `0.80`).
pub const PAN_ROOM_FRACTION: f32 = 0.80;

/// Starting zoom delta for combo zoom+pan presets so pan room exists at frame 0 (`ADR-004`).
pub const COMBO_START_ZOOM_DELTA: f32 = 0.09;

/// Ending zoom delta for combo zoom+pan presets at `Intensity::Normal` (`ADR-004`).
pub const COMBO_END_ZOOM_DELTA: f32 = 0.25;

/// Share of minimum-zoom margin traversed in zoom + pan combinations (`ADR-004`: `0.72`).
pub const COMBO_PAN_ROOM_FRACTION: f32 = 0.72;

fn default_motion_duration_ms() -> u64 {
    5_000
}

/// Camera window state in normalized coordinates (`TDD 12.2`).
///
/// - `cx`, `cy`: Center of the camera window in image coordinates (`0.0..=1.0`).
/// - `zoom`: Zoom scale factor (`>= 1.0`). `1.0` exactly fills the frame.
/// - `rotation_rad`: Roll angle in radians around the frame center (`0.0` = upright).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CamState {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
    #[serde(default)]
    pub rotation_rad: f32,
}

impl CamState {
    pub fn new(cx: f32, cy: f32, zoom: f32) -> Self {
        Self {
            cx: cx.clamp(0.0, 1.0),
            cy: cy.clamp(0.0, 1.0),
            zoom: zoom.max(1.0),
            rotation_rad: 0.0,
        }
    }

    pub fn with_rotation(cx: f32, cy: f32, zoom: f32, rotation_rad: f32) -> Self {
        Self {
            cx: cx.clamp(0.0, 1.0),
            cy: cy.clamp(0.0, 1.0),
            zoom: zoom.max(1.0),
            rotation_rad,
        }
    }

    /// Clamps the center `(cx, cy)` within `[0.0, 1.0]`.
    pub fn clamp_within_bounds(self) -> Self {
        Self {
            cx: self.cx.clamp(0.0, 1.0),
            cy: self.cy.clamp(0.0, 1.0),
            zoom: self.zoom.max(1.0),
            rotation_rad: self.rotation_rad,
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

/// Motion presets supported by PACE (`EFX-01`, `ADR-004`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionPreset {
    Still,
    ZoomIn,
    ZoomOut,
    PunchZoomIn,
    PunchZoomOut,
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
    KenBurnsDiagonalIn,
    KenBurnsDiagonalOut,
    HandheldDrift,
    ZoomSnap,
    Pendulum,
    Wobble,
    FallLeft,
    FallRight,
}

/// Optional per-scene visual effect layered on top of camera motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SceneVisualEffect {
    #[default]
    None,
    Handheld,
    CameraShake,
    ZoomLens,
}

/// Velocity easing curve applied over normalized scene progress `[0.0, 1.0]` (`EFX-03`, `ADR-004`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    /// Linear constant-speed tracking: `t = p`.
    Steady,
    /// Smooth cinematic ease-in and ease-out: `t = 3p^2 - 2p^3`.
    #[default]
    EaseInOut,
    /// CapCut-style fast burst at scene start followed by slow glide: `1 - (1 - p)^3.6`.
    PunchOut,
    /// Slow start ramping rapidly into the outgoing cut/transition: `p^3`.
    RampIn,
    /// High-contrast cubic S-whip curve accelerating briskly across mid-scene.
    DynamicCurve,
}

impl Easing {
    pub fn apply(self, progress: f32) -> f32 {
        let p = progress.clamp(0.0, 1.0);
        match self {
            Easing::Steady => p,
            Easing::EaseInOut => p * p * (3.0 - 2.0 * p),
            Easing::PunchOut => 1.0 - (1.0 - p).powf(3.6),
            Easing::RampIn => p * p * p,
            Easing::DynamicCurve => {
                if p < 0.5 {
                    4.0 * p * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(3) * 0.5
                }
            }
        }
    }
}

/// Motion intensity multiplier (`EFX-03`, `ADR-004`).
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
            Intensity::Strong => 1.55,
        }
    }
}

/// Organic multi-harmonic handheld camera shake configuration (`EFX-07`, `ADR-004`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shake {
    /// Maximum displacement amplitude in normalized coordinates (e.g. `0.009`).
    pub amplitude: f32,
    /// Primary handheld sway frequency in Hertz (`cycles / real second`, e.g. `1.35 Hz`).
    pub frequency_hz: f32,
    /// Initial phase offset in radians.
    pub phase: f32,
}

impl Default for Shake {
    fn default() -> Self {
        Self {
            amplitude: 0.009,
            frequency_hz: 1.35,
            phase: 0.0,
        }
    }
}

/// Complete camera motion plan for a scene (`TDD 12.2`, `ADR-004`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub preset: MotionPreset,
    pub from: CamState,
    pub to: CamState,
    pub easing: Easing,
    pub intensity: Intensity,
    pub shake: Option<Shake>,
    #[serde(default = "default_motion_duration_ms")]
    pub duration_ms: u64,
    #[serde(default)]
    pub visual_effect: SceneVisualEffect,
}

/// Fits a camera center travel interval `[start, end]` inside the safe crop-center window
/// `[0.5 / min_zoom, 1.0 - 0.5 / min_zoom]` so `calculate_fill_crop` never clamps/freezes against edges.
fn fit_range_for_zoom(start: f32, end: f32, min_zoom: f32) -> (f32, f32) {
    let half_win = (0.5 / min_zoom.max(1.0)).min(0.5);
    let min_b = half_win;
    let max_b = (1.0 - half_win).max(min_b);
    let max_span = (max_b - min_b).max(0.0);
    let travel = (end - start).abs().min(max_span);
    let forward = end >= start;
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
    build_motion_with_effect(
        preset,
        focal,
        duration_ms,
        intensity,
        easing,
        shake,
        SceneVisualEffect::None,
    )
}

/// Builds a `Motion` instance with an explicit `SceneVisualEffect`.
pub fn build_motion_with_effect(
    preset: MotionPreset,
    focal: FocalPoint,
    duration_ms: u64,
    intensity: Intensity,
    easing: Easing,
    shake: Option<Shake>,
    visual_effect: SceneVisualEffect,
) -> Motion {
    let safe_duration_ms = duration_ms.max(500);
    let duration_sec = (safe_duration_ms as f32) / 1000.0;
    let mult = intensity.multiplier();

    // Ensure safe center margin when shake is active on Still/Zoom presets
    let shake_pad = if shake.is_some()
        || preset == MotionPreset::HandheldDrift
        || visual_effect == SceneVisualEffect::Handheld
        || visual_effect == SceneVisualEffect::CameraShake
    {
        0.045
    } else {
        0.0
    };

    let mut effective_shake = shake;
    if effective_shake.is_none() && visual_effect == SceneVisualEffect::Handheld {
        effective_shake = Some(Shake {
            amplitude: 0.011 * mult,
            frequency_hz: 1.35,
            phase: 0.25,
        });
    }

    let (from, to) = match preset {
        MotionPreset::Still => {
            let z = (STILL_ZOOM + shake_pad).max(STILL_ZOOM);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, z);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, z);
            let state = CamState::new(cx, cy, z);
            (state, state)
        }
        MotionPreset::ZoomIn => {
            let base_from = 1.0 + shake_pad;
            let target_delta = (BASE_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, base_from);
            let to = CamState::new(focal.x, focal.y, base_from + delta);
            (from, to)
        }
        MotionPreset::ZoomOut => {
            let base_to = 1.0 + shake_pad;
            let target_delta = (BASE_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, base_to + delta);
            let to = CamState::new(focal.x, focal.y, base_to);
            (from, to)
        }
        MotionPreset::PunchZoomIn => {
            let base_from = 1.0 + shake_pad;
            let target_delta = (PUNCH_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_PUNCH_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, base_from);
            let to = CamState::new(focal.x, focal.y, base_from + delta);
            (from, to)
        }
        MotionPreset::PunchZoomOut => {
            let base_to = 1.0 + shake_pad;
            let target_delta = (PUNCH_ZOOM_TARGET - 1.0) * mult;
            let max_delta = MAX_PUNCH_ZOOM_SPEED_PER_SEC * duration_sec;
            let delta = target_delta.min(max_delta);
            let from = CamState::new(focal.x, focal.y, base_to + delta);
            let to = CamState::new(focal.x, focal.y, base_to);
            (from, to)
        }
        MotionPreset::PanLeft
        | MotionPreset::PanRight
        | MotionPreset::PanUp
        | MotionPreset::PanDown => {
            let zoom = 1.0 + (PAN_ZOOM - 1.0) * mult + shake_pad;
            let room = 1.0 - (1.0 / zoom);
            let travel = room * PAN_ROOM_FRACTION;

            let (cy_clamped, _) = fit_range_for_zoom(focal.y, focal.y, zoom);
            let (cx_clamped, _) = fit_range_for_zoom(focal.x, focal.x, zoom);

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::PanRight => {
                    let (fcx, tcx) =
                        fit_range_for_zoom(focal.x - travel * 0.5, focal.x + travel * 0.5, zoom);
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::PanLeft => {
                    let (fcx, tcx) =
                        fit_range_for_zoom(focal.x + travel * 0.5, focal.x - travel * 0.5, zoom);
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::PanDown => {
                    let (fcy, tcy) =
                        fit_range_for_zoom(focal.y - travel * 0.5, focal.y + travel * 0.5, zoom);
                    (cx_clamped, cx_clamped, fcy, tcy)
                }
                MotionPreset::PanUp => {
                    let (fcy, tcy) =
                        fit_range_for_zoom(focal.y + travel * 0.5, focal.y - travel * 0.5, zoom);
                    (cx_clamped, cx_clamped, fcy, tcy)
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
            let start_zoom = 1.0 + COMBO_START_ZOOM_DELTA * mult + shake_pad;
            let raw_delta = (COMBO_END_ZOOM_DELTA - COMBO_START_ZOOM_DELTA) * mult;
            let delta = raw_delta.min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let end_zoom = start_zoom + delta;

            let room = 1.0 - (1.0 / start_zoom);
            let travel = room * COMBO_PAN_ROOM_FRACTION;
            let (cy_clamped, _) = fit_range_for_zoom(focal.y, focal.y, start_zoom);
            let (cx_clamped, _) = fit_range_for_zoom(focal.x, focal.x, start_zoom);

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::ZoomInPanRight => {
                    let (fcx, tcx) = fit_range_for_zoom(
                        focal.x - travel * 0.5,
                        focal.x + travel * 0.5,
                        start_zoom,
                    );
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::ZoomInPanLeft => {
                    let (fcx, tcx) = fit_range_for_zoom(
                        focal.x + travel * 0.5,
                        focal.x - travel * 0.5,
                        start_zoom,
                    );
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::ZoomInPanDown => {
                    let (fcy, tcy) = fit_range_for_zoom(
                        focal.y - travel * 0.5,
                        focal.y + travel * 0.5,
                        start_zoom,
                    );
                    (cx_clamped, cx_clamped, fcy, tcy)
                }
                MotionPreset::ZoomInPanUp => {
                    let (fcy, tcy) = fit_range_for_zoom(
                        focal.y + travel * 0.5,
                        focal.y - travel * 0.5,
                        start_zoom,
                    );
                    (cx_clamped, cx_clamped, fcy, tcy)
                }
                _ => unreachable!(),
            };

            let from = CamState::new(from_cx, from_cy, start_zoom);
            let to = CamState::new(to_cx, to_cy, end_zoom);
            (from, to)
        }
        MotionPreset::ZoomOutPanLeft
        | MotionPreset::ZoomOutPanRight
        | MotionPreset::ZoomOutPanUp
        | MotionPreset::ZoomOutPanDown => {
            let end_zoom = 1.0 + COMBO_START_ZOOM_DELTA * mult + shake_pad;
            let raw_delta = (COMBO_END_ZOOM_DELTA - COMBO_START_ZOOM_DELTA) * mult;
            let delta = raw_delta.min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let start_zoom = end_zoom + delta;

            let room = 1.0 - (1.0 / end_zoom);
            let travel = room * COMBO_PAN_ROOM_FRACTION;
            let (cy_clamped, _) = fit_range_for_zoom(focal.y, focal.y, end_zoom);
            let (cx_clamped, _) = fit_range_for_zoom(focal.x, focal.x, end_zoom);

            let (from_cx, to_cx, from_cy, to_cy) = match preset {
                MotionPreset::ZoomOutPanRight => {
                    let (fcx, tcx) = fit_range_for_zoom(
                        focal.x - travel * 0.5,
                        focal.x + travel * 0.5,
                        end_zoom,
                    );
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::ZoomOutPanLeft => {
                    let (fcx, tcx) = fit_range_for_zoom(
                        focal.x + travel * 0.5,
                        focal.x - travel * 0.5,
                        end_zoom,
                    );
                    (fcx, tcx, cy_clamped, cy_clamped)
                }
                MotionPreset::ZoomOutPanDown => {
                    let (fcy, tcy) = fit_range_for_zoom(
                        focal.y - travel * 0.5,
                        focal.y + travel * 0.5,
                        end_zoom,
                    );
                    (cx_clamped, cx_clamped, fcy, tcy)
                }
                MotionPreset::ZoomOutPanUp => {
                    let (fcy, tcy) = fit_range_for_zoom(
                        focal.y + travel * 0.5,
                        focal.y - travel * 0.5,
                        end_zoom,
                    );
                    (cx_clamped, cx_clamped, fcy, tcy)
                }
                _ => unreachable!(),
            };

            let from = CamState::new(from_cx, from_cy, start_zoom);
            let to = CamState::new(to_cx, to_cy, end_zoom);
            (from, to)
        }
        MotionPreset::KenBurnsDiagonalIn | MotionPreset::KenBurnsDiagonalOut => {
            let min_zoom = 1.0 + 0.10 * mult + shake_pad;
            let raw_delta = 0.20 * mult;
            let delta = raw_delta.min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let max_zoom = min_zoom + delta;

            let room = 1.0 - (1.0 / min_zoom);
            let travel = (room * 0.75).max(0.065);
            let dir_x = if focal.x <= 0.5 { 1.0 } else { -1.0 };
            let dir_y = if focal.y <= 0.5 { 1.0 } else { -1.0 };

            let (fcx, tcx) = fit_range_for_zoom(
                focal.x - dir_x * travel * 0.5,
                focal.x + dir_x * travel * 0.5,
                min_zoom,
            );
            let (fcy, tcy) = fit_range_for_zoom(
                focal.y - dir_y * travel * 0.5,
                focal.y + dir_y * travel * 0.5,
                min_zoom,
            );

            if preset == MotionPreset::KenBurnsDiagonalIn {
                (
                    CamState::new(fcx, fcy, min_zoom),
                    CamState::new(tcx, tcy, max_zoom),
                )
            } else {
                (
                    CamState::new(tcx, tcy, max_zoom),
                    CamState::new(fcx, fcy, min_zoom),
                )
            }
        }
        MotionPreset::HandheldDrift => {
            if effective_shake.is_none() {
                effective_shake = Some(Shake {
                    amplitude: 0.012 * mult,
                    frequency_hz: 1.25,
                    phase: 0.4,
                });
            }
            let start_zoom = 1.0 + 0.11 * mult;
            let end_zoom = start_zoom + (0.06 * mult).min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, start_zoom);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, start_zoom);
            (
                CamState::new(cx, cy, start_zoom),
                CamState::new(cx, cy, end_zoom),
            )
        }
        MotionPreset::ZoomSnap => {
            let base_from = 1.0 + shake_pad;
            let target_delta = 0.30 * mult;
            let max_delta = (MAX_PUNCH_ZOOM_SPEED_PER_SEC * 1.4) * duration_sec;
            let delta = target_delta.min(max_delta);
            (
                CamState::new(focal.x, focal.y, base_from),
                CamState::new(focal.x, focal.y, base_from + delta),
            )
        }
        MotionPreset::Pendulum => {
            let z_from = 1.0 + 0.08 * mult + shake_pad;
            let z_to = z_from + (0.06 * mult).min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, z_from);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, z_from);
            (
                CamState::new(cx, cy, z_from),
                CamState::new(cx, cy, z_to),
            )
        }
        MotionPreset::Wobble => {
            let z_from = 1.0 + 0.10 * mult + shake_pad;
            let z_to = z_from + (0.04 * mult).min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, z_from);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, z_from);
            (
                CamState::new(cx, cy, z_from),
                CamState::new(cx, cy, z_to),
            )
        }
        MotionPreset::FallLeft => {
            let z_from = 1.0 + 0.06 * mult + shake_pad;
            let z_to = z_from + (0.14 * mult).min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, z_from);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, z_from);
            (
                CamState::with_rotation(cx, cy, z_from, 0.042 * mult),
                CamState::with_rotation(cx, cy, z_to, -0.078 * mult),
            )
        }
        MotionPreset::FallRight => {
            let z_from = 1.0 + 0.06 * mult + shake_pad;
            let z_to = z_from + (0.14 * mult).min(MAX_ZOOM_SPEED_PER_SEC * duration_sec);
            let (cx, _) = fit_range_for_zoom(focal.x, focal.x, z_from);
            let (cy, _) = fit_range_for_zoom(focal.y, focal.y, z_from);
            (
                CamState::with_rotation(cx, cy, z_from, -0.042 * mult),
                CamState::with_rotation(cx, cy, z_to, 0.078 * mult),
            )
        }
    };

    Motion {
        preset,
        from,
        to,
        easing,
        intensity,
        shake: effective_shake,
        duration_ms: safe_duration_ms,
        visual_effect,
    }
}

/// Calculates the camera state at normalized scene progress `progress` in `[0.0, 1.0]` (`TDD 12.2`, `ADR-004`).
pub fn cam_state_at(motion: &Motion, progress: f32) -> CamState {
    let p = progress.clamp(0.0, 1.0);
    let t = motion.easing.apply(p);
    let base_cx = motion.from.cx + t * (motion.to.cx - motion.from.cx);
    let base_cy = motion.from.cy + t * (motion.to.cy - motion.from.cy);
    let base_zoom = motion.from.zoom + t * (motion.to.zoom - motion.from.zoom);
    let base_rot = motion.from.rotation_rad + t * (motion.to.rotation_rad - motion.from.rotation_rad);

    let elapsed_sec = p * ((motion.duration_ms.max(500) as f32) / 1000.0);
    let mult = motion.intensity.multiplier();

    let preset_rot = match motion.preset {
        MotionPreset::Pendulum => 0.055 * mult * (2.0 * PI * 0.65 * elapsed_sec).sin(),
        MotionPreset::Wobble => {
            0.048 * mult * (2.0 * PI * 1.85 * elapsed_sec).sin() * (1.0 - 0.15 * p)
        }
        _ => 0.0,
    };

    let (shake_x, shake_y, zoom_breath) = if let Some(shake) = motion.shake {
        let w = 2.0 * PI * shake.frequency_hz * elapsed_sec;
        // Multi-harmonic organic handheld sway (bounded strictly within [-1.0, 1.0] * amplitude)
        let sway_x =
            0.68 * (w + shake.phase).sin() + 0.32 * (w * 2.31 + shake.phase * 1.7 + 0.65).sin();
        let sway_y = 0.65 * (w * 1.17 + shake.phase + PI / 3.0).sin()
            + 0.35 * (w * 2.63 + shake.phase * 1.3 + 1.4).sin();
        let breath = if motion.preset == MotionPreset::HandheldDrift {
            0.008 * (w * 0.55 + shake.phase).sin()
        } else {
            0.0
        };
        (shake.amplitude * sway_x, shake.amplitude * sway_y, breath)
    } else {
        (0.0, 0.0, 0.0)
    };

    let (fx_x, fx_y, fx_zoom, fx_rot) = match motion.visual_effect {
        SceneVisualEffect::None | SceneVisualEffect::Handheld => (0.0, 0.0, 0.0, 0.0),
        SceneVisualEffect::CameraShake => {
            let w = 2.0 * PI * 8.5 * elapsed_sec;
            let amp = 0.011 * mult;
            let jx = amp * (0.65 * w.sin() + 0.35 * (w * 1.61 + 0.9).sin());
            let jy = amp * (0.62 * (w * 1.19 + 1.3).sin() + 0.38 * (w * 1.73 + 2.1).sin());
            let micro_roll = 0.014 * mult * (2.0 * PI * 6.2 * elapsed_sec + 0.4).sin();
            (jx, jy, 0.0, micro_roll)
        }
        SceneVisualEffect::ZoomLens => {
            let pulse = 0.5 - 0.5 * (2.0 * PI * 1.35 * elapsed_sec).cos();
            (0.0, 0.0, 0.08 * mult * pulse, 0.0)
        }
    };

    CamState {
        cx: base_cx + shake_x + fx_x,
        cy: base_cy + shake_y + fx_y,
        zoom: (base_zoom + zoom_breath + fx_zoom).max(1.0),
        rotation_rad: base_rot + preset_rot + fx_rot,
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
    fn pan_presets_move_across_available_room() {
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
        assert!(pan_left.from.cx > pan_left.to.cx);

        let room = 1.0 - (1.0 / PAN_ZOOM);
        let expected_travel = room * PAN_ROOM_FRACTION;
        let actual_travel = (pan_right.to.cx - pan_right.from.cx).abs();
        assert!((actual_travel - expected_travel).abs() < 1e-4);
    }

    #[test]
    fn zoom_plus_pan_combinations_zoom_and_pan_from_frame_zero() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let combo = build_motion(
            MotionPreset::ZoomInPanLeft,
            focal,
            10_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        assert!(combo.from.zoom > 1.05);
        assert!(combo.to.zoom > combo.from.zoom + 0.12);
        assert!(combo.from.cx > combo.to.cx);
    }

    #[test]
    fn intensity_scales_movement() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let normal = build_motion(
            MotionPreset::ZoomIn,
            focal,
            5_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let subtle = build_motion(
            MotionPreset::ZoomIn,
            focal,
            5_000,
            Intensity::Subtle,
            Easing::Steady,
            None,
        );
        let strong = build_motion(
            MotionPreset::ZoomIn,
            focal,
            5_000,
            Intensity::Strong,
            Easing::Steady,
            None,
        );

        let normal_delta = normal.to.zoom - normal.from.zoom;
        let subtle_delta = subtle.to.zoom - subtle.from.zoom;
        let strong_delta = strong.to.zoom - strong.from.zoom;

        assert!((subtle_delta - normal_delta * 0.6).abs() < 1e-4);
        assert!((strong_delta - normal_delta * 1.55).abs() < 1e-4);
    }

    #[test]
    fn zoom_speed_cap_protects_subsecond_cuts_and_never_stops_long_scenes() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        // 0.5-second flash cut: 0.5s * 0.14/s = 0.07 max delta
        let flash_cut = build_motion(
            MotionPreset::ZoomIn,
            focal,
            500,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let flash_delta = flash_cut.to.zoom - flash_cut.from.zoom;
        assert!((flash_delta - 0.07).abs() < 1e-3);

        // 60-second scene: hits full BASE_ZOOM_TARGET - 1.0 = 0.24
        let long_scene = build_motion(
            MotionPreset::ZoomIn,
            focal,
            60_000,
            Intensity::Normal,
            Easing::Steady,
            None,
        );
        let long_delta = long_scene.to.zoom - long_scene.from.zoom;
        assert!((long_delta - 0.24).abs() < 1e-3);
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
        let expected_mid_zoom = (motion.from.zoom + motion.to.zoom) * 0.5;
        assert!((mid.zoom - expected_mid_zoom).abs() < 1e-4);
    }

    #[test]
    fn shake_adds_subtle_smooth_wobble() {
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let shake = Shake {
            amplitude: 0.009,
            frequency_hz: 1.5,
            phase: 0.2,
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

        let diff_x = (mid_shake.cx - mid_no_shake.cx).abs();
        let diff_y = (mid_shake.cy - mid_no_shake.cy).abs();
        assert!(diff_x > 0.0001 || diff_y > 0.0001);
        assert!(diff_x <= shake.amplitude + 1e-5);
        assert!(diff_y <= shake.amplitude + 1e-5);
    }
}
