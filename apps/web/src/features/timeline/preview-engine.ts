import type {
  BoundaryTransitionDto,
  MotionPreset,
  SceneEffectDto,
  StickerKind,
  StickerPlacementSpec,
  TransitionKind,
} from "@pace/shared-types";

export interface PresetOption {
  id: MotionPreset;
  label: string;
  category: "static" | "punch" | "tilt" | "cinematic" | "zoom" | "pan" | "combo";
}

export const MOTION_PRESETS: PresetOption[] = [
  { id: "still", label: "Still (Subtle Hold)", category: "static" },
  { id: "punch_zoom_in", label: "Zoom 1 (Punch In)", category: "punch" },
  { id: "zoom_snap", label: "Zoom 2 (Snap)", category: "punch" },
  { id: "punch_zoom_out", label: "Punch Zoom Out", category: "punch" },
  { id: "pendulum", label: "Pendulum", category: "tilt" },
  { id: "wobble", label: "Wobble", category: "tilt" },
  { id: "fall_left", label: "Fall Left", category: "tilt" },
  { id: "fall_right", label: "Fall Right", category: "tilt" },
  { id: "ken_burns_diagonal_in", label: "Diagonal Drift In", category: "cinematic" },
  { id: "ken_burns_diagonal_out", label: "Diagonal Drift Out", category: "cinematic" },
  { id: "handheld_drift", label: "Handheld Float", category: "cinematic" },
  { id: "zoom_in", label: "Zoom In", category: "zoom" },
  { id: "zoom_out", label: "Zoom Out", category: "zoom" },
  { id: "pan_left", label: "Pan Left", category: "pan" },
  { id: "pan_right", label: "Pan Right", category: "pan" },
  { id: "pan_up", label: "Pan Up", category: "pan" },
  { id: "pan_down", label: "Pan Down", category: "pan" },
  { id: "zoom_in_pan_left", label: "Zoom In + Pan Left", category: "combo" },
  { id: "zoom_in_pan_right", label: "Zoom In + Pan Right", category: "combo" },
  { id: "zoom_in_pan_up", label: "Zoom In + Pan Up", category: "combo" },
  { id: "zoom_in_pan_down", label: "Zoom In + Pan Down", category: "combo" },
  { id: "zoom_out_pan_left", label: "Zoom Out + Pan Left", category: "combo" },
  { id: "zoom_out_pan_right", label: "Zoom Out + Pan Right", category: "combo" },
  { id: "zoom_out_pan_up", label: "Zoom Out + Pan Up", category: "combo" },
  { id: "zoom_out_pan_down", label: "Zoom Out + Pan Down", category: "combo" },
];

const PRESET_LABEL_MAP: Record<string, string> = Object.fromEntries(
  MOTION_PRESETS.map((p) => [p.id, p.label])
);

export function formatPresetLabel(preset: string | null | undefined): string {
  if (!preset) return "Auto";
  return PRESET_LABEL_MAP[preset] ?? preset.replace(/_/g, " ");
}

export interface RectF {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type FrameLayerKind = "image" | "blurred_background";

export interface DecodedFrameLayer {
  sceneIndex: number;
  kind: FrameLayerKind;
  src: RectF;
  dst: RectF;
  opacity: number;
  rotationRad: number;
}

export interface DecodedFramePlan {
  fadeBlackAlpha: number;
  flashWhiteAlpha: number;
  primarySceneIndex: number;
  layers: DecodedFrameLayer[];
}

export interface DecodedFrameSticker {
  kind: StickerKind;
  x: number;
  y: number;
  scale: number;
  opacity: number;
  rotationRad: number;
}

const FLAT_HEADER_LEN = 4;
const FLAT_LAYER_STRIDE = 12;
const FLAT_STICKER_STRIDE = 6;
const STICKER_KINDS: StickerKind[] = [
  "subscribe",
  "like",
  "comment",
  "bell",
  "follow",
];

/**
 * Decodes the compact Float32Array returned by `WasmRenderEngine.frame_plan_flat(frameIndex)`.
 * Layout: `[fade_black_alpha, primary_scene_index, layer_count, flash_white_alpha, ...layers(stride=12)]`.
 */
export function decodeFramePlanFlat(flat: ArrayLike<number>): DecodedFramePlan {
  if (!flat || flat.length < FLAT_HEADER_LEN) {
    return {
      fadeBlackAlpha: 0,
      flashWhiteAlpha: 0,
      primarySceneIndex: 0,
      layers: [],
    };
  }

  const fadeBlackAlpha = Math.max(0, Math.min(1, Number(flat[0]) || 0));
  const primarySceneIndex = Math.max(0, Math.round(Number(flat[1]) || 0));
  const declaredLayers = Math.max(0, Math.round(Number(flat[2]) || 0));
  const flashWhiteAlpha = Math.max(0, Math.min(1, Number(flat[3]) || 0));
  const maxPossibleLayers = Math.floor(
    (flat.length - FLAT_HEADER_LEN) / FLAT_LAYER_STRIDE
  );
  const layerCount = Math.min(declaredLayers, maxPossibleLayers);

  const layers: DecodedFrameLayer[] = [];
  for (let i = 0; i < layerCount; i++) {
    const base = FLAT_HEADER_LEN + i * FLAT_LAYER_STRIDE;
    const kindCode = Math.round(Number(flat[base + 1]) || 0);
    layers.push({
      sceneIndex: Math.max(0, Math.round(Number(flat[base]) || 0)),
      kind: kindCode === 1 ? "blurred_background" : "image",
      src: {
        x: Number(flat[base + 2]) || 0,
        y: Number(flat[base + 3]) || 0,
        w: Number(flat[base + 4]) || 0,
        h: Number(flat[base + 5]) || 0,
      },
      dst: {
        x: Number(flat[base + 6]) || 0,
        y: Number(flat[base + 7]) || 0,
        w: Number(flat[base + 8]) || 0,
        h: Number(flat[base + 9]) || 0,
      },
      opacity: Math.max(0, Math.min(1, Number(flat[base + 10]) || 0)),
      rotationRad: Number(flat[base + 11]) || 0,
    });
  }

  return {
    fadeBlackAlpha,
    flashWhiteAlpha,
    primarySceneIndex,
    layers,
  };
}

export function decodeStickerPlanFlat(
  flat: ArrayLike<number>
): DecodedFrameSticker[] {
  if (!flat || flat.length < 1) return [];
  const declared = Math.max(0, Math.round(Number(flat[0]) || 0));
  const maxPossible = Math.floor((flat.length - 1) / FLAT_STICKER_STRIDE);
  const count = Math.min(declared, maxPossible);
  const out: DecodedFrameSticker[] = [];
  for (let i = 0; i < count; i++) {
    const base = 1 + i * FLAT_STICKER_STRIDE;
    const kindCode = Math.round(Number(flat[base]) || 0);
    const kind = STICKER_KINDS[kindCode] ?? "subscribe";
    out.push({
      kind,
      x: Number(flat[base + 1]) || 0.5,
      y: Number(flat[base + 2]) || 0.82,
      scale: Number(flat[base + 3]) || 1,
      opacity: Math.max(0, Math.min(1, Number(flat[base + 4]) || 0)),
      rotationRad: Number(flat[base + 5]) || 0,
    });
  }
  return out;
}

/**
 * Converts playback timestamp in milliseconds to a clamped 0-based frame index.
 */
export function timeMsToFrameIndex(
  timeMs: number,
  fps: number,
  totalFrames: number
): number {
  if (totalFrames <= 0 || fps <= 0) return 0;
  const raw = Math.floor((Math.max(0, timeMs) / 1000) * fps);
  return Math.max(0, Math.min(totalFrames - 1, raw));
}

/**
 * Returns scene indices ordered by prefetch priority: current, upcoming lookahead, and previous.
 */
export function getPrefetchSceneIndices(
  currentSceneIndex: number,
  totalScenes: number,
  lookahead = 3
): number[] {
  if (totalScenes <= 0) return [];
  const clamped = Math.max(0, Math.min(totalScenes - 1, currentSceneIndex));
  const seen = new Set<number>();
  const order: number[] = [];

  const push = (idx: number) => {
    if (idx >= 0 && idx < totalScenes && !seen.has(idx)) {
      seen.add(idx);
      order.push(idx);
    }
  };

  push(clamped);
  for (let offset = 1; offset <= lookahead; offset++) {
    push(clamped + offset);
  }
  push(clamped - 1);

  return order;
}

const ACTIVE_TRANSITION_KINDS = new Set([
  "crossfade",
  "fade_black",
  "shutter_flash",
  "zoom_through",
  "whip_pan_left",
  "whip_pan_right",
  "slide_left",
  "slide_right",
  "slide_up",
  "slide_down",
]);

/**
 * Normalizes a boundary transition payload to match backend and Rust engine invariants.
 */
export function normalizeBoundaryTransition(
  kind: TransitionKind | string,
  durationMs: number
): BoundaryTransitionDto {
  if (ACTIVE_TRANSITION_KINDS.has(kind)) {
    const fallbackMs =
      kind === "whip_pan_left" ||
      kind === "whip_pan_right" ||
      kind === "slide_left" ||
      kind === "slide_right" ||
      kind === "slide_up" ||
      kind === "slide_down" ||
      kind === "zoom_through"
        ? 450
        : kind === "shutter_flash"
          ? 350
          : 500;
    const clamped = Math.max(100, Math.min(3000, Math.round(durationMs || fallbackMs)));
    return { kind: kind as TransitionKind, duration_ms: clamped };
  }
  return { kind: "cut", duration_ms: 0 };
}

export function parseBoundaryTransition(
  raw: Record<string, unknown> | null | undefined
): BoundaryTransitionDto {
  if (!raw || typeof raw !== "object") {
    return { kind: "cut", duration_ms: 0 };
  }
  const kind = typeof raw.kind === "string" ? raw.kind : "cut";
  const durationMs =
    typeof raw.duration_ms === "number" ? raw.duration_ms : 500;
  return normalizeBoundaryTransition(kind, durationMs);
}

export interface LocalCompileSectionInput {
  id: string;
  position?: number;
  text: string;
}

export interface LocalCompileBoundaryInput {
  index: number;
  timeMs: number;
  transition?: Record<string, unknown> | BoundaryTransitionDto | null;
}

export interface LocalCompileAssignmentInput {
  sectionId: string;
  mediaId: string | null;
  fitMode: "fill" | "fit_blur";
  focalX: number;
  focalY: number;
  effect?: SceneEffectDto;
  effectOverridden?: boolean;
}

export interface LocalCompileParams {
  fps: number;
  aspectRatio: "16:9" | "9:16";
  durationMs: number;
  effectSeed: number;
  includeTiltInAuto?: boolean;
  sections: LocalCompileSectionInput[];
  boundaries: LocalCompileBoundaryInput[];
  assignments: LocalCompileAssignmentInput[];
  mediaDimensions?: Record<string, { w: number; h: number }>;
  stickers?: StickerPlacementSpec[];
}

/**
 * Builds a `CompileSpecInput` JSON payload matching `pace_engine::spec::CompileSpecInput`
 * so the browser WASM engine can evaluate local edits instantly even offline.
 */
export function buildLocalCompileSpecInput(params: LocalCompileParams) {
  const frameSize =
    params.aspectRatio === "9:16"
      ? { w: 1080, h: 1920 }
      : { w: 1920, h: 1080 };
  const fps = Math.max(1, Math.round(params.fps || 30));
  const sceneCount = Math.max(1, params.sections.length);

  let rawBoundaries = [...params.boundaries].sort((a, b) => a.index - b.index);
  if (rawBoundaries.length !== sceneCount + 1) {
    const safeTotal = Math.max(sceneCount * 500, Math.round(params.durationMs || sceneCount * 12000));
    const slice = safeTotal / sceneCount;
    rawBoundaries = Array.from({ length: sceneCount + 1 }, (_, idx) => ({
      index: idx,
      timeMs: idx === sceneCount ? safeTotal : Math.round(idx * slice),
      transition: null,
    }));
  }

  const narrationMs = Math.max(
    sceneCount * 500,
    Math.round(rawBoundaries[rawBoundaries.length - 1]?.timeMs ?? params.durationMs ?? sceneCount * 12000)
  );

  const engineBoundaries = rawBoundaries.map((b, idx) => {
    const curMs =
      idx === 0
        ? 0
        : idx === rawBoundaries.length - 1
          ? narrationMs
          : Math.max(0, Math.min(narrationMs, Math.round(b.timeMs)));
    const nextMs =
      idx + 1 < rawBoundaries.length
        ? Math.round(rawBoundaries[idx + 1].timeMs)
        : narrationMs;
    const allowZero = idx < sceneCount && nextMs - curMs < 500;
    const transition =
      idx === 0 || idx === sceneCount || !b.transition
        ? null
        : parseBoundaryTransition(b.transition as Record<string, unknown>);

    return {
      time_ms: curMs,
      allow_zero_length: allowZero,
      transition: transition && transition.kind !== "cut" ? transition : null,
    };
  });

  const assignMap = new Map(params.assignments.map((a) => [a.sectionId, a]));

  const scenes = params.sections.map((sec, idx) => {
    const assign = assignMap.get(sec.id);
    const dims =
      assign?.mediaId && params.mediaDimensions?.[assign.mediaId]
        ? params.mediaDimensions[assign.mediaId]
        : frameSize;

    const eff = assign?.effect;
    const manualPreset = eff?.manual_preset || null;
    const intensity =
      eff?.intensity === "subtle" || eff?.intensity === "strong"
        ? eff.intensity
        : "normal";
    const easing =
      eff?.easing === "steady" ||
      eff?.easing === "punch_out" ||
      eff?.easing === "ramp_in" ||
      eff?.easing === "dynamic_curve"
        ? eff.easing
        : "ease_in_out";
    const manualShake =
      typeof eff?.manual_shake === "boolean" ? eff.manual_shake : null;
    const visualEffect =
      eff?.visual_effect === "handheld" ||
      eff?.visual_effect === "camera_shake" ||
      eff?.visual_effect === "zoom_lens"
        ? eff.visual_effect
        : "none";

    return {
      scene_index: idx,
      section_id: sec.id,
      media_id: assign?.mediaId ?? null,
      image_size: {
        w: Math.max(1, dims.w),
        h: Math.max(1, dims.h),
      },
      fit_mode: assign?.fitMode === "fit_blur" ? "fit_blur" : "fill",
      focal: {
        x: Math.max(0, Math.min(1, assign?.focalX ?? 0.5)),
        y: Math.max(0, Math.min(1, assign?.focalY ?? 0.5)),
      },
      effect: {
        manual_preset: manualPreset,
        intensity,
        easing,
        manual_shake: manualShake,
        visual_effect: visualEffect,
      },
      effect_overridden: Boolean(assign?.effectOverridden),
    };
  });

  return {
    fps,
    frame_size: frameSize,
    narration_ms: narrationMs,
    effect_seed: Math.max(0, Math.round(params.effectSeed || 42)),
    include_tilt_in_auto: Boolean(params.includeTiltInAuto),
    boundaries: engineBoundaries,
    scenes,
    stickers: params.stickers ?? [],
  };
}
