use crate::config::RenderConfig;
use crate::encoder::{concat_and_mix_audio, encode_rgba_segment};
use crate::error::RenderError;
use crate::raster::{rasterize_frame_plan_with_scratch, DecodedSceneSource, FrameRasterScratch};
use crate::stickers::composite_frame_stickers;
use crate::subtitles::{composite_subtitle_overlay, SubtitleOverlayCache};
use crate::validator::validate_export_mp4;
use pace_db::{create_pool, run_migrations, DbPool};
use pace_engine::fit::SizeF;
use pace_engine::segments::plan_render_segments_with_overlays;
use pace_engine::spec::{frame_plan, RenderSpec};
use pace_engine::stickers::evaluate_stickers_at;
use pace_engine::subtitles::active_subtitle_for_frame;
use pace_engine::timing::TransitionKind;
use pace_jobs::{
    claim_next_job, complete_job, fail_job, heartbeat_job, JobQueue, JobRecord, RenderJobPayload,
};
use pace_storage::{S3Config, S3Storage, Storage, StorageKeys};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::time::{sleep, Duration};

fn rewrite_spec_resolution(mut spec: RenderSpec, width: u32, height: u32) -> RenderSpec {
    spec.frame_size = SizeF::new(width as f32, height as f32);
    spec
}

async fn run_blocking_with_heartbeat<F, T>(
    pool: &DbPool,
    job_id: uuid::Uuid,
    worker_id: &str,
    stage: &str,
    base_progress: i32,
    span_progress: i32,
    total_units: u32,
    completed_units: Arc<AtomicU32>,
    work: F,
) -> Result<T, RenderError>
where
    F: FnOnce() -> Result<T, RenderError> + Send + 'static,
    T: Send + 'static,
{
    let mut handle = tokio::task::spawn_blocking(work);
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    interval.tick().await;

    loop {
        tokio::select! {
            res = &mut handle => {
                return res.map_err(|e| RenderError::InvalidPayload(format!("render blocking task panicked: {e}")))?;
            }
            _ = interval.tick() => {
                let done = completed_units.load(Ordering::Relaxed).min(total_units);
                let frac = if total_units == 0 {
                    0
                } else {
                    ((done as i64 * span_progress as i64) / total_units as i64) as i32
                };
                let _ = heartbeat_job(pool, job_id, worker_id, base_progress + frac, Some(stage)).await;
            }
        }
    }
}

/// Executes a claimed `render` job end-to-end: plans segments (with subtitle cache invalidation),
/// checks S3/DB cache, CPU-rasterizes missing segments with burned-in `ab_glyph` subtitles,
/// concatenates + mixes/ducks narration & background music to `-14 LUFS`, validates MP4, and uploads (`M8-1..M9-4`).
pub async fn process_render_job(
    pool: &DbPool,
    storage: &dyn Storage,
    job: &JobRecord,
    worker_id: &str,
) -> Result<serde_json::Value, RenderError> {
    let payload: RenderJobPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| RenderError::InvalidPayload(e.to_string()))?;

    pace_db::mark_export_running(pool, payload.export_id).await?;
    let _ = heartbeat_job(pool, job.id, worker_id, 5, Some("planning_segments")).await;

    let raw_spec: RenderSpec = serde_json::from_value(payload.spec_json.clone())
        .map_err(|e| RenderError::InvalidPayload(format!("invalid RenderSpec JSON: {e}")))?;
    let spec = Arc::new(rewrite_spec_resolution(
        raw_spec,
        payload.width,
        payload.height,
    ));
    let segments = plan_render_segments_with_overlays(
        &spec,
        &payload.subtitles,
        &payload.subtitle_style,
        &payload.stickers,
    );
    if segments.is_empty() {
        return Err(RenderError::InvalidPayload(
            "RenderSpec contains zero segments".to_string(),
        ));
    }

    let work_dir = tempdir()?;
    let mut asset_map: HashMap<usize, String> = HashMap::new();
    for asset in &payload.scene_assets {
        asset_map.insert(asset.scene_index, asset.storage_key.clone());
    }

    let subtitles_arc = Arc::new(payload.subtitles.clone());
    let subtitle_style = payload.subtitle_style.clone();
    let stickers_arc = Arc::new(payload.stickers.clone());
    let aspect_ratio = payload.aspect_ratio.clone();

    let mut decoded_sources: HashMap<usize, DecodedSceneSource> = HashMap::new();
    let mut segment_files: Vec<PathBuf> = Vec::with_capacity(segments.len());
    let total_segs = segments.len().max(1);
    let mut cache_hits = 0_usize;
    let total_frames_all = spec.total_frames.max(1);
    let frames_done = Arc::new(AtomicU32::new(0));

    for (idx, seg) in segments.iter().enumerate() {
        let stage = format!("rendering_segment_{}_of_{}", idx + 1, total_segs);
        let base_progress =
            8 + ((frames_done.load(Ordering::Relaxed) as usize * 75) / total_frames_all as usize) as i32;
        let _ = heartbeat_job(pool, job.id, worker_id, base_progress, Some(&stage)).await;

        let seg_key = StorageKeys::render_segment_cache(
            &payload.user_id,
            &payload.project_id,
            &seg.cache_key,
        );
        let local_seg = work_dir.path().join(format!("seg-{idx:04}.mp4"));

        // Cache lookup: check object storage first so interrupted/retried jobs & unedited scenes skip rendering.
        if let Some(obj) = storage.head(&seg_key).await? {
            if obj.size_bytes > 0 {
                storage.download(&seg_key, &local_seg).await?;
                segment_files.push(local_seg);
                cache_hits += 1;
                frames_done.fetch_add(seg.frame_count, Ordering::Relaxed);
                continue;
            }
        }

        // Lazy decode only the scene image(s) touched by this segment.
        let needed_scenes: Vec<usize> = match seg.kind {
            pace_engine::segments::SegmentKind::Body { scene_index } => vec![scene_index],
            pace_engine::segments::SegmentKind::Transition { boundary_index, .. } => {
                vec![boundary_index.saturating_sub(1), boundary_index]
            }
        };

        for scene_idx in needed_scenes {
            if decoded_sources.contains_key(&scene_idx) {
                continue;
            }
            let Some(storage_key) = asset_map.get(&scene_idx) else {
                continue;
            };
            let local_img = work_dir.path().join(format!("img-scene-{scene_idx}.bin"));
            storage.download(storage_key, &local_img).await?;
            let scene_meta = &spec.scenes[scene_idx];
            let spec_w = scene_meta.image_size.w;
            let spec_h = scene_meta.image_size.h;
            let src = tokio::task::spawn_blocking(move || {
                DecodedSceneSource::from_file(&local_img, spec_w, spec_h)
            })
            .await
            .map_err(|e| RenderError::ImageDecode(format!("decode task failed: {e}")))??;
            decoded_sources.insert(scene_idx, src);
        }

        let start_frame = seg.start_frame;
        let frame_count = seg.frame_count;
        let seg_span_progress = ((frame_count as usize * 75) / total_frames_all as usize) as i32;
        let width = payload.width;
        let height = payload.height;
        let fps = payload.fps;
        let local_seg_clone = local_seg.clone();
        let spec_clone = Arc::clone(&spec);
        let subs_clone = Arc::clone(&subtitles_arc);
        let style_clone = subtitle_style.clone();
        let stickers_clone = Arc::clone(&stickers_arc);
        let aspect_clone = aspect_ratio.clone();
        let seg_frames_counter = Arc::new(AtomicU32::new(0));
        let seg_frames_for_closure = Arc::clone(&seg_frames_counter);

        let mut owned_sources = std::mem::take(&mut decoded_sources);
        decoded_sources = run_blocking_with_heartbeat(
            pool,
            job.id,
            worker_id,
            &stage,
            base_progress,
            seg_span_progress.max(1),
            frame_count,
            seg_frames_counter,
            move || {
                let mut scratch = FrameRasterScratch::new();
                let mut sub_cache = SubtitleOverlayCache::new();
                encode_rgba_segment(
                    width,
                    height,
                    fps,
                    frame_count,
                    &local_seg_clone,
                    |local_f, buf| {
                        let global_f = start_frame + local_f;
                        let plan = frame_plan(&spec_clone, global_f);
                        rasterize_frame_plan_with_scratch(
                            &spec_clone,
                            &plan,
                            &mut owned_sources,
                            width,
                            height,
                            buf,
                            &mut scratch,
                        )?;
                        let active_stickers = if !stickers_clone.is_empty() {
                            evaluate_stickers_at(&stickers_clone, global_f, fps)
                        } else {
                            plan.stickers.clone()
                        };
                        if !active_stickers.is_empty() {
                            composite_frame_stickers(buf, width, height, &active_stickers);
                        }
                        if style_clone.enabled && !subs_clone.is_empty() {
                            if let Some(cue) =
                                active_subtitle_for_frame(&subs_clone, global_f, fps)
                            {
                                if let Some(overlay) = sub_cache.get_or_render(
                                    cue,
                                    &style_clone,
                                    &aspect_clone,
                                    width,
                                    height,
                                ) {
                                    composite_subtitle_overlay(buf, width, height, overlay);
                                }
                            }
                        }
                        seg_frames_for_closure.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    },
                )?;
                Ok(owned_sources)
            },
        )
        .await?;

        frames_done.fetch_add(frame_count, Ordering::Relaxed);

        let seg_size = std::fs::metadata(&local_seg)?.len() as i64;
        storage.upload(&local_seg, &seg_key, "video/mp4").await?;
        let _ = pace_db::upsert_render_segment(
            pool,
            &seg.cache_key,
            payload.project_id,
            &seg_key,
            seg.frame_count as i32,
            seg_size,
        )
        .await;

        segment_files.push(local_seg);
    }

    let _ = heartbeat_job(pool, job.id, worker_id, 86, Some("muxing_audio")).await;
    let local_audio = if let Some(ref audio_key) = payload.audio_key {
        let p = work_dir.path().join("narration.audio");
        storage.download(audio_key, &p).await?;
        Some(p)
    } else {
        None
    };

    let local_music = if let Some(ref music_key) = payload.music_key {
        let p = work_dir.path().join("music.audio");
        storage.download(music_key, &p).await?;
        Some(p)
    } else {
        None
    };

    let fps_u64 = payload.fps.max(1) as u64;
    let expected_ms = ((spec.total_frames as u64 * 1000) + (fps_u64 / 2)) / fps_u64;
    let music_cfg = payload.music_settings.clone();

    let final_mp4 = work_dir.path().join("final.mp4");
    let final_for_mux = final_mp4.clone();
    run_blocking_with_heartbeat(
        pool,
        job.id,
        worker_id,
        "muxing_audio",
        86,
        5,
        1,
        Arc::new(AtomicU32::new(0)),
        move || {
            concat_and_mix_audio(
                &segment_files,
                local_audio.as_deref(),
                local_music.as_deref(),
                music_cfg.as_ref(),
                expected_ms,
                &final_for_mux,
            )
        },
    )
    .await?;

    let _ = heartbeat_job(pool, job.id, worker_id, 93, Some("validating_export")).await;
    let allowed_black_ms: Vec<(u64, u64)> = spec
        .transitions
        .iter()
        .filter(|tw| tw.kind == TransitionKind::FadeBlack && tw.duration_frames > 0)
        .map(|tw| {
            let s = (tw.start_frame as u64 * 1000) / (payload.fps.max(1) as u64);
            let e = (tw.end_frame as u64 * 1000) / (payload.fps.max(1) as u64);
            (s, e)
        })
        .collect();

    let width = payload.width;
    let height = payload.height;
    let final_for_validate = final_mp4.clone();
    let report = run_blocking_with_heartbeat(
        pool,
        job.id,
        worker_id,
        "validating_export",
        93,
        3,
        1,
        Arc::new(AtomicU32::new(0)),
        move || {
            validate_export_mp4(
                &final_for_validate,
                width,
                height,
                expected_ms,
                &allowed_black_ms,
            )
        },
    )
    .await?;

    let _ = heartbeat_job(pool, job.id, worker_id, 97, Some("uploading_mp4")).await;
    storage
        .upload(&final_mp4, &payload.output_key, "video/mp4")
        .await?;

    let validation_json = serde_json::to_value(&report).unwrap_or_else(|_| serde_json::json!({}));
    pace_db::mark_export_succeeded(
        pool,
        payload.export_id,
        &payload.output_key,
        report.size_bytes as i64,
        validation_json.clone(),
    )
    .await?;

    Ok(serde_json::json!({
        "export_id": payload.export_id,
        "output_key": payload.output_key,
        "size_bytes": report.size_bytes,
        "duration_ms": report.duration_ms,
        "segments_total": segments.len(),
        "segments_cached": cache_hits,
        "validation": validation_json,
    }))
}

pub async fn run_render_worker_loop() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pace_render=info".into()),
        )
        .init();

    let cfg = RenderConfig::from_env().expect("invalid render worker config");
    let pool = create_pool(&cfg.database_url)
        .await
        .expect("failed to connect render worker to PostgreSQL");
    run_migrations(&pool)
        .await
        .expect("failed to verify migrations");

    let storage: Arc<dyn Storage> = Arc::new(S3Storage::new(S3Config {
        endpoint: cfg.s3_endpoint.clone(),
        bucket: cfg.s3_bucket.clone(),
        region: cfg.s3_region.clone(),
        access_key_id: cfg.s3_access_key.clone(),
        secret_access_key: cfg.s3_secret_key.clone(),
        path_style: true,
    }));

    let worker_id = format!("render-{}", uuid::Uuid::now_v7());
    tracing::info!(
        worker_id = %worker_id,
        "PACE Rust render worker listening on queue='render'"
    );

    loop {
        match claim_next_job(&pool, JobQueue::Render, &worker_id).await {
            Ok(Some(job)) => {
                tracing::info!(job_id = %job.id, attempt = job.attempts, "Claimed render job");
                let export_id = serde_json::from_value::<RenderJobPayload>(job.payload.clone())
                    .map(|p| p.export_id)
                    .ok();
                match process_render_job(&pool, storage.as_ref(), &job, &worker_id).await {
                    Ok(res) => {
                        let _ = complete_job(&pool, job.id, &worker_id, Some(res)).await;
                        tracing::info!(job_id = %job.id, "Completed render job");
                    }
                    Err(err) => {
                        let permanent = matches!(
                            err,
                            RenderError::InvalidPayload(_) | RenderError::Validation { .. }
                        );
                        let msg = err.to_string();
                        tracing::error!(job_id = %job.id, error = %msg, permanent, "Render job failed");
                        let _ = fail_job(&pool, job.id, &worker_id, "RENDER_FAILED", &msg, permanent).await;
                        if let Some(eid) = export_id {
                            if permanent || job.attempts >= job.max_attempts {
                                let _ = pace_db::mark_export_failed(&pool, eid, "RENDER_FAILED", &msg).await;
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                sleep(Duration::from_millis(cfg.poll_interval_ms)).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "Error polling render queue");
                sleep(Duration::from_millis(cfg.poll_interval_ms * 2)).await;
            }
        }
    }
}
