//! Timeline-range overlay stickers (`Subscribe`, `Like`, `Comment`, `Bell`, `Follow`).
//!
//! Pure deterministic evaluation for both `pace-engine-wasm` browser preview
//! and `pace-render` native MP4 export.

use crate::timing::time_ms_to_frame;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

fn default_sticker_x() -> f32 {
    0.5
}

fn default_sticker_y() -> f32 {
    0.82
}

fn default_sticker_scale() -> f32 {
    1.0
}

/// Supported built-in transparent overlay sticker assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickerKind {
    Subscribe,
    Like,
    Comment,
    Bell,
    Follow,
}

impl StickerKind {
    pub fn code(self) -> f32 {
        match self {
            Self::Subscribe => 0.0,
            Self::Like => 1.0,
            Self::Comment => 2.0,
            Self::Bell => 3.0,
            Self::Follow => 4.0,
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Subscribe),
            1 => Some(Self::Like),
            2 => Some(Self::Comment),
            3 => Some(Self::Bell),
            4 => Some(Self::Follow),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subscribe => "subscribe",
            Self::Like => "like",
            Self::Comment => "comment",
            Self::Bell => "bell",
            Self::Follow => "follow",
        }
    }
}

/// Animation curve applied to an active overlay sticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StickerAnimation {
    None,
    #[default]
    PopIn,
    Bounce,
    Float,
}

/// Timeline-range sticker placement persisted in `projects.settings` JSONB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StickerPlacementSpec {
    #[serde(default)]
    pub id: String,
    pub kind: StickerKind,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default = "default_sticker_x")]
    pub x: f32,
    #[serde(default = "default_sticker_y")]
    pub y: f32,
    #[serde(default = "default_sticker_scale")]
    pub scale: f32,
    #[serde(default)]
    pub animation: StickerAnimation,
}

impl StickerPlacementSpec {
    /// Returns `[start_frame, end_frame)` at `fps`.
    pub fn frame_range(&self, fps: u32) -> (u32, u32) {
        let safe_fps = fps.max(1);
        let start = time_ms_to_frame(self.start_ms, safe_fps);
        let end = time_ms_to_frame(self.end_ms.max(self.start_ms), safe_fps);
        (start, end)
    }
}

/// Evaluated sticker draw instruction for a single frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameSticker {
    pub kind: StickerKind,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
    pub rotation_rad: f32,
}

/// Number of `f32` values per sticker in flat WASM encoding.
pub const FLAT_STICKER_STRIDE: usize = 6;

/// Evaluates all stickers active at `frame_index`.
pub fn evaluate_stickers_at(
    stickers: &[StickerPlacementSpec],
    frame_index: u32,
    fps: u32,
) -> Vec<FrameSticker> {
    if stickers.is_empty() {
        return Vec::new();
    }
    let safe_fps = fps.max(1) as f32;
    let mut out = Vec::new();

    for s in stickers {
        if s.end_ms <= s.start_ms {
            continue;
        }
        let (start_f, end_f) = s.frame_range(fps);
        if frame_index < start_f || frame_index >= end_f {
            continue;
        }

        let elapsed_sec = (frame_index - start_f) as f32 / safe_fps;
        let remaining_sec = (end_f - frame_index) as f32 / safe_fps;

        let base_scale = s.scale.clamp(0.3, 2.5);
        let (anim_scale, dy, rot, opacity) = match s.animation {
            StickerAnimation::None => (1.0, 0.0, 0.0, 1.0),
            StickerAnimation::PopIn => {
                let fade_in = (elapsed_sec / 0.10).clamp(0.0, 1.0);
                let fade_out = (remaining_sec / 0.14).clamp(0.0, 1.0);
                let t = (elapsed_sec / 0.28).clamp(0.0, 1.0);
                let pop = if t < 1.0 {
                    (1.0 - (1.0 - t).powi(3) * 0.65 + 0.18 * (t * PI).sin() * (1.0 - t)).max(0.2)
                } else {
                    1.0
                };
                (pop, 0.0, 0.0, fade_in.min(fade_out))
            }
            StickerAnimation::Bounce => {
                let fade_in = (elapsed_sec / 0.10).clamp(0.0, 1.0);
                let fade_out = (remaining_sec / 0.14).clamp(0.0, 1.0);
                let t = (elapsed_sec / 0.24).clamp(0.0, 1.0);
                let pop = if t < 1.0 {
                    (1.0 - (1.0 - t).powi(3) * 0.6 + 0.16 * (t * PI).sin() * (1.0 - t)).max(0.25)
                } else {
                    1.0
                };
                let bounce_y = -0.012 * (2.0 * PI * 2.2 * elapsed_sec).sin().abs();
                let tilt = 0.025 * (2.0 * PI * 1.1 * elapsed_sec).sin();
                (pop, bounce_y, tilt, fade_in.min(fade_out))
            }
            StickerAnimation::Float => {
                let fade_in = (elapsed_sec / 0.15).clamp(0.0, 1.0);
                let fade_out = (remaining_sec / 0.18).clamp(0.0, 1.0);
                let float_y = -0.010 * (2.0 * PI * 0.85 * elapsed_sec).sin();
                let tilt = 0.016 * (2.0 * PI * 0.65 * elapsed_sec).cos();
                (1.0, float_y, tilt, fade_in.min(fade_out))
            }
        };

        out.push(FrameSticker {
            kind: s.kind,
            x: s.x.clamp(0.05, 0.95),
            y: (s.y + dy).clamp(0.05, 0.95),
            scale: (base_scale * anim_scale).max(0.1),
            opacity: opacity.clamp(0.0, 1.0),
            rotation_rad: rot,
        });
    }

    out
}

/// Encodes evaluated stickers into `[sticker_count, ...stickers(stride=6)]`.
pub fn encode_stickers_flat_into(stickers: &[FrameSticker], out: &mut Vec<f32>) {
    out.clear();
    out.push(stickers.len() as f32);
    for s in stickers {
        out.extend_from_slice(&[
            s.kind.code(),
            s.x,
            s.y,
            s.scale,
            s.opacity,
            s.rotation_rad,
        ]);
    }
}

pub fn encode_stickers_flat(stickers: &[FrameSticker]) -> Vec<f32> {
    let mut out = Vec::with_capacity(1 + stickers.len() * FLAT_STICKER_STRIDE);
    encode_stickers_flat_into(stickers, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_range_and_animation_evaluate_deterministically() {
        let stickers = vec![StickerPlacementSpec {
            id: "stk-1".to_string(),
            kind: StickerKind::Subscribe,
            start_ms: 1_000,
            end_ms: 3_000,
            x: 0.5,
            y: 0.85,
            scale: 1.2,
            animation: StickerAnimation::Bounce,
        }];

        assert!(evaluate_stickers_at(&stickers, 29, 30).is_empty());
        assert!(evaluate_stickers_at(&stickers, 90, 30).is_empty());

        let active = evaluate_stickers_at(&stickers, 45, 30);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].kind, StickerKind::Subscribe);
        assert!((active[0].opacity - 1.0).abs() < 1e-5);
        assert!(active[0].scale > 1.0);

        let flat = encode_stickers_flat(&active);
        assert_eq!(flat.len(), 1 + FLAT_STICKER_STRIDE);
        assert_eq!(flat[0], 1.0);
        assert_eq!(flat[1], 0.0);
    }
}
