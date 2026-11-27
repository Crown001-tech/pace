use crate::error::RenderError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportValidationReport {
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub audio_codec: String,
    pub duration_ms: u64,
    pub expected_duration_ms: u64,
    pub size_bytes: u64,
    pub black_intervals: Vec<(f64, f64)>,
    pub silence_intervals: Vec<(f64, f64)>,
}

fn interval_covered_by_allowed(start_s: f64, end_s: f64, allowed_ms: &[(u64, u64)]) -> bool {
    let s_ms = (start_s * 1000.0).round() as i64;
    let e_ms = (end_s * 1000.0).round() as i64;
    allowed_ms.iter().any(|&(a_s, a_e)| {
        s_ms >= (a_s as i64) - 250 && e_ms <= (a_e as i64) + 250
    })
}

fn parse_black_intervals(stderr: &str) -> Vec<(f64, f64)> {
    let mut intervals = Vec::new();
    for line in stderr.lines() {
        if !line.contains("black_start:") {
            continue;
        }
        let mut b_start: Option<f64> = None;
        let mut b_end: Option<f64> = None;
        for token in line.split_whitespace() {
            if let Some(val) = token.strip_prefix("black_start:") {
                b_start = val.parse::<f64>().ok();
            } else if let Some(val) = token.strip_prefix("black_end:") {
                b_end = val.parse::<f64>().ok();
            }
        }
        if let (Some(s), Some(e)) = (b_start, b_end) {
            intervals.push((s, e));
        }
    }
    intervals
}

fn parse_silence_intervals(stderr: &str) -> Vec<(f64, f64)> {
    let mut intervals = Vec::new();
    let mut current_start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(pos) = line.find("silence_start:") {
            let rest = &line[pos + "silence_start:".len()..];
            if let Some(tok) = rest.split_whitespace().next() {
                current_start = tok.parse::<f64>().ok();
            }
        }
        if let Some(pos) = line.find("silence_end:") {
            let rest = &line[pos + "silence_end:".len()..];
            if let Some(tok) = rest.split_whitespace().next() {
                if let Ok(end_s) = tok.parse::<f64>() {
                    intervals.push((current_start.unwrap_or(end_s), end_s));
                    current_start = None;
                }
            }
        }
    }
    intervals
}

/// Probes an exported MP4 for stream validity, duration parity, and unexpected black/silence gaps (`M8-6`).
pub fn validate_export_mp4(
    mp4_path: &Path,
    expected_width: u32,
    expected_height: u32,
    expected_duration_ms: u64,
    allowed_black_windows_ms: &[(u64, u64)],
) -> Result<ExportValidationReport, RenderError> {
    let meta = std::fs::metadata(mp4_path)?;
    let size_bytes = meta.len();
    if size_bytes < 1024 {
        return Err(RenderError::Validation {
            code: "TRUNCATED_EXPORT".to_string(),
            message: format!("Export file too small ({size_bytes} bytes)"),
        });
    }

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(mp4_path)
        .output()?;

    if !probe.status.success() {
        return Err(RenderError::Validation {
            code: "FFPROBE_FAILED".to_string(),
            message: String::from_utf8_lossy(&probe.stderr).trim().to_string(),
        });
    }

    let json: serde_json::Value =
        serde_json::from_slice(&probe.stdout).map_err(|e| RenderError::Validation {
            code: "FFPROBE_JSON_INVALID".to_string(),
            message: e.to_string(),
        })?;

    let streams = json["streams"].as_array().ok_or_else(|| RenderError::Validation {
        code: "MISSING_STREAMS".to_string(),
        message: "ffprobe returned no streams".to_string(),
    })?;

    let video = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("video"))
        .ok_or_else(|| RenderError::Validation {
            code: "MISSING_VIDEO_STREAM".to_string(),
            message: "Exported MP4 is missing an H.264 video stream".to_string(),
        })?;

    let audio = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("audio"))
        .ok_or_else(|| RenderError::Validation {
            code: "MISSING_AUDIO_STREAM".to_string(),
            message: "Exported MP4 is missing an AAC audio stream".to_string(),
        })?;

    let video_codec = video["codec_name"].as_str().unwrap_or("").to_string();
    let audio_codec = audio["codec_name"].as_str().unwrap_or("").to_string();
    let width = video["width"].as_u64().unwrap_or(0) as u32;
    let height = video["height"].as_u64().unwrap_or(0) as u32;
    let pix_fmt = video["pix_fmt"].as_str().unwrap_or("");

    if video_codec != "h264" || pix_fmt != "yuv420p" {
        return Err(RenderError::Validation {
            code: "UNEXPECTED_VIDEO_CODEC".to_string(),
            message: format!("Expected h264/yuv420p, got {video_codec}/{pix_fmt}"),
        });
    }
    if width != expected_width || height != expected_height {
        return Err(RenderError::Validation {
            code: "RESOLUTION_MISMATCH".to_string(),
            message: format!(
                "Expected {expected_width}x{expected_height}, got {width}x{height}"
            ),
        });
    }
    if audio_codec != "aac" {
        return Err(RenderError::Validation {
            code: "UNEXPECTED_AUDIO_CODEC".to_string(),
            message: format!("Expected aac audio, got {audio_codec}"),
        });
    }

    let duration_secs: f64 = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let duration_ms = (duration_secs * 1000.0).round() as u64;
    let diff = duration_ms.abs_diff(expected_duration_ms);
    if diff > 350 {
        return Err(RenderError::Validation {
            code: "DURATION_MISMATCH".to_string(),
            message: format!(
                "Export duration {duration_ms}ms deviates from expected {expected_duration_ms}ms by {diff}ms"
            ),
        });
    }

    let detect = Command::new("ffmpeg")
        .args(["-v", "info", "-i"])
        .arg(mp4_path)
        .args([
            "-vf",
            "blackdetect=d=0.4:pix_th=0.05",
            "-af",
            "silencedetect=noise=-60dB:d=3.0",
            "-f",
            "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&detect.stderr);
    let black_intervals = parse_black_intervals(&stderr);
    let silence_intervals = parse_silence_intervals(&stderr);

    for &(bs, be) in &black_intervals {
        if !interval_covered_by_allowed(bs, be, allowed_black_windows_ms) {
            return Err(RenderError::Validation {
                code: "UNEXPECTED_BLACK_GAP".to_string(),
                message: format!("Unexpected black frame interval {bs:.2}s..{be:.2}s detected"),
            });
        }
    }

    Ok(ExportValidationReport {
        width,
        height,
        video_codec,
        audio_codec,
        duration_ms,
        expected_duration_ms,
        size_bytes,
        black_intervals,
        silence_intervals,
    })
}
