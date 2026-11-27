use pace_engine::stickers::{FrameSticker, StickerKind};
use std::sync::OnceLock;

struct DecodedSticker {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static STICKER_SUBSCRIBE: OnceLock<DecodedSticker> = OnceLock::new();
static STICKER_LIKE: OnceLock<DecodedSticker> = OnceLock::new();
static STICKER_COMMENT: OnceLock<DecodedSticker> = OnceLock::new();
static STICKER_BELL: OnceLock<DecodedSticker> = OnceLock::new();
static STICKER_FOLLOW: OnceLock<DecodedSticker> = OnceLock::new();

fn decode_png(bytes: &[u8]) -> DecodedSticker {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("valid embedded sticker PNG")
        .to_rgba8();
    let (width, height) = img.dimensions();
    DecodedSticker {
        width,
        height,
        rgba: img.into_raw(),
    }
}

fn get_sticker(kind: StickerKind) -> &'static DecodedSticker {
    match kind {
        StickerKind::Subscribe => STICKER_SUBSCRIBE
            .get_or_init(|| decode_png(include_bytes!("../assets/stickers/subscribe.png"))),
        StickerKind::Like => {
            STICKER_LIKE.get_or_init(|| decode_png(include_bytes!("../assets/stickers/like.png")))
        }
        StickerKind::Comment => STICKER_COMMENT
            .get_or_init(|| decode_png(include_bytes!("../assets/stickers/comment.png"))),
        StickerKind::Bell => {
            STICKER_BELL.get_or_init(|| decode_png(include_bytes!("../assets/stickers/bell.png")))
        }
        StickerKind::Follow => STICKER_FOLLOW
            .get_or_init(|| decode_png(include_bytes!("../assets/stickers/follow.png"))),
    }
}

/// Alpha-composites active timeline stickers onto `out_rgba` (`frame_w x frame_h`).
pub fn composite_frame_stickers(
    out_rgba: &mut [u8],
    frame_w: u32,
    frame_h: u32,
    stickers: &[FrameSticker],
) {
    if stickers.is_empty() || frame_w == 0 || frame_h == 0 {
        return;
    }

    let fw = frame_w as f32;
    let fh = frame_h as f32;
    let base_unit = fw.min(fh) * 0.34;

    for sticker in stickers {
        if sticker.opacity <= 0.001 || sticker.scale <= 0.01 {
            continue;
        }
        let asset = get_sticker(sticker.kind);
        if asset.width == 0 || asset.height == 0 {
            continue;
        }

        let target_w = (base_unit * sticker.scale).max(4.0);
        let target_h = target_w * (asset.height as f32 / asset.width as f32);
        let cx = sticker.x * fw;
        let cy = sticker.y * fh;

        let cos_t = (-sticker.rotation_rad).cos();
        let sin_t = (-sticker.rotation_rad).sin();
        let half_diag = 0.5 * (target_w * target_w + target_h * target_h).sqrt();

        let min_x = ((cx - half_diag).floor() as i32).clamp(0, frame_w as i32 - 1);
        let max_x = ((cx + half_diag).ceil() as i32).clamp(0, frame_w as i32 - 1);
        let min_y = ((cy - half_diag).floor() as i32).clamp(0, frame_h as i32 - 1);
        let max_y = ((cy + half_diag).ceil() as i32).clamp(0, frame_h as i32 - 1);

        let aw = asset.width as f32;
        let ah = asset.height as f32;
        let max_ax = asset.width - 1;
        let max_ay = asset.height - 1;
        let global_alpha = sticker.opacity.clamp(0.0, 1.0);

        for py in min_y..=max_y {
            let dy = (py as f32 + 0.5) - cy;
            let row_off = (py as usize) * (frame_w as usize) * 4;
            for px in min_x..=max_x {
                let dx = (px as f32 + 0.5) - cx;
                let ux = dx * cos_t - dy * sin_t;
                let uy = dx * sin_t + dy * cos_t;

                let nx = (ux / target_w) + 0.5;
                let ny = (uy / target_h) + 0.5;
                if !(0.0..=1.0).contains(&nx) || !(0.0..=1.0).contains(&ny) {
                    continue;
                }

                let sx = (nx * (aw - 1.0)).clamp(0.0, max_ax as f32);
                let sy = (ny * (ah - 1.0)).clamp(0.0, max_ay as f32);

                let x0 = sx.floor() as u32;
                let y0 = sy.floor() as u32;
                let x1 = (x0 + 1).min(max_ax);
                let y1 = (y0 + 1).min(max_ay);
                let fx = sx - x0 as f32;
                let fy = sy - y0 as f32;

                let w00 = (1.0 - fx) * (1.0 - fy);
                let w10 = fx * (1.0 - fy);
                let w01 = (1.0 - fx) * fy;
                let w11 = fx * fy;

                let i00 = ((y0 * asset.width + x0) * 4) as usize;
                let i10 = ((y0 * asset.width + x1) * 4) as usize;
                let i01 = ((y1 * asset.width + x0) * 4) as usize;
                let i11 = ((y1 * asset.width + x1) * 4) as usize;

                let a00 = asset.rgba[i00 + 3] as f32 * (w00 / 255.0);
                let a10 = asset.rgba[i10 + 3] as f32 * (w10 / 255.0);
                let a01 = asset.rgba[i01 + 3] as f32 * (w01 / 255.0);
                let a11 = asset.rgba[i11 + 3] as f32 * (w11 / 255.0);
                let sa = (a00 + a10 + a01 + a11) * global_alpha;

                if sa <= 0.002 {
                    continue;
                }

                let pr = (asset.rgba[i00] as f32 * a00
                    + asset.rgba[i10] as f32 * a10
                    + asset.rgba[i01] as f32 * a01
                    + asset.rgba[i11] as f32 * a11)
                    * global_alpha;
                let pg = (asset.rgba[i00 + 1] as f32 * a00
                    + asset.rgba[i10 + 1] as f32 * a10
                    + asset.rgba[i01 + 1] as f32 * a01
                    + asset.rgba[i11 + 1] as f32 * a11)
                    * global_alpha;
                let pb = (asset.rgba[i00 + 2] as f32 * a00
                    + asset.rgba[i10 + 2] as f32 * a10
                    + asset.rgba[i01 + 2] as f32 * a01
                    + asset.rgba[i11 + 2] as f32 * a11)
                    * global_alpha;

                let inv_sa = 1.0 - sa;
                let d_off = row_off + (px as usize) * 4;
                out_rgba[d_off] = (pr + out_rgba[d_off] as f32 * inv_sa)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                out_rgba[d_off + 1] = (pg + out_rgba[d_off + 1] as f32 * inv_sa)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                out_rgba[d_off + 2] = (pb + out_rgba[d_off + 2] as f32 * inv_sa)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                out_rgba[d_off + 3] = 255;
            }
        }
    }
}
