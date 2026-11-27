use crate::error::RenderError;
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};
use pace_engine::fit::{LayerKind, RectF};
use pace_engine::spec::{CompiledScene, FrameLayer, FramePlan, RenderSpec};
use std::collections::HashMap;
use std::path::Path;

pub struct DecodedSceneSource {
    pub image: Image<'static>,
    pub spec_width: f32,
    pub spec_height: f32,
    pub blurred_bg: Option<Vec<u8>>,
}

impl DecodedSceneSource {
    pub fn from_file(
        path: &Path,
        spec_width: f32,
        spec_height: f32,
    ) -> Result<Self, RenderError> {
        let reader = image::ImageReader::open(path)
            .map_err(|e| RenderError::ImageDecode(format!("open {}: {e}", path.display())))?
            .with_guessed_format()
            .map_err(|e| RenderError::ImageDecode(format!("guess format {}: {e}", path.display())))?;
        let decoded = reader
            .decode()
            .map_err(|e| RenderError::ImageDecode(format!("decode {}: {e}", path.display())))?;
        let rgba = decoded.to_rgba8();
        let (w, h) = rgba.dimensions();
        Self::from_rgba(w, h, rgba.into_raw(), spec_width, spec_height)
    }

    pub fn from_rgba(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        spec_width: f32,
        spec_height: f32,
    ) -> Result<Self, RenderError> {
        let img = Image::from_vec_u8(width, height, rgba, PixelType::U8x4)
            .map_err(|e| RenderError::ImageDecode(e.to_string()))?;
        Ok(Self {
            image: img,
            spec_width: spec_width.max(1.0),
            spec_height: spec_height.max(1.0),
            blurred_bg: None,
        })
    }
}

/// Reusable scratch buffers to avoid per-frame SIMD and heap allocations (`M8-3`).
pub struct FrameRasterScratch {
    resizer: Resizer,
    in_buf: Vec<u8>,
    patch_buf: Vec<u8>,
}

impl Default for FrameRasterScratch {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameRasterScratch {
    pub fn new() -> Self {
        Self {
            resizer: Resizer::new(),
            in_buf: Vec::new(),
            patch_buf: Vec::new(),
        }
    }
}

fn clamp_src_crop(x: f32, y: f32, w: f32, h: f32, img_w: f32, img_h: f32) -> RectF {
    let safe_w = w.clamp(1.0, img_w.max(1.0));
    let safe_h = h.clamp(1.0, img_h.max(1.0));
    let max_x = (img_w - safe_w).max(0.0);
    let max_y = (img_h - safe_h).max(0.0);
    RectF::new(x.clamp(0.0, max_x), y.clamp(0.0, max_y), safe_w, safe_h)
}

fn map_src_rect(src: RectF, actual_w: f32, actual_h: f32, spec_w: f32, spec_h: f32) -> RectF {
    let sx = actual_w / spec_w.max(1.0);
    let sy = actual_h / spec_h.max(1.0);
    clamp_src_crop(src.x * sx, src.y * sy, src.w * sx, src.h * sy, actual_w, actual_h)
}

fn box_blur_horizontal(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let window = (radius * 2 + 1) as u32;
    for y in 0..height {
        let row_off = y * width * 4;
        let mut r_sum: u32 = 0;
        let mut g_sum: u32 = 0;
        let mut b_sum: u32 = 0;

        for k in -(radius as isize)..=(radius as isize) {
            let x = k.clamp(0, (width as isize) - 1) as usize;
            let idx = row_off + x * 4;
            r_sum += src[idx] as u32;
            g_sum += src[idx + 1] as u32;
            b_sum += src[idx + 2] as u32;
        }

        for x in 0..width {
            let out_idx = row_off + x * 4;
            dst[out_idx] = (r_sum / window) as u8;
            dst[out_idx + 1] = (g_sum / window) as u8;
            dst[out_idx + 2] = (b_sum / window) as u8;
            dst[out_idx + 3] = 255;

            let remove_x = (x as isize - radius as isize).max(0) as usize;
            let add_x = (x + radius + 1).min(width - 1);
            let rem_idx = row_off + remove_x * 4;
            let add_idx = row_off + add_x * 4;
            r_sum = r_sum + src[add_idx] as u32 - src[rem_idx] as u32;
            g_sum = g_sum + src[add_idx + 1] as u32 - src[rem_idx + 1] as u32;
            b_sum = b_sum + src[add_idx + 2] as u32 - src[rem_idx + 2] as u32;
        }
    }
}

fn box_blur_vertical(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let window = (radius * 2 + 1) as u32;
    let stride = width * 4;
    for x in 0..width {
        let col_off = x * 4;
        let mut r_sum: u32 = 0;
        let mut g_sum: u32 = 0;
        let mut b_sum: u32 = 0;

        for k in -(radius as isize)..=(radius as isize) {
            let y = k.clamp(0, (height as isize) - 1) as usize;
            let idx = y * stride + col_off;
            r_sum += src[idx] as u32;
            g_sum += src[idx + 1] as u32;
            b_sum += src[idx + 2] as u32;
        }

        for y in 0..height {
            let out_idx = y * stride + col_off;
            dst[out_idx] = (r_sum / window) as u8;
            dst[out_idx + 1] = (g_sum / window) as u8;
            dst[out_idx + 2] = (b_sum / window) as u8;
            dst[out_idx + 3] = 255;

            let remove_y = (y as isize - radius as isize).max(0) as usize;
            let add_y = (y + radius + 1).min(height - 1);
            let rem_idx = remove_y * stride + col_off;
            let add_idx = add_y * stride + col_off;
            r_sum = r_sum + src[add_idx] as u32 - src[rem_idx] as u32;
            g_sum = g_sum + src[add_idx + 1] as u32 - src[rem_idx + 1] as u32;
            b_sum = b_sum + src[add_idx + 2] as u32 - src[rem_idx + 2] as u32;
        }
    }
}

fn ensure_blurred_background<'a>(
    source: &'a mut DecodedSceneSource,
    src_crop: RectF,
    frame_w: u32,
    frame_h: u32,
    resizer: &mut Resizer,
) -> Result<&'a [u8], RenderError> {
    let expected_len = (frame_w * frame_h * 4) as usize;
    if source
        .blurred_bg
        .as_ref()
        .is_some_and(|v| v.len() == expected_len)
    {
        return Ok(source.blurred_bg.as_deref().unwrap());
    }

    let bg_w = (frame_w / 4).max(32);
    let bg_h = (frame_h / 4).max(18);
    let mapped = map_src_rect(
        src_crop,
        source.image.width() as f32,
        source.image.height() as f32,
        source.spec_width,
        source.spec_height,
    );

    let mut small = Image::new(bg_w, bg_h, PixelType::U8x4);
    let opts = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(fast_image_resize::FilterType::Bilinear))
        .crop(
            mapped.x as f64,
            mapped.y as f64,
            mapped.w as f64,
            mapped.h as f64,
        );
    resizer
        .resize(&source.image, &mut small, &opts)
        .map_err(|e| RenderError::Resize(e.to_string()))?;

    let mut buf_a = small.into_vec();
    let mut buf_b = vec![0_u8; buf_a.len()];
    for _ in 0..3 {
        box_blur_horizontal(&buf_a, &mut buf_b, bg_w as usize, bg_h as usize, 6);
        box_blur_vertical(&buf_b, &mut buf_a, bg_w as usize, bg_h as usize, 6);
    }

    let small_ref = ImageRef::new(bg_w, bg_h, &buf_a, PixelType::U8x4)
        .map_err(|e| RenderError::Resize(e.to_string()))?;
    let mut full_bg = Image::new(frame_w, frame_h, PixelType::U8x4);
    let up_opts = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(fast_image_resize::FilterType::Bilinear));
    resizer
        .resize(&small_ref, &mut full_bg, &up_opts)
        .map_err(|e| RenderError::Resize(e.to_string()))?;

    source.blurred_bg = Some(full_bg.into_vec());
    Ok(source.blurred_bg.as_deref().unwrap())
}

fn render_rotated_image_layer(
    source_buf: &[u8],
    img_w: u32,
    img_h: u32,
    mapped_src: RectF,
    dst: RectF,
    rotation_rad: f32,
    clip: (i32, i32, i32, i32),
    width: u32,
    out_rgba: &mut [u8],
) {
    let (clip_x0, clip_y0, clip_x1, clip_y1) = clip;
    if clip_x1 <= clip_x0 || clip_y1 <= clip_y0 || img_w == 0 || img_h == 0 {
        return;
    }

    let cx = dst.x + dst.w * 0.5;
    let cy = dst.y + dst.h * 0.5;
    let cos_t = (-rotation_rad).cos();
    let sin_t = (-rotation_rad).sin();

    let max_ix = (img_w - 1) as f32;
    let max_iy = (img_h - 1) as f32;
    let stride = (width as usize) * 4;
    let src_stride = (img_w as usize) * 4;

    for py in clip_y0..clip_y1 {
        let dy = (py as f32 + 0.5) - cy;
        let row_off = (py as usize) * stride;
        for px in clip_x0..clip_x1 {
            let dx = (px as f32 + 0.5) - cx;
            let ux = dx * cos_t - dy * sin_t;
            let uy = dx * sin_t + dy * cos_t;

            let nx = (ux / dst.w) + 0.5;
            let ny = (uy / dst.h) + 0.5;
            if !(0.0..=1.0).contains(&nx) || !(0.0..=1.0).contains(&ny) {
                continue;
            }

            let sx = (mapped_src.x + nx * mapped_src.w - 0.5).clamp(0.0, max_ix);
            let sy = (mapped_src.y + ny * mapped_src.h - 0.5).clamp(0.0, max_iy);

            let x0 = sx.floor() as u32;
            let y0 = sy.floor() as u32;
            let x1 = (x0 + 1).min(img_w - 1);
            let y1 = (y0 + 1).min(img_h - 1);
            let fx = sx - x0 as f32;
            let fy = sy - y0 as f32;

            let w00 = (1.0 - fx) * (1.0 - fy);
            let w10 = fx * (1.0 - fy);
            let w01 = (1.0 - fx) * fy;
            let w11 = fx * fy;

            let i00 = (y0 as usize) * src_stride + (x0 as usize) * 4;
            let i10 = (y0 as usize) * src_stride + (x1 as usize) * 4;
            let i01 = (y1 as usize) * src_stride + (x0 as usize) * 4;
            let i11 = (y1 as usize) * src_stride + (x1 as usize) * 4;

            let d_off = row_off + (px as usize) * 4;
            out_rgba[d_off] = (source_buf[i00] as f32 * w00
                + source_buf[i10] as f32 * w10
                + source_buf[i01] as f32 * w01
                + source_buf[i11] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            out_rgba[d_off + 1] = (source_buf[i00 + 1] as f32 * w00
                + source_buf[i10 + 1] as f32 * w10
                + source_buf[i01 + 1] as f32 * w01
                + source_buf[i11 + 1] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            out_rgba[d_off + 2] = (source_buf[i00 + 2] as f32 * w00
                + source_buf[i10 + 2] as f32 * w10
                + source_buf[i01 + 2] as f32 * w01
                + source_buf[i11 + 2] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            out_rgba[d_off + 3] = 255;
        }
    }
}

fn render_scene_layers(
    scene_meta: &CompiledScene,
    layers: &[&FrameLayer],
    source: &mut DecodedSceneSource,
    width: u32,
    height: u32,
    out_rgba: &mut [u8],
    scratch: &mut FrameRasterScratch,
    clear_if_uncovered: bool,
) -> Result<(), RenderError> {
    // Clear frame to opaque black only if requested and layers don't completely cover the frame.
    let covers_canvas = layers.first().is_some_and(|l| {
        if l.rotation_rad.abs() > 1e-5 {
            return false;
        }
        let ix0 = (l.dst.x.round() as i32).clamp(0, width as i32);
        let iy0 = (l.dst.y.round() as i32).clamp(0, height as i32);
        let ix1 = (l.dst.right().round() as i32).clamp(0, width as i32);
        let iy1 = (l.dst.bottom().round() as i32).clamp(0, height as i32);
        ix0 == 0 && iy0 == 0 && (ix1 - ix0) as u32 == width && (iy1 - iy0) as u32 == height
    });

    if clear_if_uncovered && !covers_canvas {
        for px in out_rgba.chunks_exact_mut(4) {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 255;
        }
    }

    let _ = scene_meta;
    let img_w = source.image.width() as f32;
    let img_h = source.image.height() as f32;

    for layer in layers {
        match layer.kind {
            LayerKind::BlurredBackground => {
                let dst = layer.dst;
                let ix0 = (dst.x.round() as i32).clamp(0, width as i32);
                let iy0 = (dst.y.round() as i32).clamp(0, height as i32);
                let ix1 = (dst.right().round() as i32).clamp(0, width as i32);
                let iy1 = (dst.bottom().round() as i32).clamp(0, height as i32);
                let cw = (ix1 - ix0).max(0) as u32;
                let ch = (iy1 - iy0).max(0) as u32;
                if cw == 0 || ch == 0 {
                    continue;
                }
                let bg = ensure_blurred_background(
                    source,
                    layer.src,
                    width,
                    height,
                    &mut scratch.resizer,
                )?;
                if ix0 == 0 && iy0 == 0 && cw == width && ch == height {
                    out_rgba.copy_from_slice(bg);
                } else {
                    let stride = (width * 4) as usize;
                    let row_bytes = (cw * 4) as usize;
                    let sx0 = (ix0 - dst.x.round() as i32).clamp(0, (width - cw) as i32) as usize;
                    let sy0 = (iy0 - dst.y.round() as i32).clamp(0, (height - ch) as i32) as usize;
                    for row in 0..(ch as usize) {
                        let s_off = (sy0 + row) * stride + sx0 * 4;
                        let d_off = (iy0 as usize + row) * stride + (ix0 as usize) * 4;
                        out_rgba[d_off..d_off + row_bytes]
                            .copy_from_slice(&bg[s_off..s_off + row_bytes]);
                    }
                }
            }
            LayerKind::Image => {
                let mapped_src = map_src_rect(
                    layer.src,
                    img_w,
                    img_h,
                    source.spec_width,
                    source.spec_height,
                );
                let dst = layer.dst;
                if dst.w <= 0.0 || dst.h <= 0.0 {
                    continue;
                }

                if layer.rotation_rad.abs() > 1e-5 {
                    let bg_dst = layers
                        .iter()
                        .find(|l| l.kind == LayerKind::BlurredBackground)
                        .map(|l| l.dst);
                    let (vx0, vy0) = if let Some(bd) = bg_dst {
                        (bd.x, bd.y)
                    } else {
                        (
                            dst.x + dst.w * 0.5 - width as f32 * 0.5,
                            dst.y + dst.h * 0.5 - height as f32 * 0.5,
                        )
                    };
                    let clip_x0 = (vx0.round() as i32).clamp(0, width as i32);
                    let clip_y0 = (vy0.round() as i32).clamp(0, height as i32);
                    let clip_x1 = ((vx0 + width as f32).round() as i32).clamp(0, width as i32);
                    let clip_y1 = ((vy0 + height as f32).round() as i32).clamp(0, height as i32);
                    render_rotated_image_layer(
                        source.image.buffer(),
                        source.image.width(),
                        source.image.height(),
                        mapped_src,
                        dst,
                        layer.rotation_rad,
                        (clip_x0, clip_y0, clip_x1, clip_y1),
                        width,
                        out_rgba,
                    );
                    continue;
                }

                let ix0 = (dst.x.round() as i32).clamp(0, width as i32);
                let iy0 = (dst.y.round() as i32).clamp(0, height as i32);
                let ix1 = (dst.right().round() as i32).clamp(0, width as i32);
                let iy1 = (dst.bottom().round() as i32).clamp(0, height as i32);
                let cw = (ix1 - ix0).max(0) as u32;
                let ch = (iy1 - iy0).max(0) as u32;
                if cw == 0 || ch == 0 {
                    continue;
                }

                let clipped = clamp_src_crop(
                    mapped_src.x + ((ix0 as f32 - dst.x) / dst.w) * mapped_src.w,
                    mapped_src.y + ((iy0 as f32 - dst.y) / dst.h) * mapped_src.h,
                    (cw as f32 / dst.w) * mapped_src.w,
                    (ch as f32 / dst.h) * mapped_src.h,
                    img_w,
                    img_h,
                );

                let opts = ResizeOptions::new()
                    .resize_alg(ResizeAlg::Convolution(
                        fast_image_resize::FilterType::Bilinear,
                    ))
                    .crop(
                        clipped.x as f64,
                        clipped.y as f64,
                        clipped.w as f64,
                        clipped.h as f64,
                    );

                if ix0 == 0 && iy0 == 0 && cw == width && ch == height {
                    let mut dst_img = Image::from_slice_u8(width, height, out_rgba, PixelType::U8x4)
                        .map_err(|e| RenderError::Resize(e.to_string()))?;
                    scratch
                        .resizer
                        .resize(&source.image, &mut dst_img, &opts)
                        .map_err(|e| RenderError::Resize(e.to_string()))?;
                } else {
                    let needed_patch = (cw * ch * 4) as usize;
                    scratch.patch_buf.resize(needed_patch, 0);
                    {
                        let mut patch = Image::from_slice_u8(
                            cw,
                            ch,
                            &mut scratch.patch_buf,
                            PixelType::U8x4,
                        )
                        .map_err(|e| RenderError::Resize(e.to_string()))?;
                        scratch
                            .resizer
                            .resize(&source.image, &mut patch, &opts)
                            .map_err(|e| RenderError::Resize(e.to_string()))?;
                    }
                    let row_bytes = (cw * 4) as usize;
                    let dst_stride = (width * 4) as usize;
                    let x_off = (ix0 as usize) * 4;
                    for row in 0..(ch as usize) {
                        let s_start = row * row_bytes;
                        let d_start = (iy0 as usize + row) * dst_stride + x_off;
                        out_rgba[d_start..d_start + row_bytes]
                            .copy_from_slice(&scratch.patch_buf[s_start..s_start + row_bytes]);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Evaluates `frame_plan` with a fresh scratch context (`M8-3`).
pub fn rasterize_frame_plan(
    spec: &RenderSpec,
    plan: &FramePlan,
    sources: &mut HashMap<usize, DecodedSceneSource>,
    width: u32,
    height: u32,
    out_rgba: &mut [u8],
) -> Result<(), RenderError> {
    let mut scratch = FrameRasterScratch::new();
    rasterize_frame_plan_with_scratch(spec, plan, sources, width, height, out_rgba, &mut scratch)
}

/// Evaluates `frame_plan` onto `out_rgba` reusing SIMD resizer and scratch buffers (`M8-3`).
pub fn rasterize_frame_plan_with_scratch(
    spec: &RenderSpec,
    plan: &FramePlan,
    sources: &mut HashMap<usize, DecodedSceneSource>,
    width: u32,
    height: u32,
    out_rgba: &mut [u8],
    scratch: &mut FrameRasterScratch,
) -> Result<(), RenderError> {
    let expected = (width * height * 4) as usize;
    if out_rgba.len() != expected {
        return Err(RenderError::Resize(format!(
            "output buffer length mismatch: got {}, expected {expected}",
            out_rgba.len()
        )));
    }

    if plan.layers.is_empty() {
        for px in out_rgba.chunks_exact_mut(4) {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 255;
        }
        return Ok(());
    }

    // Partition layers into outgoing (first scene) and optional incoming (crossfade target).
    let first_scene_idx = plan.layers[0].scene_index;
    let mut first_layers = Vec::with_capacity(2);
    let mut second_layers = Vec::with_capacity(2);
    let mut second_scene_idx: Option<usize> = None;
    let mut in_opacity = 0.0_f32;

    for layer in &plan.layers {
        if layer.scene_index == first_scene_idx {
            first_layers.push(layer);
        } else {
            second_scene_idx = Some(layer.scene_index);
            if layer.kind == LayerKind::Image || in_opacity == 0.0 {
                in_opacity = layer.opacity;
            }
            second_layers.push(layer);
        }
    }

    if let Some(scene_out) = sources.get_mut(&first_scene_idx) {
        render_scene_layers(
            &spec.scenes[first_scene_idx],
            &first_layers,
            scene_out,
            width,
            height,
            out_rgba,
            scratch,
            true,
        )?;
    }

    if let Some(in_idx) = second_scene_idx {
        let is_spatial_transition = matches!(
            plan.active_transition.as_ref().map(|t| t.kind),
            Some(
                pace_engine::timing::TransitionKind::WhipPanLeft
                    | pace_engine::timing::TransitionKind::WhipPanRight
                    | pace_engine::timing::TransitionKind::SlideLeft
                    | pace_engine::timing::TransitionKind::SlideRight
                    | pace_engine::timing::TransitionKind::SlideUp
                    | pace_engine::timing::TransitionKind::SlideDown
            )
        );
        let weight_in = in_opacity.clamp(0.0, 1.0);
        if is_spatial_transition {
            if let Some(scene_in) = sources.get_mut(&in_idx) {
                render_scene_layers(
                    &spec.scenes[in_idx],
                    &second_layers,
                    scene_in,
                    width,
                    height,
                    out_rgba,
                    scratch,
                    false,
                )?;
            }
        } else if weight_in > 0.0 {
            if let Some(scene_in) = sources.get_mut(&in_idx) {
                let mut in_buf = std::mem::take(&mut scratch.in_buf);
                in_buf.resize(expected, 0);
                render_scene_layers(
                    &spec.scenes[in_idx],
                    &second_layers,
                    scene_in,
                    width,
                    height,
                    &mut in_buf,
                    scratch,
                    true,
                )?;
                let w_in = (weight_in * 256.0).round() as u32;
                let w_out = 256_u32.saturating_sub(w_in);
                for (dst, src) in out_rgba.chunks_exact_mut(4).zip(in_buf.chunks_exact(4)) {
                    dst[0] = ((dst[0] as u32 * w_out + src[0] as u32 * w_in + 128) >> 8) as u8;
                    dst[1] = ((dst[1] as u32 * w_out + src[1] as u32 * w_in + 128) >> 8) as u8;
                    dst[2] = ((dst[2] as u32 * w_out + src[2] as u32 * w_in + 128) >> 8) as u8;
                    dst[3] = 255;
                }
                scratch.in_buf = in_buf;
            }
        }
    }

    if plan.fade_black_alpha > 0.0 {
        let keep = ((1.0 - plan.fade_black_alpha.clamp(0.0, 1.0)) * 256.0).round() as u32;
        for px in out_rgba.chunks_exact_mut(4) {
            px[0] = ((px[0] as u32 * keep) >> 8) as u8;
            px[1] = ((px[1] as u32 * keep) >> 8) as u8;
            px[2] = ((px[2] as u32 * keep) >> 8) as u8;
            px[3] = 255;
        }
    }

    if plan.flash_white_alpha > 0.0 {
        let a = (plan.flash_white_alpha.clamp(0.0, 1.0) * 256.0).round() as u32;
        let inv = 256_u32.saturating_sub(a);
        let white_term = 255_u32 * a;
        for px in out_rgba.chunks_exact_mut(4) {
            px[0] = ((px[0] as u32 * inv + white_term) >> 8) as u8;
            px[1] = ((px[1] as u32 * inv + white_term) >> 8) as u8;
            px[2] = ((px[2] as u32 * inv + white_term) >> 8) as u8;
            px[3] = 255;
        }
    }

    Ok(())
}

/// Computes structural similarity index (SSIM, `0.0..=1.0`) across RGB luma between two RGBA frames (`M8-8`).
pub fn compute_rgba_ssim(a: &[u8], b: &[u8], width: u32, height: u32) -> f64 {
    let w = width as usize;
    let h = height as usize;
    if a.len() != w * h * 4 || b.len() != w * h * 4 || w < 8 || h < 8 {
        return 0.0;
    }

    let luma_at = |buf: &[u8], x: usize, y: usize| -> f64 {
        let i = (y * w + x) * 4;
        0.299 * (buf[i] as f64) + 0.587 * (buf[i + 1] as f64) + 0.114 * (buf[i + 2] as f64)
    };

    const C1: f64 = 6.5025; // (0.01 * 255)^2
    const C2: f64 = 58.5225; // (0.03 * 255)^2

    let mut ssim_sum = 0.0_f64;
    let mut blocks = 0_u64;

    for by in (0..=(h - 8)).step_by(8) {
        for bx in (0..=(w - 8)).step_by(8) {
            let mut mean_a = 0.0;
            let mut mean_b = 0.0;
            for dy in 0..8 {
                for dx in 0..8 {
                    mean_a += luma_at(a, bx + dx, by + dy);
                    mean_b += luma_at(b, bx + dx, by + dy);
                }
            }
            mean_a /= 64.0;
            mean_b /= 64.0;

            let mut var_a = 0.0;
            let mut var_b = 0.0;
            let mut cov = 0.0;
            for dy in 0..8 {
                for dx in 0..8 {
                    let da = luma_at(a, bx + dx, by + dy) - mean_a;
                    let db = luma_at(b, bx + dx, by + dy) - mean_b;
                    var_a += da * da;
                    var_b += db * db;
                    cov += da * db;
                }
            }
            var_a /= 63.0;
            var_b /= 63.0;
            cov /= 63.0;

            let num = (2.0 * mean_a * mean_b + C1) * (2.0 * cov + C2);
            let den = (mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2);
            ssim_sum += num / den;
            blocks += 1;
        }
    }

    if blocks == 0 {
        0.0
    } else {
        ssim_sum / (blocks as f64)
    }
}
