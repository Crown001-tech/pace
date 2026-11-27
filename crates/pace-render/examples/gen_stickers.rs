use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::{imageops::FilterType, ImageBuffer, Rgba, RgbaImage};
use std::path::Path;

const W: u32 = 512;
const H: u32 = 160;
const SS: u32 = 4; // 4x supersampling for razor-sharp anti-aliasing

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}

fn rounded_rect_sdf(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let hx = (x1 - x0) * 0.5 - r;
    let hy = (y1 - y0) * 0.5 - r;
    let dx = (px - cx).abs() - hx;
    let dy = (py - cy).abs() - hy;
    let out_dist = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let in_dist = dx.max(dy).min(0.0);
    out_dist + in_dist - r
}

fn blend_pixel(dst: &mut Rgba<u8>, src: Rgba<u8>) {
    let sa = src[3] as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }
    for c in 0..3 {
        let sc = src[c] as f32 / 255.0;
        let dc = dst[c] as f32 / 255.0;
        let oc = (sc * sa + dc * da * (1.0 - sa)) / out_a;
        dst[c] = (oc * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn draw_text(
    img: &mut RgbaImage,
    font: &FontArc,
    text: &str,
    center_x: f32,
    center_y: f32,
    px_size: f32,
    color: [u8; 3],
) {
    let scale = PxScale::from(px_size);
    let scaled = font.as_scaled(scale);
    let mut glyphs = Vec::new();
    let mut caret_x = 0.0_f32;
    let mut prev = None;
    for ch in text.chars() {
        let gid = scaled.glyph_id(ch);
        if let Some(p) = prev {
            caret_x += scaled.kern(p, gid);
        }
        let g = gid.with_scale_and_position(scale, ab_glyph::point(caret_x, scaled.ascent()));
        caret_x += scaled.h_advance(gid);
        prev = Some(gid);
        glyphs.push(g);
    }
    let total_w = caret_x;
    let total_h = scaled.ascent() - scaled.descent();
    let offset_x = center_x - total_w * 0.5;
    let offset_y = center_y - total_h * 0.5;

    for mut g in glyphs {
        g.position.x += offset_x;
        g.position.y += offset_y;
        if let Some(outlined) = font.outline_glyph(g) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    let a = (cov * 255.0).round().clamp(0.0, 255.0) as u8;
                    if a > 0 {
                        let p = img.get_pixel_mut(px as u32, py as u32);
                        blend_pixel(p, Rgba([color[0], color[1], color[2], a]));
                    }
                }
            });
        }
    }
}

fn draw_icon_circle_bg(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, rgba: [u8; 4]) {
    let min_x = (cx - r - 2.0).max(0.0) as u32;
    let max_x = ((cx + r + 2.0) as u32).min(img.width() - 1);
    let min_y = (cy - r - 2.0).max(0.0) as u32;
    let max_y = ((cy + r + 2.0) as u32).min(img.height() - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() - r;
            if d < 1.0 {
                let cov = (1.0 - d).clamp(0.0, 1.0);
                let a = (rgba[3] as f32 * cov).round() as u8;
                blend_pixel(img.get_pixel_mut(x, y), Rgba([rgba[0], rgba[1], rgba[2], a]));
            }
        }
    }
}

fn draw_polygon(img: &mut RgbaImage, pts: &[(f32, f32)], color: [u8; 3]) {
    let min_x = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
    let max_x = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max).ceil().min((img.width() - 1) as f32) as u32;
    let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
    let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil().min((img.height() - 1) as f32) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut inside = false;
            let mut j = pts.len() - 1;
            for i in 0..pts.len() {
                let (xi, yi) = pts[i];
                let (xj, yj) = pts[j];
                if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi + 1e-9) + xi) {
                    inside = !inside;
                }
                j = i;
            }
            if inside {
                blend_pixel(img.get_pixel_mut(x, y), Rgba([color[0], color[1], color[2], 255]));
            }
        }
    }
}

fn render_sticker(
    font: &FontArc,
    kind: &str,
    label: &str,
    c1: [u8; 3],
    c2: [u8; 3],
) -> RgbaImage {
    let sw = W * SS;
    let sh = H * SS;
    let mut hi: RgbaImage = ImageBuffer::from_pixel(sw, sh, Rgba([0, 0, 0, 0]));

    let pad_x = 18.0 * SS as f32;
    let pad_y = 16.0 * SS as f32;
    let x0 = pad_x;
    let y0 = pad_y;
    let x1 = sw as f32 - pad_x;
    let y1 = sh as f32 - pad_y - 8.0 * SS as f32;
    let radius = (y1 - y0) * 0.5;

    for y in 0..sh {
        for x in 0..sw {
            let fx = x as f32;
            let fy = y as f32;

            // Soft drop shadow
            let sd_shadow = rounded_rect_sdf(fx, fy - 7.0 * SS as f32, x0, y0, x1, y1, radius);
            if sd_shadow < 14.0 * SS as f32 {
                let sh_alpha = (1.0 - (sd_shadow / (14.0 * SS as f32)).max(0.0)).powi(2) * 0.42;
                blend_pixel(hi.get_pixel_mut(x, y), Rgba([0, 0, 0, (sh_alpha * 255.0) as u8]));
            }

            // Main pill body
            let sd = rounded_rect_sdf(fx, fy, x0, y0, x1, y1, radius);
            if sd < 1.5 {
                let cov = (1.0 - sd.max(0.0) / 1.5).clamp(0.0, 1.0);
                let t = ((fx - x0) / (x1 - x0)).clamp(0.0, 1.0);
                let mut r = lerp_u8(c1[0], c2[0], t);
                let mut g = lerp_u8(c1[1], c2[1], t);
                let mut b = lerp_u8(c1[2], c2[2], t);

                // Crisp inner white border highlight
                if sd > -4.5 * SS as f32 {
                    r = lerp_u8(r, 255, 0.35);
                    g = lerp_u8(g, 255, 0.35);
                    b = lerp_u8(b, 255, 0.35);
                }

                blend_pixel(hi.get_pixel_mut(x, y), Rgba([r, g, b, (cov * 255.0) as u8]));
            }
        }
    }

    // Left icon badge
    let icon_cx = x0 + radius;
    let icon_cy = (y0 + y1) * 0.5;
    let icon_r = radius * 0.62;
    draw_icon_circle_bg(&mut hi, icon_cx, icon_cy, icon_r, [255, 255, 255, 45]);

    match kind {
        "subscribe" => {
            // Play triangle
            let s = icon_r * 0.52;
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - s * 0.55, icon_cy - s * 0.85),
                    (icon_cx + s * 0.95, icon_cy),
                    (icon_cx - s * 0.55, icon_cy + s * 0.85),
                ],
                [255, 255, 255],
            );
        }
        "like" => {
            // Crisp thumbs-up silhouette
            let s = icon_r * 0.55;
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - s * 0.75, icon_cy - s * 0.1),
                    (icon_cx - s * 0.35, icon_cy - s * 0.1),
                    (icon_cx - s * 0.35, icon_cy + s * 0.85),
                    (icon_cx - s * 0.75, icon_cy + s * 0.85),
                ],
                [255, 255, 255],
            );
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - s * 0.2, icon_cy - s * 0.15),
                    (icon_cx + s * 0.1, icon_cy - s * 0.95),
                    (icon_cx + s * 0.35, icon_cy - s * 0.9),
                    (icon_cx + s * 0.25, icon_cy - s * 0.2),
                    (icon_cx + s * 0.85, icon_cy - s * 0.2),
                    (icon_cx + s * 0.72, icon_cy + s * 0.85),
                    (icon_cx - s * 0.2, icon_cy + s * 0.85),
                ],
                [255, 255, 255],
            );
        }
        "comment" => {
            // Speech bubble
            let s = icon_r * 0.56;
            draw_icon_circle_bg(&mut hi, icon_cx, icon_cy - s * 0.12, s * 0.85, [255, 255, 255, 255]);
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - s * 0.45, icon_cy + s * 0.4),
                    (icon_cx - s * 0.75, icon_cy + s * 1.05),
                    (icon_cx + s * 0.05, icon_cy + s * 0.6),
                ],
                [255, 255, 255],
            );
        }
        "bell" => {
            // Notification bell
            let s = icon_r * 0.56;
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx, icon_cy - s * 0.95),
                    (icon_cx + s * 0.55, icon_cy - s * 0.55),
                    (icon_cx + s * 0.65, icon_cy + s * 0.35),
                    (icon_cx + s * 0.92, icon_cy + s * 0.62),
                    (icon_cx - s * 0.92, icon_cy + s * 0.62),
                    (icon_cx - s * 0.65, icon_cy + s * 0.35),
                    (icon_cx - s * 0.55, icon_cy - s * 0.55),
                ],
                [255, 255, 255],
            );
            draw_icon_circle_bg(&mut hi, icon_cx, icon_cy + s * 0.85, s * 0.26, [255, 255, 255, 255]);
        }
        "follow" => {
            // Bold plus icon
            let s = icon_r * 0.58;
            let t = s * 0.24;
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - t, icon_cy - s),
                    (icon_cx + t, icon_cy - s),
                    (icon_cx + t, icon_cy + s),
                    (icon_cx - t, icon_cy + s),
                ],
                [255, 255, 255],
            );
            draw_polygon(
                &mut hi,
                &[
                    (icon_cx - s, icon_cy - t),
                    (icon_cx + s, icon_cy - t),
                    (icon_cx + s, icon_cy + t),
                    (icon_cx - s, icon_cy + t),
                ],
                [255, 255, 255],
            );
        }
        _ => {}
    }

    // Label text centered in remaining pill area
    let text_cx = (icon_cx + icon_r + x1 - 24.0 * SS as f32) * 0.5;
    let text_cy = (y0 + y1) * 0.5;
    draw_text(&mut hi, font, label, text_cx, text_cy, 54.0 * SS as f32, [255, 255, 255]);

    image::imageops::resize(&hi, W, H, FilterType::Lanczos3)
}

fn main() {
    let font = FontArc::try_from_slice(include_bytes!("../../../assets/fonts/Ubuntu-B.ttf"))
        .expect("Ubuntu-B.ttf");

    let specs = [
        ("subscribe", "SUBSCRIBE", [239, 35, 60], [200, 12, 42]),
        ("like", "LIKE", [37, 99, 235], [29, 78, 216]),
        ("comment", "COMMENT", [16, 185, 129], [5, 150, 105]),
        ("bell", "BELL", [245, 158, 11], [217, 119, 6]),
        ("follow", "FOLLOW", [139, 92, 246], [236, 72, 153]),
    ];

    let out_dirs = [
        Path::new("crates/pace-render/assets/stickers"),
        Path::new("apps/web/public/stickers"),
    ];
    for dir in &out_dirs {
        std::fs::create_dir_all(dir).expect("create sticker dir");
    }

    for (name, label, c1, c2) in specs {
        let img = render_sticker(&font, name, label, c1, c2);
        for dir in &out_dirs {
            let p = dir.join(format!("{name}.png"));
            img.save(&p).expect("save sticker png");
            println!("Generated {}", p.display());
        }
    }
}
