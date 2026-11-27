//! TDD suite for ADR-004: CapCut-grade camera motion, velocity curves,
//! real-time handheld drift, and spatial motion transitions.

use pace_engine::fit::{calculate_fill_crop, calculate_fit_layers, FitMode, LayerKind, SizeF};
use pace_engine::motion::{
    build_motion, build_motion_with_effect, cam_state_at, Easing, FocalPoint, Intensity,
    MotionPreset, SceneVisualEffect, Shake,
};
use pace_engine::spec::{
    compile_spec, encode_frame_plan_flat, frame_plan, CompileSpecInput, SceneEffectSettings,
    SceneSpecInput, FLAT_HEADER_LEN, FLAT_LAYER_STRIDE,
};
use pace_engine::effects::{assign_automatic_effects_with_options, SceneEffectInput};
use pace_engine::timing::{Boundary, TransitionKind};

#[test]
fn short_3s_scene_has_visible_zoom_and_distinct_subtle_normal_strong() {
    let focal = FocalPoint { x: 0.5, y: 0.5 };
    let subtle = build_motion(
        MotionPreset::ZoomIn,
        focal,
        3_000,
        Intensity::Subtle,
        Easing::EaseInOut,
        None,
    );
    let normal = build_motion(
        MotionPreset::ZoomIn,
        focal,
        3_000,
        Intensity::Normal,
        Easing::EaseInOut,
        None,
    );
    let strong = build_motion(
        MotionPreset::ZoomIn,
        focal,
        3_000,
        Intensity::Strong,
        Easing::EaseInOut,
        None,
    );

    let subtle_delta = subtle.to.zoom - subtle.from.zoom;
    let normal_delta = normal.to.zoom - normal.from.zoom;
    let strong_delta = strong.to.zoom - strong.from.zoom;

    assert!(
        normal_delta >= 0.20,
        "Expected >= 20% zoom on a 3s Normal scene, got {normal_delta}"
    );
    assert!(
        subtle_delta < normal_delta * 0.8,
        "Expected Subtle ({subtle_delta}) < 0.8 * Normal ({normal_delta})"
    );
    assert!(
        strong_delta > normal_delta * 1.3,
        "Expected Strong ({strong_delta}) > 1.3 * Normal ({normal_delta})"
    );
}

#[test]
fn strong_pan_scales_zoom_headroom_and_never_freezes_against_crop_edges() {
    let img = SizeF::new(1920.0, 1080.0);
    let frame = SizeF::new(1920.0, 1080.0);
    let focal = FocalPoint { x: 0.5, y: 0.5 };

    let normal_pan = build_motion(
        MotionPreset::PanRight,
        focal,
        4_000,
        Intensity::Normal,
        Easing::Steady,
        None,
    );
    let strong_pan = build_motion(
        MotionPreset::PanRight,
        focal,
        4_000,
        Intensity::Strong,
        Easing::Steady,
        None,
    );

    let c0 = calculate_fill_crop(img, frame, focal, cam_state_at(&strong_pan, 0.0));
    let c1 = calculate_fill_crop(img, frame, focal, cam_state_at(&strong_pan, 0.15));
    let c2 = calculate_fill_crop(img, frame, focal, cam_state_at(&strong_pan, 0.85));
    let c3 = calculate_fill_crop(img, frame, focal, cam_state_at(&strong_pan, 1.0));

    // Must never freeze against left/right clamp walls during initial or final 15%
    assert!(
        c1.x > c0.x + 15.0,
        "Strong pan froze at start: c0.x={} c1.x={}",
        c0.x,
        c1.x
    );
    assert!(
        c3.x > c2.x + 15.0,
        "Strong pan froze at end: c2.x={} c3.x={}",
        c2.x,
        c3.x
    );

    let normal_start = calculate_fill_crop(img, frame, focal, cam_state_at(&normal_pan, 0.0));
    let normal_end = calculate_fill_crop(img, frame, focal, cam_state_at(&normal_pan, 1.0));
    assert!(
        (c3.x - c0.x) > (normal_end.x - normal_start.x) * 1.2,
        "Strong pan pixel travel must exceed Normal pan travel"
    );
}

#[test]
fn punch_diagonal_and_handheld_presets_and_velocity_curves_work() {
    let focal = FocalPoint { x: 0.5, y: 0.5 };

    // PunchOut bursts > 65% of motion in the first 30% of the clip
    let punch = build_motion(
        MotionPreset::PunchZoomIn,
        focal,
        3_500,
        Intensity::Normal,
        Easing::PunchOut,
        None,
    );
    let p0 = cam_state_at(&punch, 0.0).zoom;
    let p30 = cam_state_at(&punch, 0.30).zoom;
    let p100 = cam_state_at(&punch, 1.0).zoom;
    let ratio = (p30 - p0) / (p100 - p0);
    assert!(
        ratio > 0.65,
        "PunchOut should deliver >65% of zoom by t=0.30, got {ratio}"
    );
    assert!(p100 >= 1.30, "PunchZoomIn should reach >= 1.30x, got {p100}");

    // RampIn accelerates into the cut
    assert!(Easing::RampIn.apply(0.3) < 0.10);
    assert!(Easing::RampIn.apply(0.8) > 0.50);

    // KenBurnsDiagonalIn moves on BOTH X and Y while zooming in
    let diag = build_motion(
        MotionPreset::KenBurnsDiagonalIn,
        FocalPoint { x: 0.35, y: 0.40 },
        5_000,
        Intensity::Normal,
        Easing::EaseInOut,
        None,
    );
    assert!(diag.to.zoom > diag.from.zoom + 0.14);
    assert!((diag.to.cx - diag.from.cx).abs() > 0.03);
    assert!((diag.to.cy - diag.from.cy).abs() > 0.03);

    // HandheldDrift produces organic movement even when Shake=None was passed
    let handheld = build_motion(
        MotionPreset::HandheldDrift,
        focal,
        6_000,
        Intensity::Normal,
        Easing::EaseInOut,
        None,
    );
    let h_a = cam_state_at(&handheld, 0.15);
    let h_b = cam_state_at(&handheld, 0.35);
    assert!(
        (h_a.cx - h_b.cx).abs() > 0.001 || (h_a.cy - h_b.cy).abs() > 0.001,
        "HandheldDrift must exhibit organic camera float"
    );
}

#[test]
fn shake_oscillates_in_real_seconds_not_normalized_scene_progress() {
    let focal = FocalPoint { x: 0.5, y: 0.5 };
    let motion = build_motion(
        MotionPreset::Still,
        focal,
        20_000, // 20-second scene
        Intensity::Normal,
        Easing::Steady,
        Some(Shake::default()),
    );

    // Sample 0.25 seconds apart (p = 0.050 -> 1.00s vs p = 0.0625 -> 1.25s)
    let s1 = cam_state_at(&motion, 0.050);
    let s2 = cam_state_at(&motion, 0.0625);
    let displacement = ((s2.cx - s1.cx).powi(2) + (s2.cy - s1.cy).powi(2)).sqrt();
    assert!(
        displacement > 0.002,
        "Expected real-time sub-second handheld displacement on a 20s scene, got {displacement}"
    );
}

#[test]
fn motion_transitions_whip_pan_slide_and_zoom_through_produce_seamless_layers() {
    let make_spec = |kind: TransitionKind| {
        compile_spec(&CompileSpecInput {
            fps: 30,
            frame_size: SizeF::new(1920.0, 1080.0),
            narration_ms: 8_000,
            effect_seed: 7,
            include_tilt_in_auto: false,
            boundaries: vec![
                Boundary::new(0),
                Boundary::with_transition(4_000, kind, 600),
                Boundary::new(8_000),
            ],
            stickers: Vec::new(),
            scenes: vec![
                SceneSpecInput {
                    scene_index: 0,
                    section_id: "s0".to_string(),
                    media_id: Some("m0".to_string()),
                    image_size: SizeF::new(1920.0, 1080.0),
                    fit_mode: FitMode::Fill,
                    focal: FocalPoint::default(),
                    effect: SceneEffectSettings::default(),
                    effect_overridden: false,
                },
                SceneSpecInput {
                    scene_index: 1,
                    section_id: "s1".to_string(),
                    media_id: Some("m1".to_string()),
                    image_size: SizeF::new(1920.0, 1080.0),
                    fit_mode: FitMode::Fill,
                    focal: FocalPoint::default(),
                    effect: SceneEffectSettings::default(),
                    effect_overridden: false,
                },
            ],
        })
        .expect("compile motion transition spec")
    };

    // 1. WhipPanLeft: outgoing moves left (< 0), incoming enters seamlessly from right (> 0)
    let whip_spec = make_spec(TransitionKind::WhipPanLeft);
    let mid_whip = frame_plan(&whip_spec, 120);
    let imgs: Vec<_> = mid_whip
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Image)
        .collect();
    assert_eq!(imgs.len(), 2);
    assert!(imgs[0].dst.x < -100.0);
    assert!(imgs[1].dst.x > 100.0);
    assert!(
        ((imgs[1].dst.x - imgs[0].dst.x) - 1920.0).abs() < 1e-2,
        "WhipPanLeft slides must abut seamlessly with zero gap"
    );

    // 2. SlideUp: outgoing moves up (< 0), incoming enters from bottom (> 0)
    let slide_spec = make_spec(TransitionKind::SlideUp);
    let mid_slide = frame_plan(&slide_spec, 120);
    let slide_imgs: Vec<_> = mid_slide
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Image)
        .collect();
    assert_eq!(slide_imgs.len(), 2);
    assert!(slide_imgs[0].dst.y < -50.0);
    assert!(slide_imgs[1].dst.y > 50.0);
    assert!(((slide_imgs[1].dst.y - slide_imgs[0].dst.y) - 1080.0).abs() < 1e-2);

    // 3. ZoomThrough: outgoing zooms deeper (smaller src crop) while incoming blends over with source-over
    let zt_spec = make_spec(TransitionKind::ZoomThrough);
    let before_zt = frame_plan(&zt_spec, 105);
    let mid_zt = frame_plan(&zt_spec, 120);
    let zt_imgs: Vec<_> = mid_zt
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Image)
        .collect();
    assert_eq!(zt_imgs.len(), 2);
    assert!(
        zt_imgs[0].src.w < before_zt.layers[0].src.w * 0.90,
        "ZoomThrough outgoing scene must punch-zoom deeper into image"
    );
    assert!((zt_imgs[0].opacity - 1.0).abs() < 1e-5);
    assert!(zt_imgs[1].opacity > 0.2 && zt_imgs[1].opacity < 0.8);
}

#[test]
fn tilted_presets_visual_effects_and_shutter_flash_work_end_to_end() {
    // 1. Auto-assign excludes tilted presets by default, includes them when include_tilt = true
    let inputs: Vec<SceneEffectInput> = (0..40)
        .map(|i| SceneEffectInput::new(i, 4_000, FocalPoint::default()))
        .collect();
    let auto_no_tilt = assign_automatic_effects_with_options(&inputs, 77, false);
    assert!(auto_no_tilt.iter().all(|a| !matches!(
        a.preset,
        MotionPreset::Pendulum
            | MotionPreset::Wobble
            | MotionPreset::FallLeft
            | MotionPreset::FallRight
    )));

    let auto_with_tilt = assign_automatic_effects_with_options(&inputs, 77, true);
    assert!(auto_with_tilt.iter().any(|a| matches!(
        a.preset,
        MotionPreset::Pendulum
            | MotionPreset::Wobble
            | MotionPreset::FallLeft
            | MotionPreset::FallRight
    )));

    // 2. Rotated Fill layers scale dst up to prevent black corners
    let m_fall = build_motion_with_effect(
        MotionPreset::FallLeft,
        FocalPoint::default(),
        4_000,
        Intensity::Normal,
        Easing::EaseInOut,
        None,
        SceneVisualEffect::None,
    );
    let c_end = cam_state_at(&m_fall, 1.0);
    assert!(c_end.rotation_rad < -0.05);
    let layers = calculate_fit_layers(
        FitMode::Fill,
        SizeF::new(1920.0, 1080.0),
        SizeF::new(1920.0, 1080.0),
        FocalPoint::default(),
        c_end,
    );
    assert_eq!(layers.len(), 1);
    assert!(layers[0].dst.w > 1920.0);
    assert!(layers[0].dst.h > 1080.0);
    assert!(layers[0].dst.x < 0.0);
    assert!(layers[0].dst.y < 0.0);

    // 3. CameraShake & ZoomLens visual effects
    let shake_motion = build_motion_with_effect(
        MotionPreset::Still,
        FocalPoint::default(),
        4_000,
        Intensity::Normal,
        Easing::Steady,
        None,
        SceneVisualEffect::CameraShake,
    );
    let s1 = cam_state_at(&shake_motion, 0.25);
    let s2 = cam_state_at(&shake_motion, 0.31);
    assert!((s1.cx - s2.cx).abs() > 1e-4 || (s1.rotation_rad - s2.rotation_rad).abs() > 1e-4);

    let lens_motion = build_motion_with_effect(
        MotionPreset::Still,
        FocalPoint::default(),
        4_000,
        Intensity::Normal,
        Easing::Steady,
        None,
        SceneVisualEffect::ZoomLens,
    );
    let z0 = cam_state_at(&lens_motion, 0.0).zoom;
    let z_peak = cam_state_at(&lens_motion, 0.1).zoom;
    assert!(z_peak > z0);

    // 4. ShutterFlash & SlideDown + flat encoding header len == 4
    let spec = compile_spec(&CompileSpecInput {
        fps: 30,
        frame_size: SizeF::new(1920.0, 1080.0),
        narration_ms: 8_000,
        effect_seed: 1,
        include_tilt_in_auto: false,
        boundaries: vec![
            Boundary::new(0),
            Boundary::with_transition(4_000, TransitionKind::ShutterFlash, 400),
            Boundary::new(8_000),
        ],
        stickers: Vec::new(),
        scenes: vec![
            SceneSpecInput {
                scene_index: 0,
                section_id: "s0".to_string(),
                media_id: Some("m0".to_string()),
                image_size: SizeF::new(1920.0, 1080.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings {
                    manual_preset: Some(MotionPreset::Pendulum),
                    ..Default::default()
                },
                effect_overridden: true,
            },
            SceneSpecInput {
                scene_index: 1,
                section_id: "s1".to_string(),
                media_id: Some("m1".to_string()),
                image_size: SizeF::new(1920.0, 1080.0),
                fit_mode: FitMode::Fill,
                focal: FocalPoint::default(),
                effect: SceneEffectSettings::default(),
                effect_overridden: false,
            },
        ],
    })
    .expect("compile shutter flash spec");

    let flash_plan = frame_plan(&spec, 120);
    assert!(flash_plan.flash_white_alpha > 0.8);
    let flat = encode_frame_plan_flat(&flash_plan);
    assert_eq!(flat.len(), FLAT_HEADER_LEN + flash_plan.layers.len() * FLAT_LAYER_STRIDE);
    assert!((flat[3] - flash_plan.flash_white_alpha).abs() < 1e-6);
}
