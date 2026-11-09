//! Fit modes and source crop geometry (`EFX-04`, `EFX-05`, TDD 12.4).
//!
//! Calculates source crop rectangles and destination layout for:
//! - `Fill`: Image is scaled to completely cover the destination frame. Uses the focal
//!   point and camera state while guaranteeing the source crop rectangle NEVER leaves
//!   the image bounds.
//! - `FitBlur`: A blurred copy fills the frame (static background), while the entire
//!   image is fitted inside without cropping and animated by the camera state.

use crate::motion::{CamState, FocalPoint};
use serde::{Deserialize, Serialize};

/// 2D float dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SizeF {
    pub w: f32,
    pub h: f32,
}

impl SizeF {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            w: w.max(1.0),
            h: h.max(1.0),
        }
    }

    pub fn aspect_ratio(self) -> f32 {
        self.w / self.h
    }
}

/// 2D float rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Fit mode for an image in a scene (`EFX-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    /// Crops image to completely fill the frame (`EFX-04`).
    #[default]
    Fill,
    /// Shows the full image on top of a blurred background (`EFX-04`).
    FitBlur,
}

/// Kind of layer in the render frame plan (`TDD 12.6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Image,
    BlurredBackground,
}

/// Calculated drawing instruction for a single layer (`TDD 12.6`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerPlan {
    pub kind: LayerKind,
    /// Which part of the source image to read, in image pixels (fractions allowed).
    pub src: RectF,
    /// Where on the destination frame to draw, in frame pixels.
    pub dst: RectF,
    /// Layer opacity (`0.0..=1.0`).
    pub opacity: f32,
}

/// Calculates the source crop rectangle for `Fill` mode (`EFX-04`, `EFX-05`).
///
/// Guarantees:
/// - Crop aspect ratio matches `frame_size.aspect_ratio()` exactly.
/// - `crop.x >= 0`, `crop.y >= 0`, `crop.right() <= image_size.w`, `crop.bottom() <= image_size.h`.
/// - Centered around `cam.cx` and `cam.cy` in image pixel coordinates, clamped within bounds.
pub fn calculate_fill_crop(
    image_size: SizeF,
    frame_size: SizeF,
    _focal: FocalPoint,
    cam: CamState,
) -> RectF {
    let frame_aspect = frame_size.aspect_ratio();
    let img_aspect = image_size.aspect_ratio();

    // Base crop size that matches frame_aspect without zoom
    let (base_w, base_h) = if img_aspect > frame_aspect {
        // Image is wider than frame -> height is full, width is cropped
        (image_size.h * frame_aspect, image_size.h)
    } else {
        // Image is taller than frame -> width is full, height is cropped
        (image_size.w, image_size.w / frame_aspect)
    };

    // Apply camera zoom (cam.zoom >= 1.0)
    let zoom = cam.zoom.max(1.0);
    let crop_w = (base_w / zoom).min(image_size.w);
    let crop_h = (base_h / zoom).min(image_size.h);

    // The desired center of the crop window is at cam.cx, cam.cy
    let center_x = cam.cx.clamp(0.0, 1.0) * image_size.w;
    let center_y = cam.cy.clamp(0.0, 1.0) * image_size.h;

    // Top-left
    let left = center_x - crop_w * 0.5;
    let top = center_y - crop_h * 0.5;

    // Clamp within bounds [0, image_size.w - crop_w] and [0, image_size.h - crop_h]
    let max_x = (image_size.w - crop_w).max(0.0);
    let max_y = (image_size.h - crop_h).max(0.0);

    let clamped_x = left.clamp(0.0, max_x);
    let clamped_y = top.clamp(0.0, max_y);

    RectF::new(clamped_x, clamped_y, crop_w, crop_h)
}

/// Calculates the layer plans for a scene given its fit mode, image size, frame size, focal point, and camera state (`TDD 12.4`, `12.6`).
pub fn calculate_fit_layers(
    mode: FitMode,
    image_size: SizeF,
    frame_size: SizeF,
    focal: FocalPoint,
    cam: CamState,
) -> Vec<LayerPlan> {
    match mode {
        FitMode::Fill => {
            let crop = calculate_fill_crop(image_size, frame_size, focal, cam);
            vec![LayerPlan {
                kind: LayerKind::Image,
                src: crop,
                dst: RectF::new(0.0, 0.0, frame_size.w, frame_size.h),
                opacity: 1.0,
            }]
        }
        FitMode::FitBlur => {
            // Layer 0: Blurred background fills the entire frame and stays still
            let bg_crop = calculate_fill_crop(
                image_size,
                frame_size,
                FocalPoint { x: 0.5, y: 0.5 },
                CamState::new(0.5, 0.5, 1.0),
            );
            let bg_layer = LayerPlan {
                kind: LayerKind::BlurredBackground,
                src: bg_crop,
                dst: RectF::new(0.0, 0.0, frame_size.w, frame_size.h),
                opacity: 1.0,
            };

            // Layer 1: Foreground image. Entire image fits inside the frame (letterboxed or pillarboxed).
            // It never crops the image: src = full image.
            let scale_fit = (frame_size.w / image_size.w).min(frame_size.h / image_size.h);
            let base_w = image_size.w * scale_fit;
            let base_h = image_size.h * scale_fit;

            let zoom = cam.zoom.max(1.0);
            let dst_w = base_w * zoom;
            let dst_h = base_h * zoom;

            let pan_offset_x = (cam.cx - 0.5) * frame_size.w;
            let pan_offset_y = (cam.cy - 0.5) * frame_size.h;

            let center_x = frame_size.w * 0.5 + pan_offset_x;
            let center_y = frame_size.h * 0.5 + pan_offset_y;

            let dst_x = center_x - dst_w * 0.5;
            let dst_y = center_y - dst_h * 0.5;

            let fg_layer = LayerPlan {
                kind: LayerKind::Image,
                src: RectF::new(0.0, 0.0, image_size.w, image_size.h),
                dst: RectF::new(dst_x, dst_y, dst_w, dst_h),
                opacity: 1.0,
            };

            vec![bg_layer, fg_layer]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn fill_crop_matches_frame_aspect_ratio_and_stays_within_bounds() {
        // Landscape image 4000x3000 (4:3) into 16:9 frame (1920x1080)
        let img = SizeF::new(4000.0, 3000.0);
        let frame = SizeF::new(1920.0, 1080.0);
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let cam = CamState::new(0.5, 0.5, 1.0);

        let crop = calculate_fill_crop(img, frame, focal, cam);

        assert!(crop.x >= 0.0);
        assert!(crop.y >= 0.0);
        assert!(crop.right() <= img.w + 1e-4);
        assert!(crop.bottom() <= img.h + 1e-4);

        let crop_aspect = crop.w / crop.h;
        let frame_aspect = frame.w / frame.h;
        assert!(
            (crop_aspect - frame_aspect).abs() < 1e-4,
            "Crop aspect ({crop_aspect}) must match frame aspect ({frame_aspect})"
        );
    }

    #[test]
    fn fill_crop_never_leaves_bounds_even_at_extreme_focal_points() {
        let img = SizeF::new(1920.0, 1080.0);
        let frame = SizeF::new(1080.0, 1920.0); // 9:16 portrait
        let extreme_focal = FocalPoint { x: 1.0, y: 1.0 };
        let cam = CamState::new(1.0, 1.0, 1.25);

        let crop = calculate_fill_crop(img, frame, extreme_focal, cam);

        assert!(crop.x >= 0.0, "crop.x ({}) < 0", crop.x);
        assert!(crop.y >= 0.0, "crop.y ({}) < 0", crop.y);
        assert!(
            crop.right() <= img.w + 1e-4,
            "crop.right ({}) > img.w ({})",
            crop.right(),
            img.w
        );
        assert!(
            crop.bottom() <= img.h + 1e-4,
            "crop.bottom ({}) > img.h ({})",
            crop.bottom(),
            img.h
        );
    }

    #[test]
    fn fit_blur_produces_two_layers_and_never_crops_foreground() {
        let img = SizeF::new(1080.0, 1920.0); // 9:16 portrait image
        let frame = SizeF::new(1920.0, 1080.0); // 16:9 landscape frame
        let focal = FocalPoint { x: 0.5, y: 0.5 };
        let cam = CamState::new(0.5, 0.5, 1.0);

        let layers = calculate_fit_layers(FitMode::FitBlur, img, frame, focal, cam);
        assert_eq!(layers.len(), 2);

        // Layer 0: Blurred background fills frame
        assert_eq!(layers[0].kind, LayerKind::BlurredBackground);
        assert_eq!(layers[0].dst, RectF::new(0.0, 0.0, frame.w, frame.h));

        // Layer 1: Foreground image is uncropped (src equals full image size)
        assert_eq!(layers[1].kind, LayerKind::Image);
        assert_eq!(layers[1].src, RectF::new(0.0, 0.0, img.w, img.h));
        // Foreground image fits inside frame (pillarboxed: height = 1080, width = 1080 * 9/16 = 607.5)
        assert_eq!(layers[1].dst.h, 1080.0);
        assert!((layers[1].dst.w - 607.5).abs() < 1e-2);
        // Centered horizontally
        let expected_x = (1920.0 - 607.5) / 2.0;
        assert!((layers[1].dst.x - expected_x).abs() < 1e-2);
    }

    proptest! {
        #[test]
        fn fill_crop_never_leaves_image_for_any_focal_point_and_zoom(
            img_w in 100.0f32..=4000.0f32,
            img_h in 100.0f32..=4000.0f32,
            frame_w in 360.0f32..=1920.0f32,
            frame_h in 360.0f32..=1920.0f32,
            focal_x in 0.0f32..=1.0f32,
            focal_y in 0.0f32..=1.0f32,
            cam_cx in 0.0f32..=1.0f32,
            cam_cy in 0.0f32..=1.0f32,
            zoom in 1.0f32..=5.0f32,
        ) {
            let img = SizeF::new(img_w, img_h);
            let frame = SizeF::new(frame_w, frame_h);
            let focal = FocalPoint { x: focal_x, y: focal_y };
            let cam = CamState::new(cam_cx, cam_cy, zoom);

            let crop = calculate_fill_crop(img, frame, focal, cam);

            prop_assert!(crop.x >= -1e-4, "crop.x ({}) < 0", crop.x);
            prop_assert!(crop.y >= -1e-4, "crop.y ({}) < 0", crop.y);
            prop_assert!(
                crop.right() <= img.w + 1e-3,
                "crop.right ({}) > img.w ({})",
                crop.right(),
                img.w
            );
            prop_assert!(
                crop.bottom() <= img.h + 1e-3,
                "crop.bottom ({}) > img.h ({})",
                crop.bottom(),
                img.h
            );
            prop_assert!(crop.w > 0.0);
            prop_assert!(crop.h > 0.0);

            let crop_aspect = crop.w / crop.h;
            let frame_aspect = frame.w / frame.h;
            prop_assert!(
                (crop_aspect - frame_aspect).abs() <= 1e-3,
                "Aspect ratio mismatch: crop={crop_aspect}, frame={frame_aspect}"
            );
        }
    }
}
