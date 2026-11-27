use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use pace_engine::subtitles::{
    wrap_subtitle_lines, SubtitleCueSpec, SubtitleFont, SubtitlePosition, SubtitleStyleSpec,
};
use std::collections::HashMap;
use std::sync::OnceLock;

static SANS_FONT: OnceLock<FontArc> = OnceLock::new();
static MONO_FONT: OnceLock<FontArc> = OnceLock::new();

fn load_font(font_kind: SubtitleFont) -> &'static FontArc {
    match font_kind {
        SubtitleFont::Sans => SANS_FONT.get_or_init(|| {
            FontArc::try_from_slice(include_bytes!("../../../assets/fonts/Ubuntu-B.ttf"))
                .expect("valid embedded Ubuntu-B.ttf")
        }),
        SubtitleFont::Mono => MONO_FONT.get_or_init(|| {
            FontArc::try_from_slice(include_bytes!("../../../assets/fonts/UbuntuMono-R.ttf"))
                .expect("valid embedded UbuntuMono-R.ttf")
        }),
    }
}

/// Parses a `#RRGGBB` or `RRGGBB` hex string into `(r, g, b)` (`M9-4`).
pub fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() == 6 {
        let r = u8::from_str_radix(&trimmed[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&trimmed[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&trimmed[4..6], 16).unwrap_or(255);
        (r, g, b)
    } else {
        (255, 255, 255)
    }
}

/// Pre-rendered RGBA bounding-box overlay for one subtitle cue (`M9-4`).
#[derive(Debug, Clone)]
pub struct PreRenderedSubtitle {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Non-premultiplied RGBA8 buffer (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
}

/// Cache of pre-rendered subtitle overlays keyed by `cue_index` so each cue is rasterized at most once per segment (`M9-4`).
#[derive(Default)]
pub struct SubtitleOverlayCache {
    entries: HashMap<usize, Option<PreRenderedSubtitle>>,
}

impl SubtitleOverlayCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get_or_render(
        &mut self,
        cue: &SubtitleCueSpec,
        style: &SubtitleStyleSpec,
        aspect_ratio: &str,
        frame_width: u32,
        frame_height: u32,
    ) -> Option<&PreRenderedSubtitle> {
        let entry = self.entries.entry(cue.position).or_insert_with(|| {
            let lines = wrap_subtitle_lines(&cue.text, aspect_ratio);
            render_subtitle_overlay(&lines, style, frame_width, frame_height)
        });
        entry.as_ref()
    }
}

/// Rasterizes a 1–2 line subtitle overlay with backdrop box, glyph outline, and colored fill (`M9-4`).
pub fn render_subtitle_overlay(
    lines: &[String],
    style: &SubtitleStyleSpec,
    frame_width: u32,
    frame_height: u32,
) -> Option<PreRenderedSubtitle> {
    if !style.enabled || lines.is_empty() || frame_width < 32 || frame_height < 32 {
        return None;
    }

    let font = load_font(style.font_family);
    let size_pct = style.font_size_pct.clamp(70, 150) as f32 / 100.0;
    let base_px = (frame_height as f32 * 0.048 * size_pct).clamp(14.0, 96.0);
    let scale = PxScale::from(base_px);
    let scaled = font.as_scaled(scale);

    let ascent = scaled.ascent().ceil().max(1.0);
    let descent = scaled.descent().floor().min(-1.0);
    let line_gap = (base_px * 0.22).round().max(2.0);
    let line_height = (ascent - descent).ceil().max(base_px);

    // Measure each line's width.
    let mut line_widths: Vec<f32> = Vec::with_capacity(lines.len());
    let mut max_line_w = 0.0_f32;
    for line in lines.iter().take(2) {
        let mut w = 0.0_f32;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for ch in line.chars() {
            let gid = scaled.glyph_id(ch);
            if let Some(p) = prev {
                w += scaled.kern(p, gid);
            }
            w += scaled.h_advance(gid);
            prev = Some(gid);
        }
        line_widths.push(w);
        if w > max_line_w {
            max_line_w = w;
        }
    }

    if max_line_w <= 1.0 {
        return None;
    }

    let outline_r = (style.outline_width_px.min(6) as f32 * (frame_height as f32 / 720.0).max(0.65))
        .round()
        .clamp(0.0, 8.0) as i32;
    let pad_x = (base_px * 0.65).round().max(8.0) as u32 + (outline_r as u32);
    let pad_y = (base_px * 0.38).round().max(5.0) as u32 + (outline_r as u32);

    let active_lines = line_widths.len();
    let total_text_h =
        (active_lines as f32 * line_height) + ((active_lines.saturating_sub(1)) as f32 * line_gap);

    let box_w = ((max_line_w.ceil() as u32) + pad_x * 2).min(frame_width.saturating_sub(8));
    let box_h = ((total_text_h.ceil() as u32) + pad_y * 2).min(frame_height.saturating_sub(8));
    if box_w == 0 || box_h == 0 {
        return None;
    }

    let box_x = (frame_width.saturating_sub(box_w)) / 2;
    let safe_margin_y = ((frame_height as f32) * 0.08).round() as u32;
    let box_y = match style.position {
        SubtitlePosition::Bottom => frame_height
            .saturating_sub(safe_margin_y + box_h)
            .max(4),
        SubtitlePosition::Center => (frame_height.saturating_sub(box_h)) / 2,
        SubtitlePosition::Top => safe_margin_y.min(frame_height.saturating_sub(box_h)),
    };

    let bg_alpha = ((style.bg_opacity_pct.min(100) as u32 * 255) / 100) as u8;
    let mut rgba = vec![0_u8; (box_w * box_h * 4) as usize];

    // Fill backdrop box with rounded-corner clipping (4px radius) for broadcast readability.
    let corner_r = 4_i32;
    for y in 0..box_h {
        for x in 0..box_w {
            let dx = if (x as i32) < corner_r {
                corner_r - (x as i32)
            } else if (x as i32) >= (box_w as i32 - corner_r) {
                (x as i32) - (box_w as i32 - corner_r - 1)
            } else {
                0
            };
            let dy = if (y as i32) < corner_r {
                corner_r - (y as i32)
            } else if (y as i32) >= (box_h as i32 - corner_r) {
                (y as i32) - (box_h as i32 - corner_r - 1)
            } else {
                0
            };
            if dx * dx + dy * dy <= corner_r * corner_r {
                let idx = ((y * box_w + x) * 4) as usize;
                rgba[idx] = 10;
                rgba[idx + 1] = 10;
                rgba[idx + 2] = 14;
                rgba[idx + 3] = bg_alpha;
            }
        }
    }

    // Build floating-point coverage masks for glyph fill and dilated outline.
    let pixel_count = (box_w * box_h) as usize;
    let mut fill_mask = vec![0.0_f32; pixel_count];

    for (line_idx, line) in lines.iter().take(2).enumerate() {
        let lw = line_widths[line_idx];
        let start_x = ((box_w as f32 - lw) * 0.5).max(pad_x as f32 * 0.5);
        let baseline_y =
            (pad_y as f32) + ascent + (line_idx as f32) * (line_height + line_gap);

        let mut caret_x = start_x;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for ch in line.chars() {
            let gid = scaled.glyph_id(ch);
            if let Some(p) = prev {
                caret_x += scaled.kern(p, gid);
            }
            let glyph = gid.with_scale_and_position(scale, ab_glyph::point(caret_x, baseline_y));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let gx0 = bounds.min.x.floor() as i32;
                let gy0 = bounds.min.y.floor() as i32;
                outlined.draw(|gx, gy, cov| {
                    let px = gx0 + (gx as i32);
                    let py = gy0 + (gy as i32);
                    if px >= 0 && px < (box_w as i32) && py >= 0 && py < (box_h as i32) {
                        let m_idx = (py as usize) * (box_w as usize) + (px as usize);
                        if cov > fill_mask[m_idx] {
                            fill_mask[m_idx] = cov;
                        }
                    }
                });
            }
            caret_x += scaled.h_advance(gid);
            prev = Some(gid);
        }
    }

    // Dilate `fill_mask` by `outline_r` to create a crisp dark outline around every glyph.
    let mut outline_mask = vec![0.0_f32; pixel_count];
    if outline_r > 0 {
        let r_sq = outline_r * outline_r;
        for y in 0..(box_h as i32) {
            for x in 0..(box_w as i32) {
                let c = fill_mask[(y as usize) * (box_w as usize) + (x as usize)];
                if c <= 0.01 {
                    continue;
                }
                for dy in -outline_r..=outline_r {
                    let ny = y + dy;
                    if ny < 0 || ny >= (box_h as i32) {
                        continue;
                    }
                    for dx in -outline_r..=outline_r {
                        if dx * dx + dy * dy > r_sq {
                            continue;
                        }
                        let nx = x + dx;
                        if nx < 0 || nx >= (box_w as i32) {
                            continue;
                        }
                        let n_idx = (ny as usize) * (box_w as usize) + (nx as usize);
                        if c > outline_mask[n_idx] {
                            outline_mask[n_idx] = c;
                        }
                    }
                }
            }
        }
    }

    let (tr, tg, tb) = parse_hex_rgb(&style.text_color);

    // Composite outline (opaque black `#000000`) then text fill (`tr, tg, tb`) over the backdrop box.
    for i in 0..pixel_count {
        let out_cov = outline_mask[i].clamp(0.0, 1.0);
        let fill_cov = fill_mask[i].clamp(0.0, 1.0);
        if out_cov <= 0.0 && fill_cov <= 0.0 {
            continue;
        }
        let idx = i * 4;
        let mut r = rgba[idx] as f32;
        let mut g = rgba[idx + 1] as f32;
        let mut b = rgba[idx + 2] as f32;
        let mut a = (rgba[idx + 3] as f32) / 255.0;

        if out_cov > 0.0 {
            let out_a = out_cov + a * (1.0 - out_cov);
            if out_a > 0.0 {
                r = (0.0 * out_cov + r * a * (1.0 - out_cov)) / out_a;
                g = (0.0 * out_cov + g * a * (1.0 - out_cov)) / out_a;
                b = (0.0 * out_cov + b * a * (1.0 - out_cov)) / out_a;
            }
            a = out_a;
        }

        if fill_cov > 0.0 {
            let out_a = fill_cov + a * (1.0 - fill_cov);
            if out_a > 0.0 {
                r = ((tr as f32) * fill_cov + r * a * (1.0 - fill_cov)) / out_a;
                g = ((tg as f32) * fill_cov + g * a * (1.0 - fill_cov)) / out_a;
                b = ((tb as f32) * fill_cov + b * a * (1.0 - fill_cov)) / out_a;
            }
            a = out_a;
        }

        rgba[idx] = r.round().clamp(0.0, 255.0) as u8;
        rgba[idx + 1] = g.round().clamp(0.0, 255.0) as u8;
        rgba[idx + 2] = b.round().clamp(0.0, 255.0) as u8;
        rgba[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    Some(PreRenderedSubtitle {
        x: box_x,
        y: box_y,
        width: box_w,
        height: box_h,
        rgba,
    })
}

/// Alpha-blends a pre-rendered subtitle overlay onto an opaque RGBA video frame (`M9-4`).
pub fn composite_subtitle_overlay(
    frame_rgba: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    overlay: &PreRenderedSubtitle,
) {
    let fw = frame_width as usize;
    let fh = frame_height as usize;
    if frame_rgba.len() != fw * fh * 4 {
        return;
    }

    let ox = overlay.x as usize;
    let oy = overlay.y as usize;
    let ow = overlay.width as usize;
    let oh = overlay.height as usize;

    for sy in 0..oh {
        let dy = oy + sy;
        if dy >= fh {
            break;
        }
        let src_row = sy * ow * 4;
        let dst_row = dy * fw * 4;
        for sx in 0..ow {
            let dx = ox + sx;
            if dx >= fw {
                break;
            }
            let s_idx = src_row + sx * 4;
            let sa = overlay.rgba[s_idx + 3] as u32;
            if sa == 0 {
                continue;
            }
            let d_idx = dst_row + dx * 4;
            let inv_a = 255_u32.saturating_sub(sa);
            frame_rgba[d_idx] =
                ((overlay.rgba[s_idx] as u32 * sa + frame_rgba[d_idx] as u32 * inv_a + 127) / 255)
                    as u8;
            frame_rgba[d_idx + 1] = ((overlay.rgba[s_idx + 1] as u32 * sa
                + frame_rgba[d_idx + 1] as u32 * inv_a
                + 127)
                / 255) as u8;
            frame_rgba[d_idx + 2] = ((overlay.rgba[s_idx + 2] as u32 * sa
                + frame_rgba[d_idx + 2] as u32 * inv_a
                + 127)
                / 255) as u8;
            frame_rgba[d_idx + 3] = 255;
        }
    }
}
