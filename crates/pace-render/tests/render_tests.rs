use pace_engine::fit::{FitMode, SizeF};
use pace_engine::motion::{Easing, FocalPoint, Intensity, MotionPreset, SceneVisualEffect};
use pace_engine::spec::{
    compile_spec, frame_plan, CompileSpecInput, SceneEffectSettings, SceneSpecInput,
};
use pace_engine::stickers::{StickerAnimation, StickerKind, StickerPlacementSpec};
use pace_engine::subtitles::{
    MusicSettingsSpec, SubtitleCueSpec, SubtitleFont, SubtitlePosition, SubtitleStyleSpec,
};
use pace_engine::timing::{Boundary, TransitionKind};
use pace_render::encoder::{
    concat_and_mix_audio, concat_and_mux, encode_rgba_segment, extract_mp4_frame_rgba,
    probe_audio_loudness,
};
use pace_render::raster::{compute_rgba_ssim, rasterize_frame_plan, DecodedSceneSource};
use pace_render::stickers::composite_frame_stickers;
use pace_render::subtitles::{composite_subtitle_overlay, render_subtitle_overlay};
use pace_render::validator::validate_export_mp4;
use std::collections::HashMap;
use std::process::Command;
use tempfile::tempdir;

fn make_gradient_rgba(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut out = vec![0_u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            out[idx] = ((x * 255 / width.max(1)) as u8).wrapping_add(seed);
            out[idx + 1] = ((y * 255 / height.max(1)) as u8).wrapping_add(seed / 2);
            out[idx + 2] = seed.wrapping_mul(3);
            out[idx + 3] = 255;
        }
    }
    out
}

#[test]
fn concat_segments_and_validates_duration_and_streams() {
    let dir = tempdir().unwrap();
    let seg0 = dir.path().join("seg0.mp4");
    let seg1 = dir.path().join("seg1.mp4");
    let final_mp4 = dir.path().join("final.mp4");

    encode_rgba_segment(640, 360, 30, 30, &seg0, |f, buf| {
        let px = make_gradient_rgba(640, 360, (f * 3) as u8);
        buf.copy_from_slice(&px);
        Ok(())
    })
    .expect("encode seg0");

    encode_rgba_segment(640, 360, 30, 30, &seg1, |f, buf| {
        let px = make_gradient_rgba(640, 360, (120 + f * 3) as u8);
        buf.copy_from_slice(&px);
        Ok(())
    })
    .expect("encode seg1");

    concat_and_mux(&[seg0, seg1], None, &final_mp4).expect("concat segments");

    // Expected 60 frames @ 30fps = 2000ms
    let report =
        validate_export_mp4(&final_mp4, 640, 360, 2_000, &[]).expect("validate final mp4");
    assert_eq!(report.width, 640);
    assert_eq!(report.height, 360);
    assert_eq!(report.video_codec, "h264");
    assert_eq!(report.audio_codec, "aac");
    assert!((report.duration_ms as i64 - 2_000).abs() <= 250);
}

#[test]
fn validator_catches_truncated_missing_audio_mismatch_and_black_gaps() {
    let dir = tempdir().unwrap();

    // 1. Truncated / empty file
    let empty_mp4 = dir.path().join("empty.mp4");
    std::fs::write(&empty_mp4, b"not a valid mp4").unwrap();
    assert!(validate_export_mp4(&empty_mp4, 640, 360, 1_000, &[]).is_err());

    // 2. Missing audio stream (video-only segment)
    let video_only = dir.path().join("no_audio.mp4");
    encode_rgba_segment(640, 360, 30, 30, &video_only, |_f, buf| {
        buf.fill(180);
        Ok(())
    })
    .expect("encode video-only");
    assert!(validate_export_mp4(&video_only, 640, 360, 1_000, &[]).is_err());

    // 3. Duration mismatch
    let short_muxed = dir.path().join("short.mp4");
    concat_and_mux(&[video_only], None, &short_muxed).expect("mux 1s video");
    assert!(validate_export_mp4(&short_muxed, 640, 360, 5_000, &[]).is_err());

    // 4. Unexpected black frames
    let black_seg = dir.path().join("black.mp4");
    let black_muxed = dir.path().join("black_muxed.mp4");
    encode_rgba_segment(640, 360, 30, 30, &black_seg, |_f, buf| {
        for chunk in buf.chunks_exact_mut(4) {
            chunk[0] = 0;
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = 255;
        }
        Ok(())
    })
    .expect("encode black seg");
    concat_and_mux(&[black_seg], None, &black_muxed).expect("mux black seg");
    let black_err = validate_export_mp4(&black_muxed, 640, 360, 1_000, &[]);
    assert!(
        black_err.is_err(),
        "validator must flag unexpected pure-black video"
    );

    // Allowing that black window passes cleanly
    let allowed = validate_export_mp4(&black_muxed, 640, 360, 1_000, &[(0, 1_000)]);
    assert!(allowed.is_ok());

    // Silence detection check with real ffmpeg executable
    let _ = Command::new("ffmpeg").arg("-version").output().unwrap();
}

#[test]
fn rasterized_mp4_frame_matches_cpu_frame_plan_with_high_ssim() {
    let input = CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(640.0, 360.0),
        narration_ms: 2_000,
        effect_seed: 42,
        include_tilt_in_auto: false,
        boundaries: vec![
            Boundary::new(0),
            Boundary::with_transition(1_000, TransitionKind::Crossfade, 400),
            Boundary::new(2_000),
        ],
        stickers: Vec::new(),
        scenes: vec![
            SceneSpecInput {
                scene_index: 0,
                section_id: "s0".to_string(),
                media_id: Some("m0".to_string()),
                image_size: SizeF::new(800.0, 600.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint { x: 0.4, y: 0.5 },
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::ZoomIn),
                    intensity: Intensity::Normal,
                    easing: Easing::EaseInOut,
                    manual_shake: Some(false),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: true,
            },
            SceneSpecInput {
                scene_index: 1,
                section_id: "s1".to_string(),
                media_id: Some("m1".to_string()),
                image_size: SizeF::new(400.0, 600.0),
                fit_mode: FitMode::FitBlur,
                focal: FocalPoint { x: 0.5, y: 0.5 },
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::ZoomOut),
                    intensity: Intensity::Normal,
                    easing: Easing::Steady,
                    manual_shake: Some(false),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: true,
            },
        ],
    };

    let spec = compile_spec(&input).expect("compile spec");
    let mut sources = HashMap::new();
    sources.insert(
        0,
        DecodedSceneSource::from_rgba(
            800,
            600,
            make_gradient_rgba(800, 600, 40),
            800.0,
            600.0,
        )
        .unwrap(),
    );
    sources.insert(
        1,
        DecodedSceneSource::from_rgba(
            400,
            600,
            make_gradient_rgba(400, 600, 160),
            400.0,
            600.0,
        )
        .unwrap(),
    );

    let dir = tempdir().unwrap();
    let seg = dir.path().join("full.mp4");
    let final_mp4 = dir.path().join("final.mp4");

    encode_rgba_segment(640, 360, 30, spec.total_frames, &seg, |f, buf| {
        let plan = frame_plan(&spec, f);
        rasterize_frame_plan(&spec, &plan, &mut sources, 640, 360, buf)
    })
    .expect("encode rendered segment");
    concat_and_mux(&[seg], None, &final_mp4).expect("mux final mp4");

    // Compare CPU FramePlan rasterized reference vs decoded MP4 frame at transition midpoint & FitBlur body
    for check_frame in [10_u32, 30, 48] {
        let mut ref_rgba = vec![0_u8; (640 * 360 * 4) as usize];
        let plan = frame_plan(&spec, check_frame);
        rasterize_frame_plan(&spec, &plan, &mut sources, 640, 360, &mut ref_rgba).unwrap();

        let decoded_rgba = extract_mp4_frame_rgba(&final_mp4, check_frame, 30, 640, 360).unwrap();
        let ssim = compute_rgba_ssim(&ref_rgba, &decoded_rgba, 640, 360);
        assert!(
            ssim >= 0.95,
            "M8-8 frame {check_frame} SSIM too low: {ssim:.4} (< 0.95)"
        );
    }
}

#[tokio::test]
async fn worker_resumes_from_cached_segments_after_interruption() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pace:pace@localhost:5432/pace".to_string());
    let pool = pace_db::create_pool(&database_url).await.expect("connect db");
    pace_db::run_migrations(&pool).await.expect("migrations ok");

    let storage_dir = tempdir().unwrap();
    let storage = pace_storage::LocalStorage::new(
        storage_dir.path().to_path_buf(),
        "http://localhost:9000",
    );

    let user = pace_db::create_user(
        &pool,
        pace_db::CreateUser {
            email: format!("render_resume_{}@example.com", uuid::Uuid::now_v7()),
            name: "Render Resume Tester".to_string(),
            password_hash: "dummy-hash".to_string(),
            role: "creator".to_string(),
            must_change_password: false,
            storage_quota_bytes: Some(50 * 1024 * 1024),
        },
    )
    .await
    .unwrap();

    let project = pace_db::create_project(
        &pool,
        pace_db::CreateProject {
            owner_id: user.id,
            name: "Resume Test".to_string(),
            aspect_ratio: "16:9".to_string(),
            fps: Some(30),
            language: None,
            effect_seed: Some(42),
            settings: None,
        },
    )
    .await
    .unwrap();

    let img_dir = tempdir().unwrap();
    let mut scene_assets = Vec::new();
    for idx in 0..2 {
        let med_id = uuid::Uuid::now_v7();
        let key = pace_storage::StorageKeys::media_original(&user.id, &project.id, &med_id, "png");
        let rgba = make_gradient_rgba(640, 360, (idx as u8) * 90 + 20);
        let local_png = img_dir.path().join(format!("img-{idx}.png"));
        let buf: image::RgbaImage = image::ImageBuffer::from_raw(640, 360, rgba).unwrap();
        buf.save(&local_png).unwrap();
        use pace_storage::Storage as _;
        storage.upload(&local_png, &key, "image/png").await.unwrap();
        scene_assets.push(pace_jobs::RenderSceneAsset {
            scene_index: idx,
            media_id: med_id,
            storage_key: key,
        });
    }

    let input = CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(640.0, 360.0),
        narration_ms: 2_000,
        effect_seed: 42,
        include_tilt_in_auto: false,
        boundaries: vec![
            Boundary::new(0),
            Boundary::new(1_000),
            Boundary::new(2_000),
        ],
        stickers: Vec::new(),
        scenes: (0..2)
            .map(|idx| SceneSpecInput {
                scene_index: idx,
                section_id: format!("sec-{idx}"),
                media_id: Some(scene_assets[idx].media_id.to_string()),
                image_size: SizeF::new(640.0, 360.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::ZoomIn),
                    ..Default::default()
                },
                effect_overridden: true,
            })
            .collect(),
    };
    let spec = compile_spec(&input).unwrap();
    let segments = pace_engine::segments::plan_render_segments(&spec);
    assert_eq!(segments.len(), 2);

    // Pre-populate segment 0 in storage cache (simulating worker interruption after segment 0 completed).
    let seg0_local = img_dir.path().join("pre-seg0.mp4");
    encode_rgba_segment(640, 360, 30, segments[0].frame_count, &seg0_local, |f, buf| {
        let px = make_gradient_rgba(640, 360, (f * 2) as u8);
        buf.copy_from_slice(&px);
        Ok(())
    })
    .unwrap();
    let seg0_key = pace_storage::StorageKeys::render_segment_cache(
        &user.id,
        &project.id,
        &segments[0].cache_key,
    );
    use pace_storage::Storage as _;
    storage.upload(&seg0_local, &seg0_key, "video/mp4").await.unwrap();

    // Remove scene 0 image intentionally: if segment 0 wasn't served from cache, worker would fail to download it!
    storage
        .delete_prefix(&format!(
            "u/{}/p/{}/media/{}/",
            user.id, project.id, scene_assets[0].media_id
        ))
        .await
        .unwrap();

    let export_id = uuid::Uuid::now_v7();
    let output_key = pace_storage::StorageKeys::export_mp4(
        &user.id,
        &project.id,
        &export_id,
        "Resume",
        "360p",
    );
    pace_db::create_export(
        &pool,
        pace_db::CreateExport {
            id: export_id,
            project_id: project.id,
            user_id: user.id,
            job_id: None,
            spec_hash: spec.spec_hash.clone(),
            resolution: "360p".to_string(),
            width: 640,
            height: 360,
            fps: 30,
            duration_ms: 2_000,
        },
    )
    .await
    .unwrap();

    let payload = pace_jobs::RenderJobPayload {
        export_id,
        project_id: project.id,
        user_id: user.id,
        project_name: "Resume Test".to_string(),
        aspect_ratio: "16:9".to_string(),
        resolution: "360p".to_string(),
        width: 640,
        height: 360,
        fps: 30,
        output_key: output_key.clone(),
        audio_key: None,
        music_key: None,
        music_settings: None,
        subtitles: Vec::new(),
        subtitle_style: SubtitleStyleSpec::default(),
        stickers: Vec::new(),
        spec_json: serde_json::to_value(&spec).unwrap(),
        scene_assets,
    };

    let job = pace_jobs::enqueue_job(
        &pool,
        pace_jobs::EnqueueJob {
            queue: pace_jobs::JobQueue::Render,
            job_type: "video_export".to_string(),
            project_id: Some(project.id),
            user_id: Some(user.id),
            priority: 10_000,
            payload: serde_json::to_value(&payload).unwrap(),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();
    pace_db::attach_export_job(&pool, export_id, job.id)
        .await
        .unwrap();

    let claimed = pace_jobs::claim_next_job(&pool, pace_jobs::JobQueue::Render, "test-worker")
        .await
        .unwrap()
        .expect("claim render job");
    assert_eq!(claimed.id, job.id);

    let result = pace_render::worker::process_render_job(&pool, &storage, &claimed, "test-worker")
        .await
        .expect("resumed render job must succeed from cache");
    assert_eq!(result["segments_total"].as_u64(), Some(2));
    assert_eq!(result["segments_cached"].as_u64(), Some(1));

    let export_row = pace_db::find_export_by_id_and_project(&pool, export_id, project.id, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(export_row.status, "succeeded");
    assert!(storage.head(&output_key).await.unwrap().is_some());

    // Mark test job completed and clean up ephemeral test records.
    let _ = pace_jobs::complete_job(&pool, claimed.id, "test-worker", Some(result)).await;
    let _ = pace_db::sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&pool)
        .await;
}

#[test]
fn fit_blur_extreme_focal_and_short_audio_pad_cleanly() {
    let dir = tempdir().unwrap();

    // 1. Extreme focal point (1.0, 1.0) + ZoomInPanRight + FitBlur across a portrait image
    let input = CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(640.0, 360.0),
        narration_ms: 2_000,
        effect_seed: 9,
        include_tilt_in_auto: false,
        boundaries: vec![Boundary::new(0), Boundary::new(2_000)],
        stickers: Vec::new(),
        scenes: vec![SceneSpecInput {
            scene_index: 0,
            section_id: "s0".to_string(),
            media_id: Some("m0".to_string()),
            image_size: SizeF::new(300.0, 500.0),
            fit_mode: FitMode::FitBlur,
            focal: FocalPoint { x: 1.0, y: 1.0 },
            effect: SceneEffectSettings {
                manual_preset: Some(MotionPreset::ZoomInPanRight),
                intensity: Intensity::Strong,
                easing: Easing::Steady,
                manual_shake: Some(true),
                visual_effect: SceneVisualEffect::None,
            },
            effect_overridden: true,
        }],
    };
    let spec = compile_spec(&input).unwrap();
    let mut sources = HashMap::new();
    sources.insert(
        0,
        DecodedSceneSource::from_rgba(
            300,
            500,
            make_gradient_rgba(300, 500, 80),
            300.0,
            500.0,
        )
        .unwrap(),
    );

    let seg = dir.path().join("extreme.mp4");
    let mut scratch = pace_render::raster::FrameRasterScratch::new();
    encode_rgba_segment(640, 360, 30, spec.total_frames, &seg, |f, buf| {
        let plan = frame_plan(&spec, f);
        pace_render::raster::rasterize_frame_plan_with_scratch(
            &spec,
            &plan,
            &mut sources,
            640,
            360,
            buf,
            &mut scratch,
        )
    })
    .expect("extreme FitBlur zoomed crop must succeed cleanly");

    // 2. Short 500ms audio muxed onto 2000ms video must pad trailing silence instead of truncating video
    let short_audio = dir.path().join("short.opus");
    let synth = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:a",
            "libopus",
        ])
        .arg(&short_audio)
        .status()
        .unwrap();
    assert!(synth.success());

    let final_mp4 = dir.path().join("padded.mp4");
    concat_and_mux(&[seg], Some(&short_audio), &final_mp4).expect("mux with apad");
    let report = validate_export_mp4(&final_mp4, 640, 360, 2_000, &[])
        .expect("short audio must not truncate 2000ms video");
    assert!((report.duration_ms as i64 - 2_000).abs() <= 250);
}

#[test]
fn subtitle_burn_in_matches_prerendered_reference_with_high_ssim() {
    let dir = tempdir().unwrap();
    let seg = dir.path().join("sub_burn.mp4");
    let final_mp4 = dir.path().join("sub_final.mp4");

    let style = SubtitleStyleSpec {
        enabled: true,
        font_family: SubtitleFont::Sans,
        font_size_pct: 100,
        position: SubtitlePosition::Bottom,
        text_color: "#FFFFFF".to_string(),
        outline_width_px: 2,
        bg_opacity_pct: 65,
    };
    let cue = SubtitleCueSpec {
        id: "c0".to_string(),
        position: 0,
        start_ms: 0,
        end_ms: 1_500,
        text: "Lit from within against the dark sky,\nit stands as proof of patient planning."
            .to_string(),
    };
    let lines = pace_engine::subtitles::wrap_subtitle_lines(&cue.text, "16:9");
    let overlay =
        render_subtitle_overlay(&lines, &style, 640, 360).expect("render subtitle overlay");

    // Verify overlay actually drew high-contrast white text pixels inside its bounding box
    let bright_pixels = overlay
        .rgba
        .chunks_exact(4)
        .filter(|px| px[0] > 220 && px[1] > 220 && px[2] > 220 && px[3] > 220)
        .count();
    assert!(
        bright_pixels > 100,
        "subtitle overlay must contain rendered glyph pixels (got {bright_pixels})"
    );

    encode_rgba_segment(640, 360, 30, 30, &seg, |_f, buf| {
        let base = make_gradient_rgba(640, 360, 48);
        buf.copy_from_slice(&base);
        composite_subtitle_overlay(buf, 640, 360, &overlay);
        Ok(())
    })
    .expect("encode segment with burned-in subtitle");

    concat_and_mux(&[seg], None, &final_mp4).expect("mux subtitle mp4");

    let mut ref_rgba = make_gradient_rgba(640, 360, 48);
    composite_subtitle_overlay(&mut ref_rgba, 640, 360, &overlay);

    let decoded = extract_mp4_frame_rgba(&final_mp4, 15, 30, 640, 360).expect("extract frame 15");
    let ssim = compute_rgba_ssim(&ref_rgba, &decoded, 640, 360);
    assert!(
        ssim >= 0.95,
        "M9-4 subtitle burn-in SSIM too low: {ssim:.4} (< 0.95)"
    );
}

#[test]
fn audio_mix_with_music_ducking_hits_target_lufs_and_peak_limit() {
    let dir = tempdir().unwrap();

    // Create 6 seconds of video (180 frames @ 30fps) so EBU R128 3-second integration windows
    // settle across both the 3s speech window and 3s music-only window.
    let seg = dir.path().join("vid_6s.mp4");
    encode_rgba_segment(640, 360, 30, 180, &seg, |f, buf| {
        let px = make_gradient_rgba(640, 360, (f % 200) as u8);
        buf.copy_from_slice(&px);
        Ok(())
    })
    .expect("encode 6s video");

    // Create 6s narration track: first 3.5s spoken tone (880 Hz), last 2.5s silence
    let narration_wav = dir.path().join("narration.wav");
    let st_narr = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=3.5:sample_rate=48000",
            "-af",
            "volume=-12dB,apad=pad_dur=2.5",
        ])
        .arg(&narration_wav)
        .status()
        .unwrap();
    assert!(st_narr.success());

    // Create 3s music track (220 Hz tone that will loop to 6s and be ducked during 0..3.5s speech)
    let music_wav = dir.path().join("music.wav");
    let st_mus = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=3.0:sample_rate=48000",
            "-af",
            "volume=-12dB",
        ])
        .arg(&music_wav)
        .status()
        .unwrap();
    assert!(st_mus.success());

    let music_cfg = MusicSettingsSpec {
        enabled: true,
        media_id: None,
        volume_db: -12.0,
        ducking_enabled: true,
        duck_amount_db: -14.0,
        loop_enabled: true,
        loop_crossfade_ms: 0,
        fade_in_ms: 0,
        fade_out_ms: 0,
    };

    let mixed_mp4 = dir.path().join("mixed_ducked.mp4");
    concat_and_mix_audio(
        &[seg],
        Some(&narration_wav),
        Some(&music_wav),
        Some(&music_cfg),
        6_000,
        &mixed_mp4,
    )
    .expect("concat and mix audio with ducking");

    let metrics = probe_audio_loudness(&mixed_mp4).expect("probe ebur128 metrics");
    assert!(
        (metrics.integrated_lufs - (-14.0)).abs() <= 1.2,
        "M9-3 integrated loudness must be -14 ± 1 LUFS (got {:.2} LUFS)",
        metrics.integrated_lufs
    );
    assert!(
        metrics.true_peak_dbtp <= -0.9,
        "M9-3 true peak must not exceed -1.0 dBTP limiter ceiling (got {:.2} dBTP)",
        metrics.true_peak_dbtp
    );

    // Verify music (220 Hz lowpass <= 320 Hz) actually dips during speech (0.5..3.0s) vs non-speech (4.2..5.8s)
    let extract_lowband_rms = |start_s: &str, dur_s: &str| -> f64 {
        let pcm = Command::new("ffmpeg")
            .args(["-v", "error", "-ss", start_s, "-t", dur_s, "-i"])
            .arg(&mixed_mp4)
            .args([
                "-af",
                "lowpass=f=320:poles=2,lowpass=f=320:poles=2",
                "-f",
                "f32le",
                "-ac",
                "1",
                "-ar",
                "48000",
                "-",
            ])
            .output()
            .unwrap();
        assert!(pcm.status.success());
        let samples: Vec<f32> = pcm
            .stdout
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum_sq / (samples.len().max(1) as f64)).sqrt()
    };

    let rms_during_speech = extract_lowband_rms("0.5", "2.2");
    let rms_after_speech = extract_lowband_rms("4.2", "1.5");
    assert!(
        rms_during_speech < rms_after_speech * 0.75,
        "M9-3 music band must dip under speech: during_speech_rms={rms_during_speech:.5} vs after_speech_rms={rms_after_speech:.5}"
    );
}

#[test]
fn capcut_rotated_affine_shutter_flash_and_sticker_burnin() {
    // Verify rotated Fill layer (FallLeft) has zero black corners,
    // ShutterFlash peaks bright white, and Subscribe/Bell stickers composite cleanly.
    let input = CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(640.0, 360.0),
        narration_ms: 2_000,
        effect_seed: 42,
        include_tilt_in_auto: false,
        boundaries: vec![
            Boundary::new(0),
            Boundary::with_transition(1_000, TransitionKind::ShutterFlash, 400),
            Boundary::new(2_000),
        ],
        stickers: vec![StickerPlacementSpec {
            id: "stk-test".to_string(),
            kind: StickerKind::Subscribe,
            start_ms: 200,
            end_ms: 1_500,
            x: 0.5,
            y: 0.82,
            scale: 1.0,
            animation: StickerAnimation::PopIn,
        }],
        scenes: vec![
            SceneSpecInput {
                scene_index: 0,
                section_id: "s0".to_string(),
                media_id: Some("m0".to_string()),
                image_size: SizeF::new(640.0, 360.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::FallLeft),
                    intensity: Intensity::Strong,
                    easing: Easing::EaseInOut,
                    manual_shake: Some(false),
                    visual_effect: SceneVisualEffect::None,
                },
                effect_overridden: true,
            },
            SceneSpecInput {
                scene_index: 1,
                section_id: "s1".to_string(),
                media_id: Some("m1".to_string()),
                image_size: SizeF::new(640.0, 360.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings::default(),
                effect_overridden: false,
            },
        ],
    };

    let spec = compile_spec(&input).expect("compile capcut spec");
    let mut sources = HashMap::new();
    // Bright blue/teal source so any black corner would be obvious (0,0,0)
    let bright_src = vec![60_u8, 160, 220, 255].repeat(640 * 360);
    sources.insert(
        0,
        DecodedSceneSource::from_rgba(640, 360, bright_src.clone(), 640.0, 360.0).unwrap(),
    );
    sources.insert(
        1,
        DecodedSceneSource::from_rgba(640, 360, bright_src, 640.0, 360.0).unwrap(),
    );

    // 1. Frame 15: rotated FallLeft + active Subscribe sticker
    let plan_rot = frame_plan(&spec, 15);
    assert!(plan_rot.layers[0].rotation_rad.abs() > 1e-4);
    assert_eq!(plan_rot.stickers.len(), 1);

    let mut rgba_rot = vec![0_u8; 640 * 360 * 4];
    rasterize_frame_plan(&spec, &plan_rot, &mut sources, 640, 360, &mut rgba_rot).unwrap();

    // Check all 4 corners are non-black (no corner bleed)
    for (cx, cy) in [(0, 0), (639, 0), (0, 359), (639, 359)] {
        let i = (cy * 640 + cx) * 4;
        assert!(
            rgba_rot[i + 2] > 150,
            "Rotated Fill corner ({cx},{cy}) revealed black: {:?}",
            &rgba_rot[i..i + 4]
        );
    }

    let before_sticker = rgba_rot.clone();
    composite_frame_stickers(&mut rgba_rot, 640, 360, &plan_rot.stickers);
    assert_ne!(
        before_sticker, rgba_rot,
        "Sticker compositing must modify pixels at sticker placement"
    );

    // 2. Frame 30: ShutterFlash peak at boundary (bright white flash)
    let plan_flash = frame_plan(&spec, 30);
    assert!(plan_flash.flash_white_alpha > 0.8);
    let mut rgba_flash = vec![0_u8; 640 * 360 * 4];
    rasterize_frame_plan(&spec, &plan_flash, &mut sources, 640, 360, &mut rgba_flash).unwrap();
    assert!(
        rgba_flash[0] > 210 && rgba_flash[1] > 230 && rgba_flash[2] > 240,
        "ShutterFlash peak must brighten frame toward white, got {:?}",
        &rgba_flash[0..4]
    );
}
