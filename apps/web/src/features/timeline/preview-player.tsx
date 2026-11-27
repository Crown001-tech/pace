"use client";

import React, { useEffect, useMemo, useRef, useState } from "react";
import { Crosshair, Film, Loader2 } from "lucide-react";
import initWasm, { WasmRenderEngine } from "@pace/engine-wasm";
import type {
  RenderSpecResponse,
  StickerKind,
  StickerPlacementSpec,
  SubtitleCueDto,
  SubtitleStyleDto,
} from "@pace/shared-types";
import type { MediaItem, SectionAssignment } from "@/features/media/media-store";
import type { TimelineBoundaryItem } from "./timeline-engine";
import {
  buildLocalCompileSpecInput,
  decodeFramePlanFlat,
  decodeStickerPlanFlat,
  getPrefetchSceneIndices,
  timeMsToFrameIndex,
} from "./preview-engine";
import {
  DEFAULT_SUBTITLE_STYLE,
  wrapSubtitlePreviewLines,
} from "./subtitles-music-panel";

export interface PreviewSpecMeta {
  specHash: string;
  totalFrames: number;
  resolvedPresets: Record<string, { preset: string; hasShake: boolean }>;
}

interface PreviewPlayerProps {
  aspectRatio: "16:9" | "9:16";
  fps: number;
  currentTimeMs: number;
  durationMs: number;
  effectSeed: number;
  sections: { id: string; position?: number; text: string }[];
  boundaries: TimelineBoundaryItem[];
  assignments: SectionAssignment[];
  media: MediaItem[];
  compiledRenderSpec: RenderSpecResponse | null;
  preferLocalSpec: boolean;
  activeSceneIndex: number;
  inspectedSceneIndex: number;
  activeSubtitle?: SubtitleCueDto;
  subtitleStyle?: SubtitleStyleDto;
  stickers?: StickerPlacementSpec[];
  includeTiltInAuto?: boolean;
  showSafeGrid: boolean;
  isPickingFocal: boolean;
  inspectedFocal: { x: number; y: number };
  onPickFocalPoint?: (focalX: number, focalY: number) => void;
  onSpecMetaChange?: (meta: PreviewSpecMeta) => void;
  isFullscreen?: boolean;
}

let wasmInitPromise: Promise<void> | null = null;

function ensureBrowserWasm(): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = initWasm({
      module_or_path: "/wasm/pace_engine_wasm_bg.wasm",
    })
      .then(() => undefined)
      .catch((err) => {
        wasmInitPromise = null;
        throw err;
      });
  }
  return wasmInitPromise;
}

export function PreviewPlayer({
  aspectRatio,
  fps,
  currentTimeMs,
  durationMs,
  effectSeed,
  sections,
  boundaries,
  assignments,
  media,
  compiledRenderSpec,
  preferLocalSpec,
  activeSceneIndex,
  inspectedSceneIndex,
  activeSubtitle,
  subtitleStyle = DEFAULT_SUBTITLE_STYLE,
  stickers = [],
  includeTiltInAuto = false,
  showSafeGrid,
  isPickingFocal,
  inspectedFocal,
  onPickFocalPoint,
  onSpecMetaChange,
  isFullscreen = false,
}: PreviewPlayerProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const engineRef = useRef<WasmRenderEngine | null>(null);
  const imageCacheRef = useRef<Map<string, HTMLImageElement>>(new Map());
  const stickerCacheRef = useRef<Map<StickerKind, HTMLImageElement>>(new Map());
  const failedUrlsRef = useRef<Set<string>>(new Set());

  const [wasmReady, setWasmReady] = useState(false);
  const [wasmError, setWasmError] = useState<string | null>(null);
  const [renderTick, setRenderTick] = useState(0);
  const [mediaDimensions, setMediaDimensions] = useState<
    Record<string, { w: number; h: number }>
  >(() => {
    const initial: Record<string, { w: number; h: number }> = {};
    for (const m of media) {
      if (m.width && m.height) {
        initial[m.id] = { w: m.width, h: m.height };
      }
    }
    return initial;
  });

  // Sync explicit media dimensions from store / API assets
  useEffect(() => {
    setMediaDimensions((prev) => {
      let changed = false;
      const next = { ...prev };
      for (const m of media) {
        if (m.width && m.height && !next[m.id]) {
          next[m.id] = { w: m.width, h: m.height };
          changed = true;
        }
      }
      if (compiledRenderSpec?.scene_assets) {
        for (const asset of compiledRenderSpec.scene_assets) {
          if (asset.media_id && asset.width > 0 && asset.height > 0 && !next[asset.media_id]) {
            next[asset.media_id] = { w: asset.width, h: asset.height };
            changed = true;
          }
        }
      }
      return changed ? next : prev;
    });
  }, [media, compiledRenderSpec]);

  // Initialize WASM module once
  useEffect(() => {
    let mounted = true;
    ensureBrowserWasm()
      .then(() => {
        if (mounted) {
          setWasmReady(true);
          setWasmError(null);
        }
      })
      .catch((err) => {
        if (mounted) {
          setWasmError(
            err instanceof Error ? err.message : "Failed to initialize preview engine"
          );
        }
      });
    return () => {
      mounted = false;
    };
  }, []);

  // Preload transparent sticker PNG assets once
  useEffect(() => {
    const kinds: StickerKind[] = ["subscribe", "like", "comment", "bell", "follow"];
    for (const kind of kinds) {
      if (stickerCacheRef.current.has(kind)) continue;
      const img = new Image();
      img.onload = () => {
        stickerCacheRef.current.set(kind, img);
        setRenderTick((t) => t + 1);
      };
      img.src = `/stickers/${kind}.png`;
    }
  }, []);

  // Resolve per-scene image URLs & media IDs
  const sceneMediaList = useMemo(() => {
    const mediaMap = new Map(media.map((m) => [m.id, m]));
    const assignMap = new Map(assignments.map((a) => [a.sectionId, a]));
    const assetBySection = new Map(
      (compiledRenderSpec?.scene_assets ?? []).map((a) => [a.section_id, a])
    );

    return sections.map((sec) => {
      const assign = assignMap.get(sec.id);
      const localMedia = assign?.mediaId ? mediaMap.get(assign.mediaId) : undefined;
      const serverAsset = assetBySection.get(sec.id);
      const url =
        localMedia?.previewUrl ||
        localMedia?.thumbUrl ||
        serverAsset?.preview_url ||
        serverAsset?.thumb_url ||
        null;
      return {
        sectionId: sec.id,
        mediaId: assign?.mediaId ?? serverAsset?.media_id ?? null,
        url,
      };
    });
  }, [sections, assignments, media, compiledRenderSpec]);

  // Prefetch current, upcoming, and previous scene images
  useEffect(() => {
    if (sceneMediaList.length === 0) return;

    const order = getPrefetchSceneIndices(
      activeSceneIndex,
      sceneMediaList.length,
      3
    );

    for (const idx of order) {
      const item = sceneMediaList[idx];
      if (!item?.url) continue;
      const url = item.url;
      if (imageCacheRef.current.has(url) || failedUrlsRef.current.has(url)) {
        continue;
      }

      const loadImg = (useCors: boolean) => {
        const img = new Image();
        if (useCors) {
          img.crossOrigin = "anonymous";
        }
        img.onload = () => {
          imageCacheRef.current.set(url, img);
          if (item.mediaId && img.naturalWidth > 0 && img.naturalHeight > 0) {
            setMediaDimensions((prev) => {
              const cur = prev[item.mediaId!];
              if (
                cur &&
                cur.w === img.naturalWidth &&
                cur.h === img.naturalHeight
              ) {
                return prev;
              }
              return {
                ...prev,
                [item.mediaId!]: {
                  w: img.naturalWidth,
                  h: img.naturalHeight,
                },
              };
            });
          }
          setRenderTick((t) => t + 1);
        };
        img.onerror = () => {
          if (useCors) {
            loadImg(false);
          } else {
            failedUrlsRef.current.add(url);
          }
        };
        img.src = url;
      };

      loadImg(true);
    }
  }, [activeSceneIndex, sceneMediaList]);

  // Compile or load RenderSpec into WasmRenderEngine
  useEffect(() => {
    if (!wasmReady || sections.length === 0) return;

    let nextEngine: WasmRenderEngine | null = null;
    try {
      const canUseServerSpec =
        !preferLocalSpec &&
        compiledRenderSpec?.spec &&
        compiledRenderSpec.spec.scenes.length === sections.length;

      if (canUseServerSpec && compiledRenderSpec) {
        nextEngine = WasmRenderEngine.from_compiled_spec(
          JSON.stringify(compiledRenderSpec.spec)
        );
      } else {
        const compileInput = buildLocalCompileSpecInput({
          fps,
          aspectRatio,
          durationMs,
          effectSeed,
          includeTiltInAuto,
          sections,
          boundaries,
          assignments,
          mediaDimensions,
          stickers,
        });
        nextEngine = new WasmRenderEngine(JSON.stringify(compileInput));
      }

      if (engineRef.current) {
        engineRef.current.free();
      }
      engineRef.current = nextEngine;

      if (onSpecMetaChange) {
        try {
          const parsed = JSON.parse(nextEngine.compiled_spec_json());
          const resolvedPresets: Record<
            string,
            { preset: string; hasShake: boolean }
          > = {};
          for (const sc of parsed.scenes ?? []) {
            if (sc.section_id) {
              resolvedPresets[sc.section_id] = {
                preset: sc.preset,
                hasShake: Boolean(sc.has_shake),
              };
            }
          }
          onSpecMetaChange({
            specHash: nextEngine.spec_hash,
            totalFrames: nextEngine.total_frames,
            resolvedPresets,
          });
        } catch {}
      }

      setRenderTick((t) => t + 1);
    } catch (err) {
      if (nextEngine) {
        nextEngine.free();
      }
      console.warn("WASM spec compilation warning:", err);
    }
  }, [
    wasmReady,
    preferLocalSpec,
    compiledRenderSpec,
    fps,
    aspectRatio,
    durationMs,
    effectSeed,
    includeTiltInAuto,
    sections,
    boundaries,
    assignments,
    mediaDimensions,
    stickers,
    onSpecMetaChange,
  ]);

  // Free WASM memory on unmount
  useEffect(() => {
    return () => {
      if (engineRef.current) {
        engineRef.current.free();
        engineRef.current = null;
      }
    };
  }, []);

  // Render current frame plan onto Canvas 2D
  useEffect(() => {
    const canvas = canvasRef.current;
    const engine = engineRef.current;
    if (!canvas || !engine) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const specWidth = engine.frame_width || (aspectRatio === "9:16" ? 1080 : 1920);
    const specHeight = engine.frame_height || (aspectRatio === "9:16" ? 1920 : 1080);

    // Render internally at true 720p logical resolution for crisp subpixel motion
    const targetW = aspectRatio === "9:16" ? 720 : 1280;
    const targetH = aspectRatio === "9:16" ? 1280 : 720;
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }

    const scaleX = targetW / specWidth;
    const scaleY = targetH / specHeight;

    const frameIdx = timeMsToFrameIndex(
      currentTimeMs,
      engine.fps || fps,
      engine.total_frames
    );
    const flat = engine.frame_plan_flat(frameIdx);
    const plan = decodeFramePlanFlat(flat);

    const firstSceneIdx = plan.layers[0]?.sceneIndex;
    const hasOverlappingSecondScene = plan.layers.some(
      (l) => l.sceneIndex !== firstSceneIdx && l.opacity < 0.999
    );

    ctx.save();
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.fillStyle = "#09090b";
    ctx.fillRect(0, 0, targetW, targetH);

    for (const layer of plan.layers) {
      const sceneMedia = sceneMediaList[layer.sceneIndex];
      const img = sceneMedia?.url
        ? imageCacheRef.current.get(sceneMedia.url)
        : undefined;
      if (!img || !img.complete || img.naturalWidth <= 0) {
        continue;
      }

      // Map spec-reported image coordinates to loaded image pixel coordinates
      const specSceneDims =
        sceneMedia?.mediaId && mediaDimensions[sceneMedia.mediaId]
          ? mediaDimensions[sceneMedia.mediaId]
          : { w: img.naturalWidth, h: img.naturalHeight };
      const srcScaleX = img.naturalWidth / Math.max(1, specSceneDims.w);
      const srcScaleY = img.naturalHeight / Math.max(1, specSceneDims.h);

      const sx = Math.max(0, layer.src.x * srcScaleX);
      const sy = Math.max(0, layer.src.y * srcScaleY);
      const sw = Math.min(img.naturalWidth - sx, Math.max(1, layer.src.w * srcScaleX));
      const sh = Math.min(img.naturalHeight - sy, Math.max(1, layer.src.h * srcScaleY));

      const dx = layer.dst.x * scaleX;
      const dy = layer.dst.y * scaleY;
      const dw = layer.dst.w * scaleX;
      const dh = layer.dst.h * scaleY;

      // Preserve 100% brightness during overlapping crossfades under Canvas source-over compositing
      const effectiveAlpha =
        hasOverlappingSecondScene && layer.sceneIndex === firstSceneIdx
          ? 1.0
          : layer.opacity;

      ctx.save();
      ctx.globalAlpha = effectiveAlpha;

      if (layer.kind === "blurred_background") {
        ctx.beginPath();
        ctx.rect(dx, dy, dw, dh);
        ctx.clip();
        ctx.filter = "blur(20px)";
        const bleed = 28;
        ctx.drawImage(
          img,
          sx,
          sy,
          sw,
          sh,
          dx - bleed,
          dy - bleed,
          dw + bleed * 2,
          dh + bleed * 2
        );
      } else if (Math.abs(layer.rotationRad) > 1e-5) {
        const bgLayer = plan.layers.find(
          (l) => l.sceneIndex === layer.sceneIndex && l.kind === "blurred_background"
        );
        const clipX = bgLayer ? bgLayer.dst.x * scaleX : dx + dw * 0.5 - targetW * 0.5;
        const clipY = bgLayer ? bgLayer.dst.y * scaleY : dy + dh * 0.5 - targetH * 0.5;
        ctx.beginPath();
        ctx.rect(clipX, clipY, targetW, targetH);
        ctx.clip();
        ctx.translate(dx + dw * 0.5, dy + dh * 0.5);
        ctx.rotate(layer.rotationRad);
        ctx.drawImage(img, sx, sy, sw, sh, -dw * 0.5, -dh * 0.5, dw, dh);
      } else {
        ctx.drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh);
      }
      ctx.restore();
    }

    if (plan.fadeBlackAlpha > 0.001) {
      ctx.fillStyle = `rgba(0, 0, 0, ${plan.fadeBlackAlpha})`;
      ctx.fillRect(0, 0, targetW, targetH);
    }

    if (plan.flashWhiteAlpha > 0.001) {
      ctx.fillStyle = `rgba(255, 255, 255, ${plan.flashWhiteAlpha})`;
      ctx.fillRect(0, 0, targetW, targetH);
    }

    if (typeof engine.sticker_plan_flat === "function") {
      const activeStickers = decodeStickerPlanFlat(engine.sticker_plan_flat(frameIdx));
      const baseUnit = Math.min(targetW, targetH) * 0.34;
      for (const st of activeStickers) {
        const stImg = stickerCacheRef.current.get(st.kind);
        if (!stImg || !stImg.complete || stImg.naturalWidth <= 0 || st.opacity <= 0.001) {
          continue;
        }
        const drawW = baseUnit * st.scale;
        const drawH = drawW * (stImg.naturalHeight / stImg.naturalWidth);
        ctx.save();
        ctx.globalAlpha = st.opacity;
        ctx.translate(st.x * targetW, st.y * targetH);
        if (Math.abs(st.rotationRad) > 1e-5) {
          ctx.rotate(st.rotationRad);
        }
        ctx.drawImage(stImg, -drawW * 0.5, -drawH * 0.5, drawW, drawH);
        ctx.restore();
      }
    }

    ctx.restore();
  }, [
    currentTimeMs,
    fps,
    aspectRatio,
    sceneMediaList,
    mediaDimensions,
    renderTick,
  ]);

  const activeSceneMedia = sceneMediaList[activeSceneIndex];
  const activeImgLoaded = Boolean(
    activeSceneMedia?.url && imageCacheRef.current.get(activeSceneMedia.url)
  );

  const handleCanvasClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isPickingFocal || !onPickFocalPoint) return;
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const x = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    onPickFocalPoint(Number(x.toFixed(3)), Number(y.toFixed(3)));
  };

  return (
    <div
      onClick={handleCanvasClick}
      className={`relative overflow-hidden rounded-[4px] bg-zinc-950 border border-zinc-800 shadow-2xl ${
        aspectRatio === "9:16"
          ? isFullscreen
            ? "aspect-[9/16] h-[74vh] max-h-[820px] w-auto max-w-full"
            : "aspect-[9/16] h-[205px] sm:h-[240px] w-auto max-w-full"
          : isFullscreen
          ? "aspect-video max-h-[74vh] w-full max-w-[1120px]"
          : "aspect-video h-[205px] sm:h-[240px] w-auto max-w-full"
      } ${isPickingFocal ? "cursor-crosshair ring-2 ring-brand-lavender" : ""}`}
    >
      <canvas
        ref={canvasRef}
        className="h-full w-full block"
        aria-label={`Preview canvas for scene ${activeSceneIndex + 1}`}
      />

      {/* Empty or Loading State Overlay */}
      {!wasmReady && !wasmError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center bg-zinc-950/85 text-zinc-400">
          <Loader2 className="h-5 w-5 animate-spin mb-1.5" />
          <span className="font-mono text-[11px]">Initializing preview engine...</span>
        </div>
      )}

      {wasmError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center bg-zinc-950/90 px-4 text-center text-red-400">
          <span className="font-mono text-[11px]">{wasmError}</span>
        </div>
      )}

      {wasmReady && !activeSceneMedia?.url && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center text-zinc-500">
          <Film className="h-7 w-7 mb-1.5 opacity-60" />
          <span className="font-mono text-[11px]">
            Scene {String(activeSceneIndex + 1).padStart(2, "0")} · No image
          </span>
        </div>
      )}

      {wasmReady && activeSceneMedia?.url && !activeImgLoaded && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center bg-zinc-950/70 text-zinc-400">
          <Loader2 className="h-5 w-5 animate-spin mb-1" />
          <span className="font-mono text-[10px]">Loading frame...</span>
        </div>
      )}

      {/* Rule-of-thirds composition & broadcast safe-area guide */}
      {showSafeGrid && (
        <div className="pointer-events-none absolute inset-0">
          <div className="absolute inset-y-0 left-1/3 w-px bg-white/20" />
          <div className="absolute inset-y-0 left-2/3 w-px bg-white/20" />
          <div className="absolute inset-x-0 top-1/3 h-px bg-white/20" />
          <div className="absolute inset-x-0 top-2/3 h-px bg-white/20" />
          <div
            className="absolute inset-x-[6%] inset-y-[8%] border border-dashed border-brand-lavender/50 rounded-[2px]"
            title="Broadcast Title Safe Zone"
          />
        </div>
      )}

      {/* Interactive Focal Point Marker when picking */}
      {isPickingFocal && (
        <div className="pointer-events-none absolute inset-0 bg-black/20">
          <div
            className="absolute -translate-x-1/2 -translate-y-1/2 flex items-center justify-center"
            style={{
              left: `${inspectedFocal.x * 100}%`,
              top: `${inspectedFocal.y * 100}%`,
            }}
          >
            <div className="h-6 w-6 rounded-md border-2 border-brand-lavender bg-black/50 flex items-center justify-center shadow-lg">
              <Crosshair className="h-3.5 w-3.5 text-brand-lavender" />
            </div>
          </div>
          <div className="absolute top-2 left-2 rounded-md bg-black/85 border border-brand-lavender/60 px-2 py-1 font-mono text-[10px] text-brand-lavender">
            Click frame to set focal point for Scene{" "}
            {String(inspectedSceneIndex + 1).padStart(2, "0")}
          </div>
        </div>
      )}

      {/* Live Synced Subtitle Overlay matching M9 burn-in style */}
      {activeSubtitle && subtitleStyle.enabled && (
        <div
          className={`pointer-events-none absolute inset-x-[6%] flex justify-center ${
            subtitleStyle.position === "top"
              ? "top-[8%]"
              : subtitleStyle.position === "center"
              ? "top-1/2 -translate-y-1/2"
              : "bottom-[8%]"
          }`}
        >
          <div
            style={{
              backgroundColor: `rgba(10, 10, 14, ${
                Math.min(100, Math.max(0, subtitleStyle.bg_opacity_pct)) / 100
              })`,
              color: subtitleStyle.text_color || "#FFFFFF",
              fontSize: `${Math.round(
                13 * (Math.min(150, Math.max(70, subtitleStyle.size_pct)) / 100)
              )}px`,
              WebkitTextStroke:
                subtitleStyle.outline_width_px > 0
                  ? `${Math.max(0.5, subtitleStyle.outline_width_px * 0.45)}px #000000`
                  : undefined,
              textShadow:
                subtitleStyle.outline_width_px > 0
                  ? "0 1px 2px rgba(0,0,0,0.95), 0 0 3px rgba(0,0,0,0.9)"
                  : "0 1px 3px rgba(0,0,0,0.9), 0 2px 6px rgba(0,0,0,0.7)",
            }}
            className={`max-w-[94%] rounded-[6px] px-3 py-1.5 text-center font-semibold leading-snug whitespace-pre-line backdrop-blur-[2px] ${
              subtitleStyle.font === "mono" ? "font-mono" : "font-sans"
            }`}
          >
            {wrapSubtitlePreviewLines(activeSubtitle.text, aspectRatio).join("\n") ||
              activeSubtitle.text}
          </div>
        </div>
      )}
    </div>
  );
}
