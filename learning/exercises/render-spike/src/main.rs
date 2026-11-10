//! Spike S1: Render Speed Benchmark (`ADR-003`, TDD 13.2, Milestones line 181).
//!
//! Loads a high-resolution 4K test image, applies PACE camera motion (`motion.rs`)
//! and fill-crop geometry (`fit.rs`) across 1,800 frames (60 seconds at 30 fps),
//! crops and resizes each frame using SIMD via `fast_image_resize`, and pipes
//! raw RGBA bytes directly into FFmpeg stdin for libx264 encoding.
//!
//! Measures FPS and export time for both 720p and 1080p.

use fast_image_resize::images::Image;
use fast_image_resize::{PixelType, ResizeOptions, Resizer};
use pace_engine::fit::{calculate_fill_crop, SizeF};
use pace_engine::motion::{
    build_motion, cam_state_at, Easing, FocalPoint, Intensity, MotionPreset,
};
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

#[allow(dead_code)]
struct BenchmarkResult {
    name: &'static str,
    width: u32,
    height: u32,
    frames: u32,
    duration_secs: f64,
    fps: f64,
    realtime_factor: f64,
    est_22min_secs: f64,
    output_path: PathBuf,
}

fn run_benchmark(
    name: &'static str,
    width: u32,
    height: u32,
    total_frames: u32,
    src_image: &Image,
    src_size: SizeF,
    output_dir: &Path,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    println!("\n========================================================");
    println!("  Starting Benchmark: {} ({}x{})", name, width, height);
    println!(
        "  Total frames: {} ({}s at 30 fps)",
        total_frames,
        total_frames / 30
    );
    println!("========================================================");

    let output_path = output_dir.join(format!("output_{}.mp4", name));
    let frame_size = SizeF::new(width as f32, height as f32);
    let focal = FocalPoint { x: 0.5, y: 0.5 };

    // 60-second slow zoom in preset
    let motion = build_motion(
        MotionPreset::ZoomIn,
        focal,
        (total_frames as u64 * 1000) / 30,
        Intensity::Normal,
        Easing::EaseInOut,
        None,
    );

    // Fast SIMD resizer
    let mut resizer = Resizer::new();
    let mut dst_image = Image::new(width, height, PixelType::U8x4);

    // Spawn FFmpeg with exact PACE encoder settings (TDD 13.2 / 13.3)
    let crf = if height <= 720 { "23" } else { "22" };
    let maxrate = if height <= 720 { "2.5M" } else { "4.5M" };
    let bufsize = if height <= 720 { "5M" } else { "9M" };

    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", width, height),
            "-r",
            "30",
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-profile:v",
            "high",
            "-pix_fmt",
            "yuv420p",
            "-r",
            "30",
            "-crf",
            crf,
            "-maxrate",
            maxrate,
            "-bufsize",
            bufsize,
            "-g",
            "60",
            "-keyint_min",
            "30",
            "-sc_threshold",
            "0",
            "-bf",
            "2",
            "-x264-params",
            "open-gop=0",
            "-video_track_timescale",
            "15360",
            output_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = ffmpeg.stdin.take().expect("Failed to open FFmpeg stdin");

    let start_time = Instant::now();
    let mut last_log = Instant::now();

    for frame_idx in 0..total_frames {
        let progress = if total_frames > 1 {
            frame_idx as f32 / (total_frames - 1) as f32
        } else {
            0.0
        };

        // 1. Calculate camera state and source crop
        let cam = cam_state_at(&motion, progress);
        let crop = calculate_fill_crop(src_size, frame_size, focal, cam);

        // 2. Crop & resize with SIMD
        let options =
            ResizeOptions::new().crop(crop.x as f64, crop.y as f64, crop.w as f64, crop.h as f64);
        resizer.resize(src_image, &mut dst_image, &options)?;

        // 3. Pipe raw RGBA frame to FFmpeg
        stdin.write_all(dst_image.buffer())?;

        // Progress update every 300 frames (~10s of video)
        if (frame_idx + 1) % 300 == 0 || frame_idx + 1 == total_frames {
            let elapsed = last_log.elapsed().as_secs_f64();
            let step_fps = 300.0 / elapsed;
            println!(
                "  -> Rendered frame {} / {} ({:.1}%) - current speed: {:.1} fps",
                frame_idx + 1,
                total_frames,
                ((frame_idx + 1) as f32 / total_frames as f32) * 100.0,
                step_fps
            );
            last_log = Instant::now();
        }
    }

    drop(stdin); // Signal EOF to FFmpeg
    let status = ffmpeg.wait_with_output()?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!(
            "FFmpeg failed with exit code {:?}: {}",
            status.status.code(),
            stderr
        )
        .into());
    }

    let total_secs = start_time.elapsed().as_secs_f64();
    let avg_fps = (total_frames as f64) / total_secs;
    let video_duration = (total_frames as f64) / 30.0;
    let realtime_factor = video_duration / total_secs;
    let est_22min_frames = 22.0 * 60.0 * 30.0;
    let est_22min_secs = est_22min_frames / avg_fps;

    println!("\n  Results for {}:", name);
    println!("    Total time:       {:.2} seconds", total_secs);
    println!("    Average speed:    {:.1} FPS", avg_fps);
    println!(
        "    Real-time factor: {:.2}x ({:.2}x real-time)",
        realtime_factor, realtime_factor
    );
    println!(
        "    Est. 22-min video:{:.1} minutes ({:.1}s)",
        est_22min_secs / 60.0,
        est_22min_secs
    );

    Ok(BenchmarkResult {
        name,
        width,
        height,
        frames: total_frames,
        duration_secs: total_secs,
        fps: avg_fps,
        realtime_factor,
        est_22min_secs,
        output_path,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================================");
    println!("  PACE Spike S1: Render Speed Benchmark");
    println!("  Testing CPU SIMD Frame Drawing + FFmpeg Encoding");
    println!("========================================================");

    // Locate project root & image fixture
    let image_path = PathBuf::from("fixtures/images/sample_card_4k.png");
    if !image_path.exists() {
        return Err(format!(
            "Fixture image not found at {:?}. Please ensure sample_card_4k.png exists.",
            image_path
        )
        .into());
    }

    println!("Loading 4K source image: {:?}", image_path);
    let load_start = Instant::now();
    let dyn_img = image::open(&image_path)?.to_rgba8();
    let (img_w, img_h) = dyn_img.dimensions();
    println!(
        "Loaded {}x{} image in {:.2}s. Preparing SIMD buffer...",
        img_w,
        img_h,
        load_start.elapsed().as_secs_f64()
    );

    let src_size = SizeF::new(img_w as f32, img_h as f32);
    let src_image = Image::from_vec_u8(img_w, img_h, dyn_img.into_raw(), PixelType::U8x4)?;

    let output_dir = PathBuf::from("learning/exercises/render-spike");
    std::fs::create_dir_all(&output_dir)?;

    let total_frames: u32 = env::var("SPIKE_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1800); // Default 1,800 frames = 60 seconds

    // 1. Benchmark 720p (1280x720) - Default PACE export resolution
    let res_720p = run_benchmark(
        "720p",
        1280,
        720,
        total_frames,
        &src_image,
        src_size,
        &output_dir,
    )?;

    // 2. Benchmark 1080p (1920x1080) - High-res PACE export resolution
    let res_1080p = run_benchmark(
        "1080p",
        1920,
        1080,
        total_frames,
        &src_image,
        src_size,
        &output_dir,
    )?;

    // Print summary comparison
    println!("\n========================================================");
    println!("  PACE Spike S1 Summary & Architectural Metrics");
    println!("========================================================");
    println!(
        "{:<8} | {:<10} | {:<10} | {:<10} | {:<12} | {:<14}",
        "Target", "Dimensions", "FPS", "Real-Time", "60s Render", "Est. 22m Video"
    );
    println!("-------------------------------------------------------------------------");
    for res in [&res_720p, &res_1080p] {
        println!(
            "{:<8} | {:<10} | {:<10.1} | {:<10.2}x | {:<12.1}s | {:<14.1} min",
            res.name,
            format!("{}x{}", res.width, res.height),
            res.fps,
            res.realtime_factor,
            res.duration_secs,
            res.est_22min_secs / 60.0
        );
    }
    println!("========================================================");
    println!("Output videos saved to:");
    println!("  - {:?}", res_720p.output_path);
    println!("  - {:?}", res_1080p.output_path);

    Ok(())
}
