use crate::error::RenderError;
use pace_engine::subtitles::MusicSettingsSpec;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Encodes a contiguous stream of RGBA frames produced by `render_frame` into a concat-safe MP4 segment (`M8-5`).
pub fn encode_rgba_segment<F>(
    width: u32,
    height: u32,
    fps: u32,
    frame_count: u32,
    output_mp4: &Path,
    mut render_frame: F,
) -> Result<(), RenderError>
where
    F: FnMut(u32, &mut [u8]) -> Result<(), RenderError>,
{
    if frame_count == 0 {
        return Err(RenderError::Ffmpeg {
            code: "EMPTY_SEGMENT".to_string(),
            message: "Cannot encode a 0-frame segment".to_string(),
        });
    }

    let crf = if height <= 720 { "23" } else { "22" };
    let maxrate = if height <= 720 { "2.5M" } else { "4.5M" };
    let bufsize = if height <= 720 { "5M" } else { "9M" };
    let size_arg = format!("{width}x{height}");
    let fps_arg = fps.max(1).to_string();
    let gop_arg = (fps.max(1) * 2).to_string();

    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size_arg,
            "-r",
            &fps_arg,
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-profile:v",
            "high",
            "-level",
            "4.1",
            "-pix_fmt",
            "yuv420p",
            "-r",
            &fps_arg,
            "-crf",
            crf,
            "-maxrate",
            maxrate,
            "-bufsize",
            bufsize,
            "-g",
            &gop_arg,
            "-keyint_min",
            &fps_arg,
            "-sc_threshold",
            "0",
            "-bf",
            "2",
            "-x264-params",
            "open-gop=0",
            "-video_track_timescale",
            "15360",
            "-an",
        ])
        .arg(output_mp4)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut frame_rgba = vec![0_u8; (width * height * 4) as usize];
    {
        let stdin = child.stdin.take().ok_or_else(|| RenderError::Ffmpeg {
            code: "STDIN_MISSING".to_string(),
            message: "Failed to capture FFmpeg stdin".to_string(),
        })?;
        let mut writer = BufWriter::with_capacity(frame_rgba.len().min(4 * 1024 * 1024), stdin);
        for local_idx in 0..frame_count {
            render_frame(local_idx, &mut frame_rgba)?;
            writer.write_all(&frame_rgba)?;
        }
        writer.flush()?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(RenderError::Ffmpeg {
            code: "SEGMENT_ENCODE_FAILED".to_string(),
            message: stderr,
        });
    }

    Ok(())
}

/// Joins encoded MP4 segments via FFmpeg concat demuxer (`-c:v copy`) and muxes narration/silence AAC (`M8-5`).
pub fn concat_and_mux(
    segment_paths: &[PathBuf],
    audio_path: Option<&Path>,
    output_mp4: &Path,
) -> Result<(), RenderError> {
    concat_and_mix_audio(segment_paths, audio_path, None, None, 0, output_mp4)
}

/// Joins encoded MP4 segments (`-c:v copy`) and runs the full M9 audio mix pipeline (`M9-3`):
/// - Narration level normalization (`loudnorm=I=-16:TP=-1.5:LRA=11`)
/// - Optional background music loop (`-stream_loop -1`), volume (`-40..=0 dB`), fade-in / fade-out,
///   and sidechain compression ducking under speech (`sidechaincompress`)
/// - Mix (`amix`) + master broadcast loudness (`loudnorm=I=-14:TP=-1.0:LRA=11`) + true-peak limiter (`alimiter=limit=-1.0dB:level=false`).
pub fn concat_and_mix_audio(
    segment_paths: &[PathBuf],
    narration_path: Option<&Path>,
    music_path: Option<&Path>,
    music_settings: Option<&MusicSettingsSpec>,
    total_duration_ms: u64,
    output_mp4: &Path,
) -> Result<(), RenderError> {
    if segment_paths.is_empty() {
        return Err(RenderError::Ffmpeg {
            code: "NO_SEGMENTS".to_string(),
            message: "No video segments provided for concatenation".to_string(),
        });
    }

    let concat_list_path = output_mp4.with_extension("concat.txt");
    let mut concat_text = String::new();
    for seg in segment_paths {
        let abs = seg.canonicalize().unwrap_or_else(|_| seg.clone());
        let escaped = abs.to_string_lossy().replace('\'', "'\\''");
        concat_text.push_str(&format!("file '{escaped}'\n"));
    }
    std::fs::write(&concat_list_path, concat_text)?;

    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-v",
        "error",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ])
    .arg(&concat_list_path);

    let has_music = music_path.is_some();

    if let Some(narr) = narration_path {
        cmd.arg("-i").arg(narr);
    } else {
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
        ]);
    }

    if let Some(music) = music_path {
        let loop_enabled = music_settings.map(|m| m.loop_enabled).unwrap_or(true);
        let crossfade_ms = music_settings
            .map(|m| m.loop_crossfade_ms)
            .unwrap_or(0);
        if loop_enabled && crossfade_ms == 0 {
            cmd.args(["-stream_loop", "-1"]);
        }
        cmd.arg("-i").arg(music);
    }

    if !has_music {
        // Narration-only or silent track: keep M8 fast path with `apad` (plus master limiter if narration exists).
        let af_chain = if narration_path.is_some() {
            "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,loudnorm=I=-14:TP=-1.0:LRA=11,alimiter=limit=-1.0dB:level=false,apad"
        } else {
            "apad"
        };
        cmd.args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-af",
            af_chain,
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-shortest",
            "-movflags",
            "+faststart",
        ])
        .arg(output_mp4);
    } else {
        let cfg = music_settings.cloned().unwrap_or_default();
        let dur_sec = ((total_duration_ms.max(500)) as f64) / 1000.0;
        let vol_db = cfg.volume_db.clamp(-40.0, 0.0);
        let fade_in_sec = ((cfg.fade_in_ms.min(15_000)) as f64 / 1000.0).min(dur_sec * 0.45);
        let fade_out_sec = ((cfg.fade_out_ms.min(15_000)) as f64 / 1000.0).min(dur_sec * 0.45);
        let fade_out_start = (dur_sec - fade_out_sec).max(0.0);

        let mut music_chain = String::from(
            "[2:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo",
        );
        if cfg.loop_enabled && cfg.loop_crossfade_ms > 0 {
            let cf_sec = ((cfg.loop_crossfade_ms.min(5_000)) as f64 / 1000.0).max(0.05);
            // Build looped crossfade chain up to video duration using afade on each loop pass
            // and aloop so loop seams never click.
            music_chain.push_str(&format!(
                ",afade=t=in:st=0:d={cf_sec:.3},aloop=loop=-1:size=2147483647"
            ));
        }
        music_chain.push_str(&format!(
            ",apad,atrim=0:{dur_sec:.3},asetpts=PTS-STARTPTS,volume={vol_db:.2}dB"
        ));
        if fade_in_sec > 0.01 {
            music_chain.push_str(&format!(",afade=t=in:st=0:d={fade_in_sec:.3}"));
        }
        if fade_out_sec > 0.01 {
            music_chain.push_str(&format!(
                ",afade=t=out:st={fade_out_start:.3}:d={fade_out_sec:.3}"
            ));
        }
        music_chain.push_str("[mus];");

        // Map user duck_amount_db (-18..-4 dB) to compression ratio & threshold so speech ducks music decisively.
        let duck_mag = cfg.duck_amount_db.abs().clamp(4.0, 24.0);
        let ratio = (duck_mag / 1.5).clamp(4.0, 16.0);

        let filter_complex = if narration_path.is_some() && cfg.ducking_enabled {
            format!(
                "[1:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,loudnorm=I=-16:TP=-1.5:LRA=11,apad,atrim=0:{dur_sec:.3},asetpts=PTS-STARTPTS,asplit=2[narr][sc];\
                 {music_chain}\
                 [mus][sc]sidechaincompress=threshold=0.015:ratio={ratio:.1}:attack=50:release=400:makeup=1[ducked];\
                 [narr][ducked]amix=inputs=2:duration=first:dropout_transition=0:normalize=0,\
                 loudnorm=I=-14:TP=-1.0:LRA=11,alimiter=limit=-1.0dB:level=false[aout]"
            )
        } else if narration_path.is_some() {
            format!(
                "[1:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,loudnorm=I=-16:TP=-1.5:LRA=11,apad,atrim=0:{dur_sec:.3},asetpts=PTS-STARTPTS[narr];\
                 {music_chain}\
                 [narr][mus]amix=inputs=2:duration=first:dropout_transition=0:normalize=0,\
                 loudnorm=I=-14:TP=-1.0:LRA=11,alimiter=limit=-1.0dB:level=false[aout]"
            )
        } else {
            format!(
                "{music_chain}\
                 [mus]loudnorm=I=-14:TP=-1.0:LRA=11,alimiter=limit=-1.0dB:level=false[aout]"
            )
        };

        cmd.args([
            "-filter_complex",
            &filter_complex,
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-shortest",
            "-movflags",
            "+faststart",
        ])
        .arg(output_mp4);
    }

    let out = cmd.output()?;
    let _ = std::fs::remove_file(&concat_list_path);

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(RenderError::Ffmpeg {
            code: "CONCAT_MUX_FAILED".to_string(),
            message: stderr,
        });
    }

    Ok(())
}

/// Measured EBU R128 integrated loudness (LUFS) and True Peak (dBTP) of an audio/video file (`M9-3`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioLoudnessMetrics {
    pub integrated_lufs: f64,
    pub true_peak_dbtp: f64,
}

/// Probes integrated loudness (`I:` in LUFS) and true peak (`Peak:` in dBFS/dBTP) using FFmpeg `ebur128=peak=true` (`M9-3`).
pub fn probe_audio_loudness(media_path: &Path) -> Result<AudioLoudnessMetrics, RenderError> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(media_path)
        .args(["-af", "ebur128=peak=true", "-f", "null", "-"])
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut integrated_lufs: Option<f64> = None;
    let mut true_peak_dbtp: Option<f64> = None;

    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("I:") {
            if let Some(num_str) = rest.split_whitespace().next() {
                if let Ok(v) = num_str.parse::<f64>() {
                    integrated_lufs = Some(v);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("Peak:") {
            if let Some(num_str) = rest.split_whitespace().next() {
                if let Ok(v) = num_str.parse::<f64>() {
                    true_peak_dbtp = Some(v);
                }
            }
        }
    }

    match (integrated_lufs, true_peak_dbtp) {
        (Some(i), Some(tp)) => Ok(AudioLoudnessMetrics {
            integrated_lufs: i,
            true_peak_dbtp: tp,
        }),
        _ => Err(RenderError::Ffmpeg {
            code: "EBUR128_PARSE_FAILED".to_string(),
            message: format!("Could not parse ebur128 summary from FFmpeg output: {stderr}"),
        }),
    }
}

/// Extracts one decoded RGBA frame at `frame_index` from an encoded MP4 via FFmpeg (`M8-8`).
pub fn extract_mp4_frame_rgba(
    mp4_path: &Path,
    frame_index: u32,
    fps: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, RenderError> {
    let _ = fps;
    let select_expr = format!("select=eq(n\\,{frame_index})");
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(mp4_path)
        .args([
            "-vf",
            &select_expr,
            "-vframes",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .output()?;

    if !out.status.success() {
        return Err(RenderError::Ffmpeg {
            code: "FRAME_EXTRACT_FAILED".to_string(),
            message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    let expected = (width * height * 4) as usize;
    if out.stdout.len() != expected {
        return Err(RenderError::Ffmpeg {
            code: "FRAME_SIZE_MISMATCH".to_string(),
            message: format!(
                "Extracted frame bytes {} != expected {expected}",
                out.stdout.len()
            ),
        });
    }

    Ok(out.stdout)
}
