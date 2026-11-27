"use client";

import React, { useState, useRef, useEffect, useMemo, useCallback } from "react";
import {
  Play,
  Pause,
  ZoomIn,
  ZoomOut,
  Clock,
  Film,
  ChevronLeft,
  ChevronRight,
  Repeat,
  Magnet,
  Edit3,
  Volume2,
  VolumeX,
  PanelLeftClose,
  PanelLeftOpen,
  Grid,
  Captions,
  SkipBack,
  SkipForward,
  Sliders,
  Shuffle,
  Music,
  Download,
  Maximize2,
  Minimize2,
  AlertTriangle,
  LocateFixed,
  Eye,
  EyeOff,
  Lock,
  Unlock,
  Images,
  Check,
  Sticker,
} from "lucide-react";
import type {
  BoundaryTransitionDto,
  MusicSettingsDto,
  RenderSpecResponse,
  StickerPlacementSpec,
  SubtitleCueDto,
  SubtitleStyleDto,
  UpdateMusicSettingsRequest,
  UpdateSceneEffectRequest,
} from "@pace/shared-types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { api } from "@/lib/api-client";
import { useScriptEditorStore } from "@/stores/script-editor-store";
import { useMediaStore } from "@/features/media/media-store";
import { getProjectAudio } from "@/lib/audio-storage";
import {
  moveBoundary,
  formatTimestampPrecise,
  parseTimestamp,
  extractFlaggedCuts,
  SilenceGap,
  TimelineBoundaryItem,
} from "./timeline-engine.ts";
import { PreviewPlayer, PreviewSpecMeta } from "./preview-player";
import { MobileCapCutStudio } from "./mobile-capcut-studio";
import { SceneEffectsPanel } from "./scene-effects-panel";
import { StickersPanel } from "./stickers-panel";
import {
  DEFAULT_MUSIC_SETTINGS,
  DEFAULT_SUBTITLE_STYLE,
  MusicInspectorPanel,
  SubtitlesInspectorPanel,
} from "./subtitles-music-panel";
import {
  formatPresetLabel,
  parseBoundaryTransition,
} from "./preview-engine";

function resolveMusicMime(file: File): string | null {
  const mime = file.type.split(";")[0]?.trim().toLowerCase() || "";
  if (
    [
      "audio/mpeg",
      "audio/mp3",
      "audio/wav",
      "audio/x-wav",
      "audio/wave",
      "audio/mp4",
      "audio/x-m4a",
      "audio/m4a",
      "audio/aac",
      "audio/ogg",
      "audio/opus",
      "audio/flac",
      "audio/x-flac",
      "audio/webm",
    ].includes(mime)
  ) {
    return mime;
  }
  const lower = file.name.toLowerCase();
  if (lower.endsWith(".mp3")) return "audio/mpeg";
  if (lower.endsWith(".wav")) return "audio/wav";
  if (lower.endsWith(".m4a")) return "audio/mp4";
  if (lower.endsWith(".aac")) return "audio/aac";
  if (lower.endsWith(".ogg")) return "audio/ogg";
  if (lower.endsWith(".opus")) return "audio/opus";
  if (lower.endsWith(".flac")) return "audio/flac";
  if (lower.endsWith(".webm")) return "audio/webm";
  return null;
}

async function computeFileSha256Hex(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const hashBuffer = await crypto.subtle.digest("SHA-256", buf);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function putFileWithProgress(
  url: string,
  file: File,
  contentType: string,
  onProgress: (pct: number) => void
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("PUT", url, true);
    xhr.setRequestHeader("Content-Type", contentType);
    xhr.upload.onprogress = (ev) => {
      if (ev.lengthComputable && ev.total > 0) {
        onProgress(Math.round((ev.loaded / ev.total) * 100));
      }
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        onProgress(100);
        resolve();
      } else {
        reject(new Error(`Music upload failed with HTTP ${xhr.status}`));
      }
    };
    xhr.onerror = () => reject(new Error("Network error uploading music file"));
    xhr.send(file);
  });
}

interface StepTimelineViewProps {
  projectId: string;
  projectName?: string;
  projectAspectRatio: "16:9" | "9:16";
  onOpenExport?: () => void;
  onChangeStep?: (step: "script" | "media" | "narration" | "timeline") => void;
  onExitProject?: () => void;
}

function buildFallbackWaveform(barsCount: number): [number, number][] {
  const result: [number, number][] = [];
  for (let i = 0; i < barsCount; i++) {
    const val =
      0.22 +
      Math.abs(Math.sin(i * 0.43) * 0.42) +
      Math.abs(Math.cos(i * 1.17) * 0.25);
    const amp = Math.min(0.95, val);
    result.push([-amp, amp]);
  }
  return result;
}

export function StepTimelineView({
  projectId,
  projectName,
  projectAspectRatio,
  onOpenExport,
  onChangeStep,
  onExitProject,
}: StepTimelineViewProps) {
  const [isMobileViewport, setIsMobileViewport] = useState(false);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const mq = window.matchMedia("(max-width: 1023px)");
    const update = () => setIsMobileViewport(mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);
  const { sections } = useScriptEditorStore();
  const {
    media,
    assignments,
    setMedia,
    setAssignments,
    assignToSection,
    updateSceneEffect,
  } = useMediaStore();

  // Playback & monitor state
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoopingScene, setIsLoopingScene] = useState(false);
  const [isMuted, setIsMuted] = useState(false);
  const [isV1Locked, setIsV1Locked] = useState(false);
  const [isA1Muted, setIsA1Muted] = useState(false);
  const [isA2Muted, setIsA2Muted] = useState(false);
  const [showSafeGrid, setShowSafeGrid] = useState(false);
  const [isMonitorFullscreen, setIsMonitorFullscreen] = useState(false);
  const [showSceneDrawer, setShowSceneDrawer] = useState(true);
  const [leftDrawerTab, setLeftDrawerTab] = useState<"scenes" | "media">("scenes");
  const [showMobileInspector, setShowMobileInspector] = useState(false);
  const [inspectorTab, setInspectorTab] = useState<
    "effects" | "stickers" | "timing" | "subtitles" | "music"
  >("effects");
  const [isPickingFocal, setIsPickingFocal] = useState(false);

  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const [zoomLevel, setZoomLevel] = useState(1);
  const [selectedSceneIndex, setSelectedSceneIndex] = useState(0);
  const [isPauseSnapEnabled, setIsPauseSnapEnabled] = useState(true);
  const [isAutoFollowEnabled, setIsAutoFollowEnabled] = useState(true);
  const [snappedFeedback, setSnappedFeedback] = useState<string | null>(null);

  // M7 Effects & RenderSpec state
  const [effectSeed, setEffectSeed] = useState<number>(() => {
    if (typeof window !== "undefined") {
      try {
        const saved = localStorage.getItem(`pace_project_${projectId}_effects`);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (typeof parsed.effectSeed === "number") return parsed.effectSeed;
        }
      } catch { }
    }
    return 42;
  });
  const [compiledRenderSpec, setCompiledRenderSpec] =
    useState<RenderSpecResponse | null>(null);
  const [preferLocalSpec, setPreferLocalSpec] = useState(false);
  const [specMeta, setSpecMeta] = useState<PreviewSpecMeta | null>(null);
  const [isRerolling, setIsRerolling] = useState(false);
  const [isSavingEffect, setIsSavingEffect] = useState(false);

  // Exact timecode modal state
  const [editingBoundaryIndex, setEditingBoundaryIndex] = useState<number | null>(null);
  const [editTimeInput, setEditTimeInput] = useState("");
  const [editError, setEditError] = useState<string | null>(null);

  // Narration & timeline telemetry state (M5 API + IndexedDB/localStorage fallback)
  const [backendDurationMs, setBackendDurationMs] = useState<number | null>(null);
  const [silences, setSilences] = useState<SilenceGap[]>(() => {
    if (typeof window !== "undefined") {
      try {
        const saved = localStorage.getItem(`pace_project_${projectId}_narration`);
        if (saved) return JSON.parse(saved).silences || [];
      } catch { }
    }
    return [];
  });
  const [peaks, setPeaks] = useState<[number, number][]>([]);
  const [subtitleCues, setSubtitleCues] = useState<SubtitleCueDto[]>([]);
  const [subtitleStyle, setSubtitleStyle] = useState<SubtitleStyleDto>(
    DEFAULT_SUBTITLE_STYLE
  );
  const [isUpdatingSubtitles, setIsUpdatingSubtitles] = useState(false);
  const [stickers, setStickers] = useState<StickerPlacementSpec[]>([]);
  const [isSavingStickers, setIsSavingStickers] = useState(false);
  const [includeTiltInAuto, setIncludeTiltInAuto] = useState(false);

  // M9 Background Music state
  const [musicFilename, setMusicFilename] = useState<string | null>(null);
  const [musicUrl, setMusicUrl] = useState<string | null>(null);
  const [musicSettings, setMusicSettings] = useState<MusicSettingsDto>(
    DEFAULT_MUSIC_SETTINGS
  );
  const [isUploadingMusic, setIsUploadingMusic] = useState(false);
  const [musicUploadProgress, setMusicUploadProgress] = useState(0);

  // Audio playback references
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const objectUrlRef = useRef<string | null>(null);
  const [hasAudioSource, setHasAudioSource] = useState(false);
  const [audioFileName, setAudioFileName] = useState("");

  const totalDurationMs = useMemo(() => {
    if (backendDurationMs && backendDurationMs > 0) return backendDurationMs;
    if (typeof window !== "undefined") {
      try {
        const saved = localStorage.getItem(`pace_project_${projectId}_narration`);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (parsed?.durationMs > 0) return parsed.durationMs;
        }
      } catch { }
    }
    return Math.max(30000, sections.length * 12000);
  }, [backendDurationMs, projectId, sections.length]);

  const [boundaries, setBoundaries] = useState<TimelineBoundaryItem[]>(() => {
    if (typeof window !== "undefined") {
      try {
        const saved = localStorage.getItem(`pace_project_${projectId}_narration`);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (parsed?.boundaries && parsed.boundaries.length >= 2) {
            return parsed.boundaries.map(
              (b: {
                index: number;
                timeMs: number;
                transition?: Record<string, unknown> | null;
              }) => ({
                index: b.index,
                timeMs: b.timeMs,
                transition: b.transition ?? null,
              })
            );
          }
        }
      } catch { }
    }
    const count = Math.max(1, sections.length);
    const slice = totalDurationMs / count;
    return Array.from({ length: count + 1 }, (_, i) => ({
      index: i,
      timeMs: Math.round(i * slice),
      transition: null,
    }));
  });

  const persistEffectsLocal = useCallback(
    (seed: number, currentAssignments = useMediaStore.getState().assignments) => {
      if (typeof window === "undefined") return;
      try {
        localStorage.setItem(
          `pace_project_${projectId}_effects`,
          JSON.stringify({
            effectSeed: seed,
            assignments: currentAssignments.map((a) => ({
              sectionId: a.sectionId,
              fitMode: a.fitMode,
              focalX: a.focalX,
              focalY: a.focalY,
              effect: a.effect,
              effectOverridden: a.effectOverridden,
            })),
          })
        );
      } catch { }
    },
    [projectId]
  );

  const refreshRenderSpec = useCallback(async () => {
    try {
      const specResp = await api.effects.getRenderSpec(projectId);
      setCompiledRenderSpec(specResp);
      setEffectSeed(specResp.effect_seed);
      setPreferLocalSpec(false);
    } catch {
      // Offline / unseeded fallback: keep WASM compiling locally
    }
  }, [projectId]);

  // Wire audio element helper
  const attachAudioUrl = useCallback((url: string, filename: string, isBlob = false) => {
    if (audioRef.current) {
      audioRef.current.pause();
    }
    if (objectUrlRef.current) {
      URL.revokeObjectURL(objectUrlRef.current);
      objectUrlRef.current = null;
    }
    if (isBlob) {
      objectUrlRef.current = url;
    }

    const audio = new Audio(url);
    audio.preload = "metadata";
    audioRef.current = audio;
    setHasAudioSource(true);
    setAudioFileName(filename);

    audio.onplay = () => setIsPlaying(true);
    audio.onpause = () => setIsPlaying(false);
    audio.onended = () => {
      setIsPlaying(false);
      setCurrentTimeMs(0);
    };
    audio.ontimeupdate = () => {
      if (audioRef.current) {
        setCurrentTimeMs(Math.round(audioRef.current.currentTime * 1000));
      }
    };
  }, []);

  // Hydrate full timeline state, effects, RenderSpec & audio from API (with offline fallback)
  useEffect(() => {
    let active = true;

    async function hydrateTimeline() {
      try {
        const [tlState, specResp, mediaListResp] = await Promise.all([
          api.timeline.get(projectId),
          api.effects.getRenderSpec(projectId).catch(() => null),
          api.media.list(projectId).catch(() => []),
        ]);
        if (!active) return;

        if (mediaListResp.length > 0) {
          const existingMedia = useMediaStore.getState().media;
          const mediaById = new Map(existingMedia.map((m) => [m.id, m]));
          for (const m of mediaListResp) {
            const w = m.width ?? 16;
            const h = m.height ?? 9;
            mediaById.set(m.id, {
              id: m.id,
              filename: m.filename,
              bytes: m.bytes,
              sha256: m.sha256,
              mime: m.mime,
              aspectRatio: w >= h ? "16:9" : "9:16",
              width: m.width ?? undefined,
              height: m.height ?? undefined,
              storageKey: "",
              thumbUrl: m.thumb_url || m.download_url,
              previewUrl: m.preview_url || m.download_url,
            });
          }
          setMedia(Array.from(mediaById.values()));
        }

        if (tlState.duration_ms > 0) {
          setBackendDurationMs(tlState.duration_ms);
        }
        setEffectSeed(tlState.effect_seed);

        if (tlState.silences?.length) {
          setSilences(
            tlState.silences.map((s) => ({
              startMs: s.start_ms,
              endMs: s.end_ms,
              durationMs: s.duration_ms,
            }))
          );
        }

        if (tlState.boundaries?.length >= 2) {
          setBoundaries(
            [...tlState.boundaries]
              .sort((a, b) => a.index - b.index)
              .map((b) => ({
                index: b.index,
                timeMs: b.time_ms,
                source: b.source,
                confidence: b.confidence,
                flags: b.flags,
                transition: b.transition,
              }))
          );
        }

        if (tlState.subtitle_cues?.length) {
          setSubtitleCues(
            [...tlState.subtitle_cues].sort((a, b) => a.position - b.position)
          );
        }
        if (tlState.subtitle_style) {
          setSubtitleStyle(tlState.subtitle_style);
        }
        if (tlState.music_settings) {
          setMusicSettings(tlState.music_settings);
        }
        if (Array.isArray(tlState.stickers)) {
          setStickers(tlState.stickers);
        }
        if (typeof tlState.include_tilt_in_auto === "boolean") {
          setIncludeTiltInAuto(tlState.include_tilt_in_auto);
        }
        setMusicFilename(tlState.music_filename ?? null);
        setMusicUrl(tlState.music_url ?? null);

        if (tlState.scenes?.length) {
          const existingMedia = useMediaStore.getState().media;
          const mediaById = new Map(existingMedia.map((m) => [m.id, m]));
          let mediaUpdated = false;

          for (const sc of tlState.scenes) {
            if (sc.media_id && !mediaById.has(sc.media_id)) {
              mediaById.set(sc.media_id, {
                id: sc.media_id,
                filename: sc.filename || `scene-${sc.index + 1}.jpg`,
                bytes: 0,
                sha256: "",
                mime: "image/jpeg",
                aspectRatio: tlState.aspect_ratio === "9:16" ? "9:16" : "16:9",
                storageKey: "",
                thumbUrl: sc.thumb_url || sc.preview_url || "",
                previewUrl: sc.preview_url || sc.thumb_url || "",
              });
              mediaUpdated = true;
            }
          }
          if (mediaUpdated) {
            setMedia(Array.from(mediaById.values()));
          }

          setAssignments(
            tlState.scenes.map((sc) => ({
              sectionId: sc.section_id,
              mediaId: sc.media_id ?? null,
              fitMode: sc.fit_mode === "fit_blur" ? "fit_blur" : "fill",
              focalX: sc.focal_x,
              focalY: sc.focal_y,
              effect: sc.effect,
              effectOverridden: sc.effect_overridden,
              resolvedPreset: sc.resolved_preset,
              resolvedHasShake: sc.resolved_has_shake,
            }))
          );
        }

        if (specResp) {
          setCompiledRenderSpec(specResp);
          setPreferLocalSpec(false);
        }

        const audioUrl = tlState.audio_url || specResp?.audio_url;
        if (audioUrl) {
          attachAudioUrl(
            audioUrl,
            tlState.audio_filename || "narration.opus",
            false
          );
        }

        if (tlState.has_peaks) {
          api.narration
            .peaks(projectId)
            .then((peaksResp) => {
              if (active && peaksResp.peaks?.length) {
                setPeaks(peaksResp.peaks);
              }
            })
            .catch(() => { });
        }
        return;
      } catch {
        // Fallback to M5 narration endpoint or local IndexedDB/localStorage if offline
      }

      try {
        const state = await api.narration.get(projectId);
        if (!active) return;

        if (state.duration_ms && state.duration_ms > 0) {
          setBackendDurationMs(state.duration_ms);
        }
        if (state.silences?.length) {
          setSilences(
            state.silences.map((s) => ({
              startMs: s.start_ms,
              endMs: s.end_ms,
              durationMs: s.duration_ms,
            }))
          );
        }
        if (state.boundaries?.length >= 2) {
          setBoundaries(
            [...state.boundaries]
              .sort((a, b) => a.index - b.index)
              .map((b) => ({
                index: b.index,
                timeMs: b.time_ms,
                transition: b.transition,
              }))
          );
        }
        if (state.subtitle_cues?.length) {
          setSubtitleCues(
            [...state.subtitle_cues].sort((a, b) => a.position - b.position)
          );
        }
        if (state.audio_url) {
          attachAudioUrl(state.audio_url, state.filename || "narration.opus", false);
        }
        if (state.has_peaks) {
          api.narration
            .peaks(projectId)
            .then((peaksResp) => {
              if (active && peaksResp.peaks?.length) {
                setPeaks(peaksResp.peaks);
              }
            })
            .catch(() => { });
        }
        return;
      } catch {
        // Offline mode: hydrate local effect overrides & IndexedDB audio
      }

      if (typeof window !== "undefined") {
        try {
          const savedEffects = localStorage.getItem(
            `pace_project_${projectId}_effects`
          );
          if (savedEffects && active) {
            const parsed = JSON.parse(savedEffects);
            if (typeof parsed.effectSeed === "number") {
              setEffectSeed(parsed.effectSeed);
            }
            if (Array.isArray(parsed.assignments)) {
              for (const item of parsed.assignments) {
                if (item?.sectionId) {
                  updateSceneEffect(item.sectionId, {
                    fitMode: item.fitMode === "fit_blur" ? "fit_blur" : "fill",
                    focalX: typeof item.focalX === "number" ? item.focalX : 0.5,
                    focalY: typeof item.focalY === "number" ? item.focalY : 0.5,
                    effect: item.effect,
                    effectOverridden: Boolean(item.effectOverridden),
                  });
                }
              }
              setPreferLocalSpec(true);
            }
          }
        } catch { }
      }

      try {
        const saved = await getProjectAudio(projectId);
        if (saved && active) {
          const blobUrl = URL.createObjectURL(saved.blob);
          attachAudioUrl(blobUrl, saved.filename, true);
        }
      } catch { }
    }

    hydrateTimeline();

    return () => {
      active = false;
      if (audioRef.current) {
        audioRef.current.pause();
      }
      if (objectUrlRef.current) {
        URL.revokeObjectURL(objectUrlRef.current);
      }
    };
  }, [projectId, attachAudioUrl, setMedia, setAssignments, updateSceneEffect]);

  // Mute sync (global monitor mute or individual A1 voiceover track mute)
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.muted = isMuted || isA1Muted;
    }
  }, [isMuted, isA1Muted]);

  // Keep boundaries synchronized if script section count changes cleanly before narration load
  useEffect(() => {
    const targetCount = Math.max(1, sections.length);
    if (boundaries.length === targetCount + 1) return;
    const slice = totalDurationMs / targetCount;
    setBoundaries(
      Array.from({ length: targetCount + 1 }, (_, i) => ({
        index: i,
        timeMs: Math.round(i * slice),
        transition: null,
      }))
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sections.length]);

  // Dragging boundary & ruler scrubbing state
  const [draggingBoundaryIndex, setDraggingBoundaryIndex] = useState<number | null>(null);
  const [isScrubbingPlayhead, setIsScrubbingPlayhead] = useState(false);
  const [hoverTimeMs, setHoverTimeMs] = useState<number | null>(null);
  const draggingBoundaryRef = useRef<number | null>(null);
  const latestBoundariesRef = useRef<TimelineBoundaryItem[]>(boundaries);
  latestBoundariesRef.current = boundaries;
  const timelineTrackRef = useRef<HTMLDivElement>(null);

  const pxPerSec = 64 * zoomLevel;
  const totalWidthPx = Math.max(640, (totalDurationMs / 1000) * pxPerSec);

  const activeSceneIndex = useMemo(() => {
    if (boundaries.length <= 1) return 0;
    for (let i = 0; i < boundaries.length - 1; i++) {
      if (
        currentTimeMs >= boundaries[i].timeMs &&
        currentTimeMs < boundaries[i + 1].timeMs
      ) {
        return i;
      }
    }
    return Math.max(0, sections.length - 1);
  }, [currentTimeMs, boundaries, sections.length]);

  // Active scene range for loop mode
  const currentSceneRange = useMemo(() => {
    const start = boundaries[selectedSceneIndex]?.timeMs ?? 0;
    const end = boundaries[selectedSceneIndex + 1]?.timeMs ?? totalDurationMs;
    return { start, end };
  }, [boundaries, selectedSceneIndex, totalDurationMs]);

  // High-frequency 30fps clock sync during real <audio> playback so WASM motion is smooth
  useEffect(() => {
    if (!isPlaying || !hasAudioSource) return;

    let rafId = 0;
    const tick = () => {
      if (audioRef.current) {
        const ms = Math.round(audioRef.current.currentTime * 1000);
        setCurrentTimeMs(ms);
      }
      rafId = requestAnimationFrame(tick);
    };

    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [isPlaying, hasAudioSource]);

  // Simulated clock fallback when playing without uploaded audio file
  useEffect(() => {
    if (!isPlaying || hasAudioSource) return;

    let last = performance.now();
    let rafId = 0;

    const tick = (now: number) => {
      const delta = now - last;
      last = now;
      setCurrentTimeMs((prev) => {
        const next = prev + delta;
        if (isLoopingScene && next >= currentSceneRange.end) {
          return currentSceneRange.start;
        }
        if (next >= totalDurationMs) {
          setIsPlaying(false);
          return 0;
        }
        return next;
      });
      rafId = requestAnimationFrame(tick);
    };

    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [isPlaying, hasAudioSource, isLoopingScene, currentSceneRange, totalDurationMs]);

  // Loop guard for real <audio>
  useEffect(() => {
    if (!isLoopingScene || !isPlaying || !hasAudioSource) return;
    if (currentTimeMs >= currentSceneRange.end) {
      if (audioRef.current) {
        audioRef.current.currentTime = currentSceneRange.start / 1000;
      }
      setCurrentTimeMs(currentSceneRange.start);
    }
  }, [currentTimeMs, isLoopingScene, isPlaying, hasAudioSource, currentSceneRange]);

  const seekToMs = useCallback(
    (timeMs: number) => {
      const clamped = Math.max(0, Math.min(totalDurationMs, Math.round(timeMs)));
      setCurrentTimeMs(clamped);
      if (audioRef.current) {
        audioRef.current.currentTime = clamped / 1000;
      }
    },
    [totalDurationMs]
  );

  const togglePlay = useCallback(() => {
    if (audioRef.current && hasAudioSource) {
      if (isPlaying) {
        audioRef.current.pause();
      } else {
        if (isLoopingScene) {
          const inRange =
            currentTimeMs >= currentSceneRange.start &&
            currentTimeMs < currentSceneRange.end;
          if (!inRange) {
            audioRef.current.currentTime = currentSceneRange.start / 1000;
            setCurrentTimeMs(currentSceneRange.start);
          }
        }
        audioRef.current.play().catch(() => { });
      }
      return;
    }

    if (!isPlaying && isLoopingScene) {
      const inRange =
        currentTimeMs >= currentSceneRange.start &&
        currentTimeMs < currentSceneRange.end;
      if (!inRange) setCurrentTimeMs(currentSceneRange.start);
    }
    setIsPlaying((p) => !p);
  }, [hasAudioSource, isPlaying, isLoopingScene, currentTimeMs, currentSceneRange]);

  // Step 1 frame (33ms at 30fps)
  const stepFrame = useCallback(
    (direction: -1 | 1) => {
      if (isPlaying) {
        if (audioRef.current) audioRef.current.pause();
        setIsPlaying(false);
      }
      seekToMs(currentTimeMs + direction * Math.round(1000 / 30));
    },
    [isPlaying, currentTimeMs, seekToMs]
  );

  // Desktop keyboard shortcuts (Space to play/pause, Left/Right arrows to frame step)
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || editingBoundaryIndex !== null) {
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        togglePlay();
      } else if (e.code === "ArrowLeft") {
        e.preventDefault();
        stepFrame(-1);
      } else if (e.code === "ArrowRight") {
        e.preventDefault();
        stepFrame(1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [togglePlay, stepFrame, editingBoundaryIndex]);

  // CapCut-style playhead auto-follow: keep the playhead in view during playback when Follow is active.
  useEffect(() => {
    const el = timelineTrackRef.current;
    if (!el || !isPlaying || !isAutoFollowEnabled) return;
    const playheadX = (currentTimeMs / 1000) * pxPerSec;
    const viewLeft = el.scrollLeft;
    const viewWidth = el.clientWidth;
    if (viewWidth <= 0) return;

    if (playheadX > viewLeft + viewWidth - 48) {
      el.scrollLeft = Math.max(0, playheadX - viewWidth * 0.2);
    } else if (playheadX < viewLeft) {
      el.scrollLeft = Math.max(0, playheadX - viewWidth * 0.2);
    }
  }, [currentTimeMs, pxPerSec, isPlaying, isAutoFollowEnabled]);

  const persistBoundaryLocal = useCallback(
    (updated: TimelineBoundaryItem[]) => {
      if (typeof window === "undefined") return;
      try {
        const key = `pace_project_${projectId}_narration`;
        const existing = localStorage.getItem(key);
        const parsed = existing ? JSON.parse(existing) : {};
        localStorage.setItem(
          key,
          JSON.stringify({ ...parsed, boundaries: updated })
        );
      } catch { }
    },
    [projectId]
  );

  const commitBoundaryToBackend = useCallback(
    (index: number, timeMs: number, transition?: Record<string, unknown> | null) => {
      api.boundaries
        .update(projectId, index, {
          time_ms: timeMs,
          ...(transition ? { transition } : {}),
        })
        .then(() => refreshRenderSpec())
        .catch(() => { });
    },
    [projectId, refreshRenderSpec]
  );

  const applyBoundaryMove = useCallback(
    (boundaryIndex: number, targetTimeMs: number, commitApi: boolean) => {
      const res = moveBoundary({
        boundaries: latestBoundariesRef.current,
        index: boundaryIndex,
        targetTimeMs,
        totalDurationMs,
        fps: 30,
        minDurationMs: 500,
        silences,
        enablePauseSnap: isPauseSnapEnabled,
        pauseSnapThresholdMs: 160,
      });

      setBoundaries(res.boundaries);
      setPreferLocalSpec(true);
      persistBoundaryLocal(res.boundaries);

      const movedItem = res.boundaries.find((b) => b.index === boundaryIndex);
      if (commitApi && movedItem) {
        commitBoundaryToBackend(
          boundaryIndex,
          movedItem.timeMs,
          movedItem.transition ?? null
        );
      }

      if (res.snappedToPause) {
        setSnappedFeedback("Snapped to speech pause");
      } else {
        setSnappedFeedback("Frame locked (30 FPS)");
      }
    },
    [
      totalDurationMs,
      silences,
      isPauseSnapEnabled,
      persistBoundaryLocal,
      commitBoundaryToBackend,
    ]
  );

  // Pointer drag handling for cut dividers (mouse + touch compatible)
  const handleDividerPointerDown = (e: React.PointerEvent, index: number) => {
    e.stopPropagation();
    if (isV1Locked) {
      setSnappedFeedback("V1 track is locked — unlock V1 to move cuts");
      setTimeout(() => setSnappedFeedback(null), 1600);
      return;
    }
    setDraggingBoundaryIndex(index);
    draggingBoundaryRef.current = index;
  };

  useEffect(() => {
    if (draggingBoundaryIndex === null) return;

    const handlePointerMove = (e: PointerEvent) => {
      if (!timelineTrackRef.current) return;
      const rect = timelineTrackRef.current.getBoundingClientRect();
      const scrollLeft = timelineTrackRef.current.scrollLeft;
      const offsetX = Math.max(0, e.clientX - rect.left + scrollLeft);
      const rawTimeMs = (offsetX / pxPerSec) * 1000;
      applyBoundaryMove(draggingBoundaryIndex, rawTimeMs, false);
    };

    const handlePointerUp = () => {
      const idx = draggingBoundaryRef.current;
      if (idx !== null) {
        const finalBoundary = latestBoundariesRef.current.find((b) => b.index === idx);
        if (finalBoundary) {
          commitBoundaryToBackend(
            idx,
            finalBoundary.timeMs,
            finalBoundary.transition ?? null
          );
        }
      }
      setDraggingBoundaryIndex(null);
      draggingBoundaryRef.current = null;
      setTimeout(() => setSnappedFeedback(null), 1600);
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
    };
  }, [draggingBoundaryIndex, pxPerSec, applyBoundaryMove, commitBoundaryToBackend]);

  // Nudge out-cut of selected scene by deltaMs
  const nudgeSelectedOutCut = (deltaMs: number) => {
    if (isV1Locked) {
      setSnappedFeedback("V1 track is locked — unlock V1 to trim cuts");
      setTimeout(() => setSnappedFeedback(null), 1600);
      return;
    }
    const outIdx = selectedSceneIndex + 1;
    if (outIdx <= 0 || outIdx >= boundaries.length - 1) return;
    const currentOut = boundaries[outIdx]?.timeMs ?? 0;
    applyBoundaryMove(outIdx, currentOut + deltaMs, true);
    setTimeout(() => setSnappedFeedback(null), 1600);
  };

  const openTimecodeEditor = (index: number) => {
    if (isV1Locked) {
      setSnappedFeedback("V1 track is locked — unlock V1 to edit cut timecode");
      setTimeout(() => setSnappedFeedback(null), 1600);
      return;
    }
    if (index <= 0 || index >= boundaries.length - 1) return;
    setEditingBoundaryIndex(index);
    setEditTimeInput(formatTimestampPrecise(boundaries[index].timeMs));
    setEditError(null);
  };

  const saveExactTimecode = (e: React.FormEvent) => {
    e.preventDefault();
    if (editingBoundaryIndex === null) return;
    const parsedMs = parseTimestamp(editTimeInput);
    if (parsedMs === null) {
      setEditError("Enter mm:ss.mmm (for example 01:14.233)");
      return;
    }

    const prevTime = boundaries[editingBoundaryIndex - 1]?.timeMs ?? 0;
    const nextTime = boundaries[editingBoundaryIndex + 1]?.timeMs ?? totalDurationMs;
    if (parsedMs < prevTime + 500 || parsedMs > nextTime - 500) {
      setEditError(
        `Valid window: ${formatTimestampPrecise(prevTime + 500)} to ${formatTimestampPrecise(
          nextTime - 500
        )}`
      );
      return;
    }

    applyBoundaryMove(editingBoundaryIndex, parsedMs, true);
    setEditingBoundaryIndex(null);
  };

  const clientXToTimeMs = useCallback(
    (clientX: number) => {
      if (!timelineTrackRef.current) return 0;
      const rect = timelineTrackRef.current.getBoundingClientRect();
      const scrollLeft = timelineTrackRef.current.scrollLeft;
      const offsetX = Math.max(0, clientX - rect.left + scrollLeft);
      return Math.max(
        0,
        Math.min(totalDurationMs, Math.round((offsetX / pxPerSec) * 1000))
      );
    },
    [pxPerSec, totalDurationMs]
  );

  const handleTrackScrub = (e: React.MouseEvent<HTMLDivElement>) => {
    seekToMs(clientXToTimeMs(e.clientX));
  };

  const handleRulerPointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.stopPropagation();
      setIsScrubbingPlayhead(true);
      seekToMs(clientXToTimeMs(e.clientX));
    },
    [clientXToTimeMs, seekToMs]
  );

  useEffect(() => {
    if (!isScrubbingPlayhead) return;

    const onPointerMove = (e: PointerEvent) => {
      const el = timelineTrackRef.current;
      if (el) {
        const rect = el.getBoundingClientRect();
        if (e.clientX > rect.right - 36) {
          el.scrollLeft += 18;
        } else if (e.clientX < rect.left + 36) {
          el.scrollLeft = Math.max(0, el.scrollLeft - 18);
        }
      }
      seekToMs(clientXToTimeMs(e.clientX));
    };

    const onPointerUp = () => {
      setIsScrubbingPlayhead(false);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    };
  }, [isScrubbingPlayhead, clientXToTimeMs, seekToMs]);

  // Monitor media & active subtitle calculation
  const monitorSceneIndex = activeSceneIndex;
  const activeSection = sections[monitorSceneIndex];
  const activeAssignment = assignments.find((a) => a.sectionId === activeSection?.id);

  const inspectedSection = sections[selectedSceneIndex];
  const inspectedAssignment = assignments.find(
    (a) => a.sectionId === inspectedSection?.id
  );
  const inspectedMedia = inspectedAssignment?.mediaId
    ? media.find((m) => m.id === inspectedAssignment.mediaId)
    : undefined;

  const activeResolved = activeSection
    ? specMeta?.resolvedPresets[activeSection.id]
    : undefined;
  const inspectedResolved = inspectedSection
    ? specMeta?.resolvedPresets[inspectedSection.id]
    : undefined;

  const handleUpdateSceneEffect = useCallback(
    async (patch: UpdateSceneEffectRequest) => {
      if (!inspectedSection) return;

      const currentAssign = useMediaStore
        .getState()
        .assignments.find((a) => a.sectionId === inspectedSection.id);
      const prevEff = currentAssign?.effect ?? {
        manual_preset: null,
        intensity: "normal",
        easing: "ease_in_out",
        manual_shake: null,
      };

      const reset = patch.reset_to_auto === true;
      const nextEffect = reset
        ? {
          manual_preset: null,
          intensity: "normal",
          easing: "ease_in_out",
          manual_shake: null,
        }
        : {
          manual_preset: patch.clear_manual_preset
            ? null
            : patch.manual_preset !== undefined
              ? patch.manual_preset
              : prevEff.manual_preset ?? null,
          intensity: patch.intensity ?? prevEff.intensity ?? "normal",
          easing: patch.easing ?? prevEff.easing ?? "ease_in_out",
          manual_shake: patch.clear_manual_shake
            ? null
            : patch.manual_shake !== undefined
              ? patch.manual_shake
              : prevEff.manual_shake ?? null,
        };

      const nextOverridden = reset
        ? false
        : Boolean(
          nextEffect.manual_preset ||
          nextEffect.manual_shake !== null ||
          nextEffect.intensity !== "normal" ||
          nextEffect.easing !== "ease_in_out"
        );

      updateSceneEffect(inspectedSection.id, {
        ...(patch.fit_mode
          ? {
            fitMode:
              patch.fit_mode === "fit_blur" ? "fit_blur" : "fill",
          }
          : {}),
        ...(typeof patch.focal_x === "number" ? { focalX: patch.focal_x } : {}),
        ...(typeof patch.focal_y === "number" ? { focalY: patch.focal_y } : {}),
        effect: nextEffect,
        effectOverridden: nextOverridden,
      });

      setPreferLocalSpec(true);
      persistEffectsLocal(effectSeed);
      setSnappedFeedback(reset ? "Reset to automatic motion" : "Scene effect updated");
      setTimeout(() => setSnappedFeedback(null), 1600);

      setIsSavingEffect(true);
      try {
        const updated = await api.effects.updateSceneEffect(
          projectId,
          inspectedSection.id,
          patch
        );
        updateSceneEffect(inspectedSection.id, {
          fitMode: updated.fit_mode === "fit_blur" ? "fit_blur" : "fill",
          focalX: updated.focal_x,
          focalY: updated.focal_y,
          effect: updated.effect,
          effectOverridden: updated.effect_overridden,
        });
        persistEffectsLocal(effectSeed);
        await refreshRenderSpec();
      } catch {
        // Keep optimistic local edit when offline
      } finally {
        setIsSavingEffect(false);
      }
    },
    [
      inspectedSection,
      projectId,
      effectSeed,
      updateSceneEffect,
      persistEffectsLocal,
      refreshRenderSpec,
    ]
  );

  const handlePickFocalPoint = useCallback(
    (focalX: number, focalY: number) => {
      setIsPickingFocal(false);
      void handleUpdateSceneEffect({ focal_x: focalX, focal_y: focalY });
    },
    [handleUpdateSceneEffect]
  );

  const handleUpdateOutTransition = useCallback(
    (transition: BoundaryTransitionDto) => {
      const outIdx = selectedSceneIndex + 1;
      if (outIdx <= 0 || outIdx >= boundaries.length - 1) return;

      const updatedBoundaries = boundaries.map((b) =>
        b.index === outIdx ? { ...b, transition: { ...transition } } : b
      );
      setBoundaries(updatedBoundaries);
      setPreferLocalSpec(true);
      persistBoundaryLocal(updatedBoundaries);

      const targetBoundary = updatedBoundaries.find((b) => b.index === outIdx);
      if (targetBoundary) {
        commitBoundaryToBackend(outIdx, targetBoundary.timeMs, {
          ...transition,
        });
      }

      setSnappedFeedback(
        transition.kind === "cut"
          ? "Transition set to hard cut"
          : `${transition.kind === "fade_black" ? "Fade black" : "Crossfade"
          } (${transition.duration_ms}ms)`
      );
      setTimeout(() => setSnappedFeedback(null), 1600);
    },
    [
      selectedSceneIndex,
      boundaries,
      persistBoundaryLocal,
      commitBoundaryToBackend,
    ]
  );

  const handleRerollEffects = useCallback(async () => {
    if (isRerolling) return;
    setIsRerolling(true);

    const nextLocalSeed = effectSeed + 1;
    setEffectSeed(nextLocalSeed);
    setPreferLocalSpec(true);

    const currentAssignments = useMediaStore.getState().assignments;
    for (const a of currentAssignments) {
      if (!a.effectOverridden) {
        updateSceneEffect(a.sectionId, {
          effect: {
            manual_preset: null,
            intensity: "normal",
            easing: "ease_in_out",
            manual_shake: null,
          },
          effectOverridden: false,
        });
      }
    }
    persistEffectsLocal(nextLocalSeed);

    try {
      const res = await api.effects.reroll(projectId);
      setEffectSeed(res.effect_seed);
      persistEffectsLocal(res.effect_seed);
      await refreshRenderSpec();
      setSnappedFeedback(`Re-rolled effects (seed #${res.effect_seed})`);
    } catch {
      setSnappedFeedback(`Re-rolled effects (seed #${nextLocalSeed})`);
    } finally {
      setIsRerolling(false);
      setTimeout(() => setSnappedFeedback(null), 1800);
    }
  }, [
    isRerolling,
    effectSeed,
    projectId,
    updateSceneEffect,
    persistEffectsLocal,
    refreshRenderSpec,
  ]);

  const activeSubtitle = useMemo(() => {
    return subtitleCues.find(
      (cue) => currentTimeMs >= cue.start_ms && currentTimeMs <= cue.end_ms
    );
  }, [subtitleCues, currentTimeMs]);

  const handleUpdateSubtitleCue = useCallback(
    async (cueId: string, text: string, startMs?: number, endMs?: number) => {
      setIsUpdatingSubtitles(true);
      setSubtitleCues((prev) =>
        prev.map((c) =>
          c.id === cueId
            ? {
              ...c,
              text,
              start_ms: startMs ?? c.start_ms,
              end_ms: endMs ?? c.end_ms,
              edited: true,
            }
            : c
        )
      );
      try {
        const updated = await api.subtitles.update(projectId, cueId, {
          text,
          start_ms: startMs,
          end_ms: endMs,
        });
        setSubtitleCues((prev) =>
          prev.map((c) => (c.id === cueId ? updated : c))
        );
        setSnappedFeedback("Subtitle cue saved");
      } catch {
        setSnappedFeedback("Subtitle cue updated locally");
      } finally {
        setIsUpdatingSubtitles(false);
        setTimeout(() => setSnappedFeedback(null), 1600);
      }
    },
    [projectId]
  );

  const handleSplitSubtitleCue = useCallback(
    async (cueId: string) => {
      setIsUpdatingSubtitles(true);
      try {
        const nextCues = await api.subtitles.split(projectId, cueId, {});
        setSubtitleCues([...nextCues].sort((a, b) => a.position - b.position));
        setSnappedFeedback("Split subtitle cue into two");
      } catch {
        // Local split fallback when offline
        setSubtitleCues((prev) => {
          const idx = prev.findIndex((c) => c.id === cueId);
          if (idx < 0) return prev;
          const target = prev[idx];
          const words = target.text.trim().split(/\s+/).filter(Boolean);
          if (words.length < 2 || target.end_ms - target.start_ms <= 400) {
            return prev;
          }
          const midWord = Math.max(1, Math.floor(words.length / 2));
          const midMs = Math.round((target.start_ms + target.end_ms) / 2);
          const left: SubtitleCueDto = {
            ...target,
            end_ms: midMs,
            text: words.slice(0, midWord).join(" "),
            edited: true,
          };
          const right: SubtitleCueDto = {
            id: `${target.id}-split-${Date.now()}`,
            position: target.position + 1,
            start_ms: midMs,
            end_ms: target.end_ms,
            text: words.slice(midWord).join(" "),
            edited: true,
          };
          const out = [...prev];
          out.splice(idx, 1, left, right);
          return out.map((c, i) => ({ ...c, position: i }));
        });
        setSnappedFeedback("Split subtitle cue");
      } finally {
        setIsUpdatingSubtitles(false);
        setTimeout(() => setSnappedFeedback(null), 1600);
      }
    },
    [projectId]
  );

  const handleMergeSubtitleCues = useCallback(
    async (firstCueId: string, secondCueId: string) => {
      setIsUpdatingSubtitles(true);
      try {
        const nextCues = await api.subtitles.merge(projectId, {
          first_cue_id: firstCueId,
          second_cue_id: secondCueId,
        });
        setSubtitleCues([...nextCues].sort((a, b) => a.position - b.position));
        setSnappedFeedback("Merged adjacent subtitle cues");
      } catch {
        setSubtitleCues((prev) => {
          const i1 = prev.findIndex((c) => c.id === firstCueId);
          const i2 = prev.findIndex((c) => c.id === secondCueId);
          if (i1 < 0 || i2 !== i1 + 1) return prev;
          const a = prev[i1];
          const b = prev[i2];
          const merged: SubtitleCueDto = {
            id: a.id,
            position: a.position,
            start_ms: a.start_ms,
            end_ms: b.end_ms,
            text: `${a.text.replace(/\s+/g, " ").trim()} ${b.text
              .replace(/\s+/g, " ")
              .trim()}`,
            edited: true,
          };
          const out = [...prev];
          out.splice(i1, 2, merged);
          return out.map((c, i) => ({ ...c, position: i }));
        });
        setSnappedFeedback("Merged subtitle cues");
      } finally {
        setIsUpdatingSubtitles(false);
        setTimeout(() => setSnappedFeedback(null), 1600);
      }
    },
    [projectId]
  );

  const handleUpdateSubtitleStyle = useCallback(
    async (nextStyle: SubtitleStyleDto) => {
      setSubtitleStyle(nextStyle);
      try {
        const saved = await api.subtitles.updateStyle(projectId, nextStyle);
        setSubtitleStyle(saved);
      } catch { }
    },
    [projectId]
  );

  const persistProjectSettingsPatch = useCallback(
    async (patch: Record<string, unknown>) => {
      try {
        const proj = await api.projects.get(projectId);
        await api.projects.update(
          projectId,
          {
            version: proj.version,
            settings: {
              ...(proj.settings || {}),
              ...patch,
            },
          },
          proj.version
        );
      } catch { }
    },
    [projectId]
  );

  const handleUpdateStickers = useCallback(
    async (nextStickers: StickerPlacementSpec[]) => {
      setStickers(nextStickers);
      setPreferLocalSpec(true);
      setIsSavingStickers(true);
      try {
        await persistProjectSettingsPatch({ stickers: nextStickers });
      } finally {
        setIsSavingStickers(false);
      }
    },
    [persistProjectSettingsPatch]
  );

  const handleToggleIncludeTiltInAuto = useCallback(
    async (nextVal: boolean) => {
      setIncludeTiltInAuto(nextVal);
      setPreferLocalSpec(true);
      await persistProjectSettingsPatch({ include_tilt_in_auto: nextVal });
    },
    [persistProjectSettingsPatch]
  );

  const handleUploadMusicFile = useCallback(
    async (file: File) => {
      const mime = resolveMusicMime(file);
      if (!mime) {
        setSnappedFeedback("Unsupported music format (use MP3, WAV, M4A, OGG, FLAC)");
        setTimeout(() => setSnappedFeedback(null), 2400);
        return;
      }
      setIsUploadingMusic(true);
      setMusicUploadProgress(5);
      try {
        const sha256 = await computeFileSha256Hex(file);
        const presign = await api.music.presign(projectId, {
          filename: file.name,
          content_type: mime,
          bytes: file.size,
        });
        await putFileWithProgress(presign.upload_url, file, mime, (pct) => {
          setMusicUploadProgress(Math.max(8, Math.min(96, pct)));
        });
        const registered = await api.music.register(projectId, {
          media_id: presign.media_id,
          storage_key: presign.storage_key,
          original_filename: file.name,
          mime,
          bytes: file.size,
          sha256,
        });
        setMusicFilename(registered.filename ?? file.name);
        setMusicUrl(registered.audio_url ?? null);
        setMusicSettings(registered.settings);
        setSnappedFeedback(`Uploaded music: ${file.name}`);
      } catch {
        const localUrl = URL.createObjectURL(file);
        setMusicFilename(file.name);
        setMusicUrl(localUrl);
        setSnappedFeedback(`Loaded music track: ${file.name}`);
      } finally {
        setIsUploadingMusic(false);
        setMusicUploadProgress(0);
        setTimeout(() => setSnappedFeedback(null), 2000);
      }
    },
    [projectId]
  );

  const handleUpdateMusicSettings = useCallback(
    async (patch: UpdateMusicSettingsRequest) => {
      setMusicSettings((prev) => ({
        volume_db: patch.volume_db ?? prev.volume_db,
        ducking_enabled: patch.ducking_enabled ?? prev.ducking_enabled,
        duck_db: patch.duck_db ?? prev.duck_db,
        loop_enabled: patch.loop_enabled ?? prev.loop_enabled,
        crossfade_ms: patch.crossfade_ms ?? prev.crossfade_ms,
        fade_in_ms: patch.fade_in_ms ?? prev.fade_in_ms,
        fade_out_ms: patch.fade_out_ms ?? prev.fade_out_ms,
      }));
      try {
        const updated = await api.music.updateSettings(projectId, patch);
        setMusicSettings(updated.settings);
      } catch { }
    },
    [projectId]
  );

  const handleDeleteMusic = useCallback(async () => {
    setMusicFilename(null);
    setMusicUrl(null);
    try {
      await api.music.delete(projectId);
    } catch { }
    setSnappedFeedback("Background music removed");
    setTimeout(() => setSnappedFeedback(null), 1600);
  }, [projectId]);

  const timeTicks = useMemo(() => {
    const totalSec = Math.ceil(totalDurationMs / 1000);
    const interval =
      zoomLevel >= 2.5
        ? 1
        : zoomLevel >= 1.5
          ? 2
          : zoomLevel >= 0.75
            ? 5
            : zoomLevel >= 0.35
              ? 15
              : 30;
    const ticks: number[] = [];
    for (let s = 0; s <= totalSec; s += interval) {
      ticks.push(s * 1000);
    }
    return ticks;
  }, [totalDurationMs, zoomLevel]);

  // High-density A1 voiceover waveform bars with speech-pause awareness
  const waveformBars = useMemo(() => {
    const targetCount = Math.max(
      120,
      Math.min(480, Math.floor(totalWidthPx / 4))
    );
    const base =
      peaks.length === 0
        ? buildFallbackWaveform(targetCount)
        : Array.from({ length: targetCount }, (_, i) => {
          const idx = Math.min(
            peaks.length - 1,
            Math.floor((i * peaks.length) / targetCount)
          );
          return peaks[idx];
        });

    return base.map(([min, max], i) => {
      const barTimeMs = ((i + 0.5) / targetCount) * totalDurationMs;
      const inSilence = silences.some(
        (s) => barTimeMs >= s.startMs && barTimeMs <= s.endMs
      );
      const rawPct = Math.max(
        10,
        Math.min(95, Math.round((Math.abs(max - min) / 2) * 100))
      );
      return {
        heightPct: inSilence && peaks.length === 0 ? 10 : inSilence ? Math.min(20, rawPct) : rawPct,
        inSilence,
      };
    });
  }, [peaks, totalWidthPx, totalDurationMs, silences]);

  // High-density A2 background music waveform bars visualizing auto-ducking & fades
  const musicWaveformBars = useMemo(() => {
    const targetCount = Math.max(
      120,
      Math.min(480, Math.floor(totalWidthPx / 4))
    );
    const duckRatio = musicSettings.ducking_enabled
      ? Math.max(0.28, Math.min(0.85, 1 + musicSettings.duck_db / 22))
      : 1;

    return Array.from({ length: targetCount }, (_, i) => {
      const barTimeMs = ((i + 0.5) / targetCount) * totalDurationMs;
      const inSilence = silences.some(
        (s) => barTimeMs >= s.startMs && barTimeMs <= s.endMs
      );
      const baseWave =
        0.48 +
        Math.abs(Math.sin(i * 0.37) * 0.28) +
        Math.abs(Math.cos(i * 0.91) * 0.18);

      // Apply fade-in and fade-out envelope ramps
      let fadeFactor = 1;
      if (musicSettings.fade_in_ms > 0 && barTimeMs < musicSettings.fade_in_ms) {
        fadeFactor = Math.max(0.15, barTimeMs / musicSettings.fade_in_ms);
      } else if (
        musicSettings.fade_out_ms > 0 &&
        barTimeMs > totalDurationMs - musicSettings.fade_out_ms
      ) {
        fadeFactor = Math.max(
          0.15,
          (totalDurationMs - barTimeMs) / musicSettings.fade_out_ms
        );
      }

      const env = (inSilence ? 1 : duckRatio) * fadeFactor;
      const heightPct = Math.max(12, Math.min(94, Math.round(baseWave * env * 100)));
      return { heightPct, isDucked: !inSilence && musicSettings.ducking_enabled };
    });
  }, [totalWidthPx, totalDurationMs, silences, musicSettings]);

  const outCutEditable =
    selectedSceneIndex + 1 > 0 && selectedSceneIndex + 1 < boundaries.length - 1;
  const outTransition = outCutEditable
    ? parseBoundaryTransition(boundaries[selectedSceneIndex + 1]?.transition)
    : { kind: "cut", duration_ms: 0 };

  const jumpScene = (dir: -1 | 1) => {
    const next = Math.max(0, Math.min(sections.length - 1, selectedSceneIndex + dir));
    setSelectedSceneIndex(next);
    seekToMs(boundaries[next]?.timeMs ?? 0);
  };

  const flaggedCuts = useMemo(
    () =>
      extractFlaggedCuts(
        boundaries,
        sections.map((s) => s.text)
      ),
    [boundaries, sections]
  );

  const handleJumpNextFlaggedCut = useCallback(() => {
    if (flaggedCuts.length === 0) return;
    const nextFlagged =
      flaggedCuts.find((b) => b.timeMs > currentTimeMs + 50) ?? flaggedCuts[0];
    const sceneIdx = Math.max(
      0,
      Math.min(sections.length - 1, nextFlagged.boundaryIndex - 1)
    );
    setSelectedSceneIndex(sceneIdx);
    seekToMs(nextFlagged.timeMs);
    if (timelineTrackRef.current) {
      const targetX = (nextFlagged.timeMs / 1000) * pxPerSec;
      timelineTrackRef.current.scrollLeft = Math.max(
        0,
        targetX - timelineTrackRef.current.clientWidth * 0.35
      );
    }
    setSnappedFeedback(`Jumped to flagged cut #${nextFlagged.boundaryIndex}`);
    setTimeout(() => setSnappedFeedback(null), 1600);
  }, [flaggedCuts, currentTimeMs, sections.length, seekToMs, pxPerSec]);

  const handleFitToTimeline = useCallback(() => {
    const el = timelineTrackRef.current;
    const viewWidth = el ? Math.max(320, el.clientWidth - 24) : 960;
    const durationSec = Math.max(1, totalDurationMs / 1000);
    const idealPxPerSec = viewWidth / durationSec;
    const nextZoom = Math.max(
      0.15,
      Math.min(4, Number((idealPxPerSec / 64).toFixed(2)))
    );
    setZoomLevel(nextZoom);
    if (el) el.scrollLeft = 0;
    setSnappedFeedback("Fit full timeline to screen");
    setTimeout(() => setSnappedFeedback(null), 1500);
  }, [totalDurationMs]);

  const handleAssignMediaInStudio = useCallback(
    async (mediaId: string) => {
      const targetSection = sections[selectedSceneIndex];
      if (!targetSection) return;

      assignToSection(targetSection.id, mediaId);
      setPreferLocalSpec(true);
      const mediaItem = media.find((m) => m.id === mediaId);
      setSnappedFeedback(
        `Assigned ${mediaItem?.filename || "image"} to Scene ${String(
          selectedSceneIndex + 1
        ).padStart(2, "0")}`
      );
      setTimeout(() => setSnappedFeedback(null), 1600);

      try {
        await api.assignments.assign(projectId, targetSection.id, {
          media_id: mediaId,
          fit_mode: inspectedAssignment?.fitMode ?? "fill",
          focal_x: inspectedAssignment?.focalX ?? 0.5,
          focal_y: inspectedAssignment?.focalY ?? 0.5,
        });
        await refreshRenderSpec();
      } catch {
        // Keep optimistic local assignment when offline
      }
    },
    [
      sections,
      selectedSceneIndex,
      assignToSection,
      media,
      projectId,
      inspectedAssignment,
      refreshRenderSpec,
    ]
  );

  const handleSeamTransitionClick = useCallback(
    (boundaryIndex: number) => {
      const targetSceneIdx = boundaryIndex - 1;
      if (targetSceneIdx < 0 || targetSceneIdx >= sections.length) return;

      const b = boundaries.find((item) => item.index === boundaryIndex);
      if (!b) return;

      setSelectedSceneIndex(targetSceneIdx);
      setInspectorTab("effects");

      if (selectedSceneIndex === targetSceneIdx) {
        const currentTrans = parseBoundaryTransition(b.transition);
        const nextKind: BoundaryTransitionDto["kind"] =
          currentTrans.kind === "cut"
            ? "crossfade"
            : currentTrans.kind === "crossfade"
              ? "fade_black"
              : "cut";
        const nextTrans: BoundaryTransitionDto = {
          kind: nextKind,
          duration_ms: nextKind === "cut" ? 0 : currentTrans.duration_ms || 600,
        };

        const updatedBoundaries = boundaries.map((item) =>
          item.index === boundaryIndex
            ? { ...item, transition: { ...nextTrans } }
            : item
        );
        setBoundaries(updatedBoundaries);
        setPreferLocalSpec(true);
        persistBoundaryLocal(updatedBoundaries);
        commitBoundaryToBackend(boundaryIndex, b.timeMs, { ...nextTrans });
        setSnappedFeedback(
          nextKind === "cut"
            ? `S${String(targetSceneIdx + 1).padStart(2, "0")} → Hard Cut`
            : `S${String(targetSceneIdx + 1).padStart(2, "0")} → ${nextKind === "fade_black" ? "Fade Black" : "Crossfade"
            } (${nextTrans.duration_ms}ms)`
        );
        setTimeout(() => setSnappedFeedback(null), 1600);
      } else {
        seekToMs(b.timeMs);
      }
    },
    [
      sections.length,
      boundaries,
      selectedSceneIndex,
      persistBoundaryLocal,
      commitBoundaryToBackend,
      seekToMs,
    ]
  );

  return (
    <div className="mx-auto w-full max-w-[1800px] flex flex-col flex-1 bg-background select-none">
      {isMobileViewport ? (
        <MobileCapCutStudio
          projectId={projectId}
          projectName={projectName}
          projectAspectRatio={projectAspectRatio}
          fps={compiledRenderSpec?.fps ?? 30}
          currentTimeMs={currentTimeMs}
          totalDurationMs={totalDurationMs}
          isPlaying={isPlaying}
          isLoopingScene={isLoopingScene}
          isMuted={isMuted}
          isA1Muted={isA1Muted}
          showSafeGrid={showSafeGrid}
          isPickingFocal={isPickingFocal}
          effectSeed={effectSeed}
          isRerolling={isRerolling}
          isSavingEffect={isSavingEffect}
          zoomLevel={zoomLevel}
          pxPerSec={pxPerSec}
          totalWidthPx={totalWidthPx}
          timeTicks={timeTicks}
          sections={sections}
          boundaries={boundaries}
          assignments={assignments}
          media={media}
          compiledRenderSpec={compiledRenderSpec}
          preferLocalSpec={preferLocalSpec}
          activeSceneIndex={monitorSceneIndex}
          selectedSceneIndex={selectedSceneIndex}
          currentSceneRange={currentSceneRange}
          outCutEditable={outCutEditable}
          outTransition={outTransition}
          inspectedAssignment={inspectedAssignment}
          inspectedMedia={inspectedMedia}
          inspectedResolved={inspectedResolved}
          activeSubtitle={activeSubtitle}
          subtitleCues={subtitleCues}
          subtitleStyle={subtitleStyle}
          isUpdatingSubtitles={isUpdatingSubtitles}
          stickers={stickers}
          isSavingStickers={isSavingStickers}
          includeTiltInAuto={includeTiltInAuto}
          onUpdateStickers={(next) => void handleUpdateStickers(next)}
          onToggleIncludeTiltInAuto={(next) => void handleToggleIncludeTiltInAuto(next)}
          waveformBars={waveformBars}
          audioFileName={audioFileName}
          musicFilename={musicFilename}
          musicUrl={musicUrl}
          musicSettings={musicSettings}
          isUploadingMusic={isUploadingMusic}
          musicUploadProgress={musicUploadProgress}
          isPauseSnapEnabled={isPauseSnapEnabled}
          snappedFeedback={snappedFeedback}
          onTogglePlay={togglePlay}
          onSeekToMs={seekToMs}
          onJumpScene={jumpScene}
          onSelectScene={setSelectedSceneIndex}
          onSetZoomLevel={setZoomLevel}
          onToggleLoopScene={() => setIsLoopingScene((v) => !v)}
          onToggleMute={() => setIsMuted((v) => !v)}
          onToggleA1Mute={() => setIsA1Muted((v) => !v)}
          onTogglePauseSnap={() => setIsPauseSnapEnabled((v) => !v)}
          onTogglePickFocal={() => setIsPickingFocal((v) => !v)}
          onPickFocalPoint={handlePickFocalPoint}
          onSpecMetaChange={setSpecMeta}
          onRerollEffects={() => void handleRerollEffects()}
          onNudgeOutCut={nudgeSelectedOutCut}
          onOpenTimecodeEditor={openTimecodeEditor}
          onApplyBoundaryMove={applyBoundaryMove}
          onUpdateSceneEffect={(patch) => void handleUpdateSceneEffect(patch)}
          onUpdateOutTransition={handleUpdateOutTransition}
          onAssignMediaInStudio={(mediaId) =>
            void handleAssignMediaInStudio(mediaId)
          }
          onUpdateSubtitleCue={handleUpdateSubtitleCue}
          onSplitSubtitleCue={handleSplitSubtitleCue}
          onMergeSubtitleCues={handleMergeSubtitleCues}
          onUpdateSubtitleStyle={handleUpdateSubtitleStyle}
          onUploadMusicFile={handleUploadMusicFile}
          onUpdateMusicSettings={handleUpdateMusicSettings}
          onDeleteMusic={handleDeleteMusic}
          onOpenExport={onOpenExport}
          onChangeStep={onChangeStep}
          onExitProject={onExitProject}
        />
      ) : (
        <>
      {/* UPPER STUDIO WORKSPACE: CapCut Docked 3-Zone NLE Shell */}
      <div className="w-full px-1.5 sm:px-2.5 pt-1.5 sm:pt-2 pb-1.5 grid grid-cols-1 lg:grid-cols-12 gap-2 items-stretch">
        {/* LEFT PANEL: CapCut Scenes & Media Pool Browser (with 'Added' Badges) */}
        {showSceneDrawer && (
          <aside className="hidden lg:flex lg:col-span-3 flex-col rounded-xl border border-border/70 bg-card overflow-hidden lg:h-[360px] studio-card">
            <div className="flex items-center justify-between border-b border-border/50 px-2.5 py-2 bg-secondary/30">
              <div className="flex items-center rounded-[8px] bg-secondary/80 p-0.5 gap-0.5">
                <button
                  type="button"
                  onClick={() => setLeftDrawerTab("scenes")}
                  className={`inline-flex items-center gap-1.5 rounded-[6px] px-2.5 py-1 text-[11px] font-medium transition-colors ${
                    leftDrawerTab === "scenes"
                      ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-[#140B29] font-semibold"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Film className="h-3 w-3" />
                  <span>Scenes ({sections.length})</span>
                </button>
                <button
                  type="button"
                  onClick={() => setLeftDrawerTab("media")}
                  className={`inline-flex items-center gap-1.5 rounded-[6px] px-2.5 py-1 text-[11px] font-medium transition-colors ${
                    leftDrawerTab === "media"
                      ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-[#140B29] font-semibold"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Images className="h-3 w-3" />
                  <span>Media ({media.length})</span>
                </button>
              </div>
              <button
                type="button"
                onClick={() => setShowSceneDrawer(false)}
                title="Collapse left panel"
                className="rounded-[8px] p-1 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                <PanelLeftClose className="h-3.5 w-3.5" />
              </button>
            </div>

            {leftDrawerTab === "scenes" ? (
              <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
                {sections.map((sec, idx) => {
                  const startMs = boundaries[idx]?.timeMs ?? 0;
                  const endMs = boundaries[idx + 1]?.timeMs ?? totalDurationMs;
                  const durSec = ((endMs - startMs) / 1000).toFixed(1);
                  const assign = assignments.find((a) => a.sectionId === sec.id);
                  const thumb = assign?.mediaId
                    ? media.find((m) => m.id === assign.mediaId)?.thumbUrl
                    : undefined;
                  const isActive = idx === activeSceneIndex;
                  const isSelected = idx === selectedSceneIndex;

                  return (
                    <button
                      key={sec.id}
                      type="button"
                      onClick={() => {
                        setSelectedSceneIndex(idx);
                        seekToMs(startMs);
                      }}
                      className={`w-full flex items-center gap-2.5 rounded-xl border p-1.5 text-left transition-all duration-150 ${
                        isSelected
                          ? "border-brand-lavender bg-brand-lavender/12 ring-1 ring-brand-lavender/35"
                          : isActive
                          ? "border-brand-violet/40 dark:border-brand-lavender/45 bg-brand-violet/5 dark:bg-brand-lavender/8"
                          : "border-transparent bg-secondary/35 hover:bg-secondary/75"
                      }`}
                    >
                      <div className="relative h-10 w-16 shrink-0 overflow-hidden rounded bg-zinc-200 dark:bg-zinc-950 border border-border">
                        {thumb ? (
                          // eslint-disable-next-line @next/next/no-img-element
                          <img
                            src={thumb}
                            alt={`Scene ${idx + 1}`}
                            className="h-full w-full object-cover"
                          />
                        ) : (
                          <div className="flex h-full w-full items-center justify-center text-[10px] font-mono text-muted-foreground">
                            {String(idx + 1).padStart(2, "0")}
                          </div>
                        )}
                        <span className="absolute bottom-0.5 right-0.5 rounded bg-black/85 px-1 font-mono text-[9px] text-zinc-200">
                          {durSec}s
                        </span>
                      </div>

                      <div className="min-w-0 flex-1">
                        <div className="flex items-center justify-between gap-1">
                          <span className="font-mono text-[10px] font-semibold text-foreground">
                            SCENE {String(idx + 1).padStart(2, "0")}
                          </span>
                          <span className="font-mono text-[10px] text-muted-foreground">
                            {formatTimestampPrecise(startMs).slice(0, 5)}
                          </span>
                        </div>
                        <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                          {sec.text || "Empty scene"}
                        </p>
                      </div>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="flex-1 flex flex-col min-h-0">
                <div className="border-b border-border/50 bg-secondary/20 px-3 py-1.5 flex items-center justify-between text-[10px] font-mono">
                  <span className="text-muted-foreground">
                    Target:{" "}
                    <strong className="text-brand-violet dark:text-brand-lavender">
                      Scene S{String(selectedSceneIndex + 1).padStart(2, "0")}
                    </strong>
                  </span>
                  <span className="text-muted-foreground/80">
                    Click image to assign
                  </span>
                </div>

                {media.length === 0 ? (
                  <div className="flex-1 flex items-center justify-center p-4 text-center font-mono text-xs text-muted-foreground">
                    No media uploaded yet. Upload images in the Media step.
                  </div>
                ) : (
                  <div className="flex-1 overflow-y-auto p-2 grid grid-cols-2 gap-2 content-start">
                    {media.map((item) => {
                      const assignedIndices: number[] = [];
                      sections.forEach((sec, sIdx) => {
                        const a = assignments.find((x) => x.sectionId === sec.id);
                        if (a?.mediaId === item.id) {
                          assignedIndices.push(sIdx + 1);
                        }
                      });
                      const isAdded = assignedIndices.length > 0;
                      const isAssignedToCurrent =
                        inspectedAssignment?.mediaId === item.id;

                      return (
                        <button
                          key={item.id}
                          type="button"
                          onClick={() => void handleAssignMediaInStudio(item.id)}
                          title={`Assign ${item.filename} to Scene S${String(
                            selectedSceneIndex + 1
                          ).padStart(2, "0")}`}
                          className="group/mcard flex flex-col text-left focus:outline-none"
                        >
                          <div
                            className={`relative aspect-video w-full overflow-hidden rounded-md border bg-zinc-950 transition-all ${
                              isAssignedToCurrent
                                ? "border-brand-lavender ring-2 ring-brand-lavender/50"
                                : "border-border/80 group-hover/mcard:border-brand-lavender/75"
                            }`}
                          >
                            {/* eslint-disable-next-line @next/next/no-img-element */}
                            <img
                              src={item.thumbUrl}
                              alt={item.filename}
                              className="h-full w-full object-cover transition-transform duration-200 group-hover/mcard:scale-105"
                              loading="lazy"
                            />

                            {/* CapCut Top-Left 'Added' Badge */}
                            {isAdded && (
                              <span
                                className={`absolute top-1 left-1 inline-flex items-center gap-0.5 rounded-[3px] px-1.5 py-0.5 font-mono text-[8.5px] font-bold shadow-sm ${
                                  isAssignedToCurrent
                                    ? "bg-brand-lavender text-[#140B29]"
                                    : "bg-black/85 text-zinc-100 border border-white/15"
                                }`}
                              >
                                {isAssignedToCurrent && (
                                  <Check className="h-2.5 w-2.5" />
                                )}
                                Added · S
                                {String(assignedIndices[0]).padStart(2, "0")}
                                {assignedIndices.length > 1
                                  ? ` +${assignedIndices.length - 1}`
                                  : ""}
                              </span>
                            )}

                            {/* Hover Action Pill */}
                            <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 via-black/60 to-transparent px-1.5 py-1 opacity-0 group-hover/mcard:opacity-100 transition-opacity">
                              <span className="font-mono text-[9px] font-semibold text-brand-lavender">
                                {isAssignedToCurrent
                                  ? `Active on S${String(
                                      selectedSceneIndex + 1
                                    ).padStart(2, "0")}`
                                  : `Use on S${String(
                                      selectedSceneIndex + 1
                                    ).padStart(2, "0")}`}
                              </span>
                            </div>
                          </div>

                          <span className="mt-1 truncate font-mono text-[10px] text-muted-foreground group-hover/mcard:text-foreground">
                            {item.filename}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            )}
          </aside>
        )}

        {/* CENTER MONITOR: Preview Canvas & Transport */}
        {isMonitorFullscreen && (
          <div
            onClick={() => setIsMonitorFullscreen(false)}
            className="fixed inset-0 z-40 bg-black/80 backdrop-blur-sm"
          />
        )}
        <section
          className={`${
            isMonitorFullscreen
              ? "fixed inset-3 sm:inset-6 z-50 flex flex-col rounded-2xl border border-brand-lavender/45 bg-card overflow-hidden shadow-2xl"
              : `${
                  showSceneDrawer ? "lg:col-span-6" : "lg:col-span-9"
                } flex flex-col rounded-xl border border-border/70 bg-card overflow-hidden lg:h-[360px] studio-card`
          }`}
        >
          {/* Monitor Top Bar */}
          <div className="flex items-center justify-between gap-2 border-b border-border/50 bg-secondary/30 px-3.5 py-2">
            <div className="flex items-center gap-2 min-w-0">
              {!showSceneDrawer && (
                <button
                  type="button"
                  onClick={() => setShowSceneDrawer(true)}
                  title="Open scene list"
                  className="hidden lg:inline-flex rounded-[8px] p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
                >
                  <PanelLeftOpen className="h-3.5 w-3.5" />
                </button>
              )}
              <span className="text-xs font-semibold text-foreground">
                Monitor
              </span>
              <Badge variant="secondary" className="font-mono">{projectAspectRatio}</Badge>
              <span className="font-mono text-[11px] text-muted-foreground truncate">
                Scene {String(monitorSceneIndex + 1).padStart(2, "0")}
                {activeResolved?.preset || activeAssignment?.resolvedPreset
                  ? ` · ${formatPresetLabel(
                    activeResolved?.preset || activeAssignment?.resolvedPreset
                  )}`
                  : ""}
              </span>
            </div>

            <div className="flex items-center gap-1 shrink-0">
              <button
                type="button"
                disabled={isRerolling || sections.length === 0}
                onClick={handleRerollEffects}
                title={`Re-roll automatic camera motion (Seed #${effectSeed})`}
                className="rounded-[8px] bg-secondary/70 px-2.5 py-1 text-xs font-medium text-foreground hover:bg-secondary disabled:opacity-40 flex items-center gap-1 transition-colors"
              >
                <Shuffle className={`h-3 w-3 text-brand-violet dark:text-brand-lavender ${isRerolling ? "animate-spin" : ""}`} />
                <span className="hidden sm:inline">Re-roll</span>
                <span className="font-mono text-[10px] text-muted-foreground">
                  #{effectSeed}
                </span>
              </button>

              <button
                type="button"
                onClick={() => setShowSafeGrid((v) => !v)}
                title="Toggle composition grid"
                className={`rounded-[8px] px-2 py-1 text-xs font-medium flex items-center gap-1 transition-colors ${showSafeGrid
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-[#140B29] font-semibold"
                  : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
              >
                <Grid className="h-3 w-3" />
                <span className="hidden sm:inline">Grid</span>
              </button>

              <button
                type="button"
                onClick={() => setIsMuted((m) => !m)}
                title={isMuted ? "Unmute audio" : "Mute audio"}
                className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                {isMuted ? (
                  <VolumeX className="h-3.5 w-3.5 text-red-400" />
                ) : (
                  <Volume2 className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
          </div>

          {/* Matte Black WASM Preview Canvas */}
          <div
            className={`relative flex-1 min-h-0 overflow-hidden bg-[#050507] flex items-center justify-center p-2 sm:p-3 ${
              isMonitorFullscreen ? "min-h-[60vh]" : "min-h-[220px]"
            }`}
          >
            <PreviewPlayer
              aspectRatio={projectAspectRatio}
              fps={compiledRenderSpec?.fps ?? 30}
              currentTimeMs={currentTimeMs}
              durationMs={totalDurationMs}
              effectSeed={effectSeed}
              sections={sections}
              boundaries={boundaries}
              assignments={assignments}
              media={media}
              compiledRenderSpec={compiledRenderSpec}
              preferLocalSpec={preferLocalSpec}
              activeSceneIndex={monitorSceneIndex}
              inspectedSceneIndex={selectedSceneIndex}
              activeSubtitle={activeSubtitle}
              subtitleStyle={subtitleStyle}
              stickers={stickers}
              includeTiltInAuto={includeTiltInAuto}
              showSafeGrid={showSafeGrid}
              isPickingFocal={isPickingFocal}
              inspectedFocal={{
                x: inspectedAssignment?.focalX ?? 0.5,
                y: inspectedAssignment?.focalY ?? 0.5,
              }}
              onPickFocalPoint={handlePickFocalPoint}
              onSpecMetaChange={setSpecMeta}
              isFullscreen={isMonitorFullscreen}
            />
          </div>

          {/* Monitor Transport Controls (CapCut Player Footer) */}
          <div className="flex items-center justify-between gap-1.5 sm:gap-2 overflow-x-auto border-t border-border/50 bg-secondary/25 px-2.5 sm:px-3.5 py-1.5 sm:py-2">
            {/* Timecode Readout */}
            <div className="flex items-center gap-1.5 font-mono text-xs tabular-nums">
              <span className="font-semibold text-brand-violet dark:text-brand-lavender">
                {formatTimestampPrecise(currentTimeMs)}
              </span>
              <span className="text-muted-foreground/60">/</span>
              <span className="text-muted-foreground">
                {formatTimestampPrecise(totalDurationMs)}
              </span>
            </div>

            {/* Center Playback Controls */}
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => jumpScene(-1)}
                disabled={selectedSceneIndex === 0}
                title="Previous scene"
                className="rounded-[8px] p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35"
              >
                <SkipBack className="h-3.5 w-3.5" />
              </button>

              <button
                type="button"
                onClick={() => stepFrame(-1)}
                title="Back 1 frame"
                className="rounded-[8px] px-2 py-1 font-mono text-[11px] text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                -1f
              </button>

              <button
                type="button"
                onClick={togglePlay}
                aria-label={isPlaying ? "Pause playback" : "Play timeline"}
                className="flex h-8 w-10 items-center justify-center rounded-[10px] bg-brand-violet text-white dark:bg-brand-lavender dark:text-[#140B29] hover:opacity-90 active:scale-95 transition-all"
              >
                {isPlaying ? (
                  <Pause className="h-4 w-4 fill-current" />
                ) : (
                  <Play className="h-4 w-4 fill-current ml-0.5" />
                )}
              </button>

              <button
                type="button"
                onClick={() => stepFrame(1)}
                title="Forward 1 frame"
                className="rounded px-1.5 py-1 font-mono text-[11px] text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                +1f
              </button>

              <button
                type="button"
                onClick={() => jumpScene(1)}
                disabled={selectedSceneIndex >= sections.length - 1}
                title="Next scene"
                className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35"
              >
                <SkipForward className="h-3.5 w-3.5" />
              </button>
            </div>

            {/* Right Loop, Ratio & Fullscreen Controls */}
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => setIsLoopingScene((v) => !v)}
                className={`inline-flex items-center gap-1 rounded border px-2 py-1 text-[11px] font-medium transition-colors ${
                  isLoopingScene
                    ? "border-brand-lavender bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                    : "border-border bg-card text-muted-foreground hover:text-foreground"
                }`}
              >
                <Repeat className="h-3 w-3" />
                <span className="hidden sm:inline">Loop</span>
              </button>

              <span className="hidden md:inline-flex items-center rounded border border-border bg-card px-1.5 py-1 font-mono text-[10px] text-muted-foreground">
                {projectAspectRatio}
              </span>

              <button
                type="button"
                onClick={() => setIsMonitorFullscreen((v) => !v)}
                title={
                  isMonitorFullscreen
                    ? "Exit fullscreen preview"
                    : "Fullscreen cinema preview"
                }
                aria-label={
                  isMonitorFullscreen
                    ? "Exit fullscreen preview"
                    : "Fullscreen cinema preview"
                }
                className={`inline-flex items-center justify-center rounded border p-1.5 transition-colors ${
                  isMonitorFullscreen
                    ? "border-brand-lavender bg-brand-lavender text-[#140B29]"
                    : "border-border bg-card text-muted-foreground hover:bg-secondary hover:text-foreground"
                }`}
              >
                {isMonitorFullscreen ? (
                  <Minimize2 className="h-3.5 w-3.5" />
                ) : (
                  <Maximize2 className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
          </div>
        </section>

        {/* RIGHT PANEL: Desktop Scene Effects, Timing, Subtitles & Music Inspector */}
        <aside className="hidden lg:flex lg:col-span-3 flex-col rounded-xl border border-border/70 bg-card overflow-hidden lg:h-[360px] studio-card">
          <div className="flex items-center justify-between gap-1 border-b border-border/50 bg-secondary/30 px-3 py-2">
            <span className="font-mono text-xs font-semibold text-foreground shrink-0">
              S{String(selectedSceneIndex + 1).padStart(2, "0")}
            </span>

            <div className="flex items-center rounded-[10px] bg-secondary/80 p-0.5">
              <button
                type="button"
                onClick={() => setInspectorTab("effects")}
                className={`inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[10px] font-medium transition-colors ${inspectorTab === "effects"
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                  : "text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Sliders className="h-2.5 w-2.5" />
                Motion
              </button>
              <button
                type="button"
                onClick={() => setInspectorTab("stickers")}
                className={`inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[10px] font-medium transition-colors ${inspectorTab === "stickers"
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                  : "text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Sticker className="h-2.5 w-2.5" />
                Stickers
              </button>
              <button
                type="button"
                onClick={() => setInspectorTab("timing")}
                className={`inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[10px] font-medium transition-colors ${inspectorTab === "timing"
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                  : "text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Clock className="h-2.5 w-2.5" />
                Timing
              </button>
              <button
                type="button"
                onClick={() => setInspectorTab("subtitles")}
                className={`inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[10px] font-medium transition-colors ${inspectorTab === "subtitles"
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                  : "text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Captions className="h-2.5 w-2.5" />
                Subs
              </button>
              <button
                type="button"
                onClick={() => setInspectorTab("music")}
                className={`inline-flex items-center gap-1 rounded-[4px] px-1.5 py-0.5 text-[10px] font-medium transition-colors ${inspectorTab === "music"
                  ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                  : "text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Music className="h-2.5 w-2.5" />
                Music
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-3 space-y-3.5 text-xs">
            {inspectorTab === "effects" ? (
              <div className="space-y-3">
                <label className="flex items-center justify-between rounded-md border border-border bg-secondary/30 px-2.5 py-2 text-[11px] text-foreground cursor-pointer">
                  <span>Include tilted presets in Auto Re-roll</span>
                  <input
                    type="checkbox"
                    checked={includeTiltInAuto}
                    onChange={(e) => void handleToggleIncludeTiltInAuto(e.target.checked)}
                    className="h-3.5 w-3.5 accent-brand-lavender"
                  />
                </label>
                <SceneEffectsPanel
                  sceneIndex={selectedSceneIndex}
                  assignment={inspectedAssignment}
                  hasAssignedMedia={Boolean(inspectedMedia || inspectedAssignment?.mediaId)}
                  resolvedPreset={
                    inspectedResolved?.preset ??
                    inspectedAssignment?.resolvedPreset ??
                    "still"
                  }
                  resolvedHasShake={
                    inspectedResolved?.hasShake ??
                    inspectedAssignment?.resolvedHasShake ??
                    false
                  }
                  isPickingFocal={isPickingFocal}
                  outCutEditable={outCutEditable}
                  outTransition={outTransition}
                  isSavingEffect={isSavingEffect}
                  onTogglePickFocal={() => setIsPickingFocal((v) => !v)}
                  onUpdateEffect={handleUpdateSceneEffect}
                  onUpdateOutTransition={handleUpdateOutTransition}
                />
              </div>
            ) : inspectorTab === "stickers" ? (
              <StickersPanel
                stickers={stickers}
                currentTimeMs={currentTimeMs}
                totalDurationMs={totalDurationMs}
                isSaving={isSavingStickers}
                onChangeStickers={(next) => void handleUpdateStickers(next)}
                onSeekToMs={seekToMs}
              />
            ) : inspectorTab === "subtitles" ? (
              <SubtitlesInspectorPanel
                aspectRatio={projectAspectRatio}
                cues={subtitleCues}
                activeCueId={activeSubtitle?.id}
                style={subtitleStyle}
                isBusy={isUpdatingSubtitles}
                onSeekToMs={seekToMs}
                onUpdateCue={handleUpdateSubtitleCue}
                onSplitCue={handleSplitSubtitleCue}
                onMergeWithNext={handleMergeSubtitleCues}
                onUpdateStyle={handleUpdateSubtitleStyle}
              />
            ) : inspectorTab === "music" ? (
              <MusicInspectorPanel
                filename={musicFilename}
                audioUrl={musicUrl}
                settings={musicSettings}
                isUploading={isUploadingMusic}
                uploadProgress={musicUploadProgress}
                onUploadMusicFile={handleUploadMusicFile}
                onUpdateSettings={handleUpdateMusicSettings}
                onDeleteMusic={handleDeleteMusic}
              />
            ) : (
              <>
                {/* Time Bounds */}
                <div className="grid grid-cols-2 gap-2">
                  <div className="rounded border border-border bg-secondary/35 p-2">
                    <div className="text-[10px] font-mono uppercase text-muted-foreground">
                      In Cut
                    </div>
                    <div className="mt-0.5 font-mono text-xs font-semibold text-foreground">
                      {formatTimestampPrecise(currentSceneRange.start)}
                    </div>
                  </div>
                  <div className="rounded border border-border bg-secondary/35 p-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] font-mono uppercase text-muted-foreground">
                        Out Cut
                      </span>
                      {outCutEditable && (
                        <button
                          type="button"
                          onClick={() => openTimecodeEditor(selectedSceneIndex + 1)}
                          className="inline-flex items-center gap-0.5 text-[10px] font-medium text-brand-violet dark:text-brand-lavender hover:underline"
                        >
                          <Edit3 className="h-2.5 w-2.5" />
                          Edit
                        </button>
                      )}
                    </div>
                    <div className="mt-0.5 font-mono text-xs font-semibold text-brand-violet dark:text-brand-lavender">
                      {formatTimestampPrecise(currentSceneRange.end)}
                    </div>
                  </div>
                </div>

                {/* Precision Trim Buttons */}
                <div className="space-y-1.5">
                  <div className="flex items-center justify-between text-[11px] text-muted-foreground">
                    <span>Trim Out Boundary</span>
                    <span className="font-mono">
                      {((currentSceneRange.end - currentSceneRange.start) / 1000).toFixed(2)}s
                    </span>
                  </div>
                  <div className="grid grid-cols-4 gap-1.5">
                    {[-500, -100, 100, 500].map((delta) => (
                      <button
                        key={delta}
                        type="button"
                        disabled={!outCutEditable}
                        onClick={() => nudgeSelectedOutCut(delta)}
                        className="rounded border border-border bg-secondary py-1.5 font-mono text-[11px] text-foreground hover:border-brand-lavender/70 disabled:opacity-35 transition-colors"
                      >
                        {delta > 0 ? `+${delta / 1000}s` : `${delta / 1000}s`}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Visual Framing Summary */}
                <div className="rounded border border-border bg-secondary/25 p-2.5 space-y-1">
                  <div className="text-[10px] font-mono uppercase text-muted-foreground">
                    Assigned Asset
                  </div>
                  <div className="font-mono text-[11px] text-foreground truncate">
                    {inspectedMedia ? inspectedMedia.filename : "Unassigned"}
                  </div>
                  {inspectedAssignment && (
                    <div className="font-mono text-[10px] text-muted-foreground">
                      Mode: {inspectedAssignment.fitMode === "fit_blur" ? "Fit Blur" : "Fill"} ·
                      Focal {Math.round(inspectedAssignment.focalX * 100)}%,{" "}
                      {Math.round(inspectedAssignment.focalY * 100)}%
                    </div>
                  )}
                </div>

                {/* Scene Script Text */}
                <div className="space-y-1">
                  <div className="text-[10px] font-mono uppercase text-muted-foreground">
                    Narration Script
                  </div>
                  <p className="rounded border border-border bg-background/60 p-2.5 text-xs leading-relaxed text-foreground/90">
                    {inspectedSection?.text || "No script text for this scene."}
                  </p>
                </div>
              </>
            )}
          </div>
        </aside>
      </div>

      {/* LOWER FULL-WIDTH MULTI-TRACK MAGNETIC TIMELINE (CapCut Docked Deck) */}
      <div className="w-full px-1.5 sm:px-2.5 pb-2 flex-1 flex flex-col">
        <div className="flex flex-col rounded-xl border border-border/70 bg-card overflow-hidden studio-card">
          {/* CapCut Middle NLE Action Toolbar */}
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-secondary/35 px-2.5 sm:px-3.5 py-1.5">
            {/* Left Action Group: Active Scene Trim, Pause Snap, Follow, Flagged Cuts */}
            <div className="flex items-center gap-1.5 flex-wrap">
              <div className="inline-flex items-center rounded-md border border-border/80 bg-card p-0.5 gap-0.5">
                <span className="px-2 py-0.5 font-mono text-[10px] font-bold text-brand-violet dark:text-brand-lavender">
                  S{String(selectedSceneIndex + 1).padStart(2, "0")}
                </span>
                <button
                  type="button"
                  disabled={!outCutEditable}
                  onClick={() => nudgeSelectedOutCut(-100)}
                  title="Trim scene out-cut -0.1s"
                  className="rounded px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35 transition-colors"
                >
                  -0.1s
                </button>
                <button
                  type="button"
                  disabled={!outCutEditable}
                  onClick={() => nudgeSelectedOutCut(100)}
                  title="Trim scene out-cut +0.1s"
                  className="rounded px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35 transition-colors"
                >
                  +0.1s
                </button>
                <button
                  type="button"
                  disabled={!outCutEditable}
                  onClick={() => openTimecodeEditor(selectedSceneIndex + 1)}
                  title="Type exact cut timecode (mm:ss.mmm)"
                  className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35 transition-colors"
                >
                  <Edit3 className="h-3 w-3" />
                </button>
              </div>

              <span className="hidden sm:inline-block h-4 w-px bg-border/70 mx-0.5" />

              <button
                type="button"
                onClick={() => setIsPauseSnapEnabled((v) => !v)}
                title="Magnetic snap to speech pauses (hold Alt while dragging to invert)"
                className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors ${isPauseSnapEnabled
                  ? "border-brand-lavender/50 bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                  : "border-border bg-card text-muted-foreground hover:text-foreground"
                  }`}
              >
                <Magnet className="h-3 w-3" />
                <span>Pause Snap</span>
              </button>

              <button
                type="button"
                onClick={() => setIsAutoFollowEnabled((v) => !v)}
                title="Auto-scroll timeline to follow playhead during playback"
                className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors ${isAutoFollowEnabled
                  ? "border-brand-lavender/50 bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                  : "border-border bg-card text-muted-foreground hover:text-foreground"
                  }`}
              >
                <LocateFixed className="h-3 w-3" />
                <span className="hidden sm:inline">Follow</span>
              </button>

              {flaggedCuts.length > 0 && (
                <button
                  type="button"
                  onClick={handleJumpNextFlaggedCut}
                  title="Jump to next flagged low-confidence cut"
                  className="inline-flex items-center gap-1 rounded-md border border-amber-500/45 bg-amber-500/15 px-2 py-1 font-mono text-[10px] font-semibold text-amber-600 dark:text-amber-300 hover:bg-amber-500/25 transition-colors"
                >
                  <AlertTriangle className="h-3 w-3" />
                  <span>{flaggedCuts.length} Flagged</span>
                </button>
              )}

              {snappedFeedback && (
                <span className="rounded-md bg-brand-violet/10 dark:bg-brand-lavender/15 px-2 py-0.5 font-mono text-[10px] font-semibold text-brand-violet dark:text-brand-lavender">
                  {snappedFeedback}
                </span>
              )}
            </div>

            {/* Right Action Group: Fit to Screen & Continuous CapCut Zoom Slider */}
            <div className="flex items-center gap-2 ml-auto">
              <span className="hidden 2xl:inline font-mono text-[10px] text-muted-foreground">
                Space Play · ←/→ 1f · Double-click cut to type timecode
              </span>

              <button
                type="button"
                onClick={handleFitToTimeline}
                title="Fit entire video duration into timeline view"
                className="inline-flex items-center gap-1 rounded-md border border-border bg-card px-2 py-1 font-mono text-[10px] font-medium text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                <Maximize2 className="h-3 w-3" />
                <span>Fit</span>
              </button>

              <div className="flex items-center gap-1.5 rounded-md border border-border bg-card px-2 py-1">
                <button
                  type="button"
                  onClick={() =>
                    setZoomLevel((z) =>
                      Math.max(0.15, Number((z - 0.25).toFixed(2)))
                    )
                  }
                  title="Zoom out timeline"
                  className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                >
                  <ZoomOut className="h-3.5 w-3.5" />
                </button>

                <input
                  type="range"
                  aria-label="Timeline zoom slider"
                  min={0.15}
                  max={4}
                  step={0.05}
                  value={zoomLevel}
                  onChange={(e) => setZoomLevel(parseFloat(e.target.value))}
                  className="hidden sm:block w-24 h-1.5 accent-brand-violet dark:accent-brand-lavender cursor-pointer"
                />

                <button
                  type="button"
                  onClick={() => setZoomLevel(1)}
                  title="Reset zoom to 100%"
                  className="min-w-[38px] text-center font-mono text-[10px] font-semibold text-foreground hover:text-brand-violet dark:hover:text-brand-lavender tabular-nums"
                >
                  {Math.round(zoomLevel * 100)}%
                </button>

                <button
                  type="button"
                  onClick={() =>
                    setZoomLevel((z) =>
                      Math.min(4, Number((z + 0.25).toFixed(2)))
                    )
                  }
                  title="Zoom in timeline"
                  className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                >
                  <ZoomIn className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          </div>

          {/* Multi-Track Strip Container */}
          <div className="flex relative overflow-hidden">
            {/* Fixed Left Track Headers with Interactive CapCut Toggles (Eye / Lock / Mute) */}
            <div className="w-20 sm:w-24 shrink-0 border-r border-border bg-secondary/45 z-20 flex flex-col">
              <div className="h-7 border-b border-border flex items-center justify-center font-mono text-[10px] text-muted-foreground">
                TIME
              </div>

              {/* T1 Captions Track Header (Top row under Ruler, matching CapCut) */}
              <div className="h-9 border-b border-border px-2 flex items-center justify-between text-muted-foreground">
                <div className="flex items-center gap-1">
                  <Captions className="h-3 w-3 text-brand-violet dark:text-brand-lavender shrink-0" />
                  <span className="font-mono text-[10px] font-semibold text-foreground">
                    T1
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    const nextEnabled = !subtitleStyle.enabled;
                    void handleUpdateSubtitleStyle({
                      ...subtitleStyle,
                      enabled: nextEnabled,
                    });
                    setSnappedFeedback(
                      nextEnabled
                        ? "T1 Captions visible in preview"
                        : "T1 Captions hidden"
                    );
                    setTimeout(() => setSnappedFeedback(null), 1500);
                  }}
                  title={
                    subtitleStyle.enabled
                      ? "Hide T1 captions in video preview"
                      : "Show T1 captions in video preview"
                  }
                  className={`rounded p-1 transition-colors ${subtitleStyle.enabled
                    ? "text-foreground hover:bg-secondary"
                    : "bg-red-500/15 text-red-400 hover:bg-red-500/25"
                    }`}
                >
                  {subtitleStyle.enabled ? (
                    <Eye className="h-3 w-3" />
                  ) : (
                    <EyeOff className="h-3 w-3" />
                  )}
                </button>
              </div>

              {/* V1 Video Track Header */}
              <div className="h-[68px] border-b border-border px-2 flex items-center justify-between text-muted-foreground">
                <div className="flex items-center gap-1">
                  <Film className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender shrink-0" />
                  <span className="font-mono text-[10px] font-semibold text-foreground">
                    V1
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setIsV1Locked((prev) => {
                      const next = !prev;
                      setSnappedFeedback(
                        next
                          ? "V1 track locked (cuts protected)"
                          : "V1 track unlocked"
                      );
                      setTimeout(() => setSnappedFeedback(null), 1500);
                      return next;
                    });
                  }}
                  title={
                    isV1Locked
                      ? "Unlock V1 track to allow cut adjustments"
                      : "Lock V1 track to prevent accidental cut moves"
                  }
                  className={`rounded p-1 transition-colors ${isV1Locked
                    ? "bg-amber-500/20 text-amber-400 hover:bg-amber-500/30"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    }`}
                >
                  {isV1Locked ? (
                    <Lock className="h-3 w-3" />
                  ) : (
                    <Unlock className="h-3 w-3" />
                  )}
                </button>
              </div>

              {/* A1 Voiceover Track Header */}
              <div className="h-14 border-b border-border px-2 flex items-center justify-between text-muted-foreground">
                <div className="flex items-center gap-1">
                  <Volume2 className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />
                  <span className="font-mono text-[10px] font-semibold text-foreground">
                    A1
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setIsA1Muted((prev) => {
                      const next = !prev;
                      setSnappedFeedback(
                        next ? "A1 Voiceover muted" : "A1 Voiceover unmuted"
                      );
                      setTimeout(() => setSnappedFeedback(null), 1500);
                      return next;
                    });
                  }}
                  title={
                    isA1Muted
                      ? "Unmute A1 voiceover track"
                      : "Mute A1 voiceover track"
                  }
                  className={`rounded p-1 transition-colors ${isA1Muted
                    ? "bg-red-500/15 text-red-400 hover:bg-red-500/25"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    }`}
                >
                  {isA1Muted ? (
                    <VolumeX className="h-3 w-3" />
                  ) : (
                    <Volume2 className="h-3 w-3" />
                  )}
                </button>
              </div>

              {/* A2 Background Music Track Header */}
              <div className="h-12 px-2 flex items-center justify-between text-muted-foreground">
                <div className="flex items-center gap-1">
                  <Music className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender shrink-0" />
                  <span className="font-mono text-[10px] font-semibold text-foreground">
                    A2
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setIsA2Muted((prev) => {
                      const next = !prev;
                      setSnappedFeedback(
                        next
                          ? "A2 Background music muted"
                          : "A2 Background music unmuted"
                      );
                      setTimeout(() => setSnappedFeedback(null), 1500);
                      return next;
                    });
                  }}
                  title={
                    isA2Muted
                      ? "Unmute A2 background music track"
                      : "Mute A2 background music track"
                  }
                  className={`rounded p-1 transition-colors ${isA2Muted
                    ? "bg-red-500/15 text-red-400 hover:bg-red-500/25"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                    }`}
                >
                  {isA2Muted ? (
                    <VolumeX className="h-3 w-3" />
                  ) : (
                    <Volume2 className="h-3 w-3" />
                  )}
                </button>
              </div>
            </div>

            {/* Horizontally Scrollable Tracks Area */}
            <div
              ref={timelineTrackRef}
              onClick={handleTrackScrub}
              className="relative flex-1 overflow-x-auto bg-zinc-100 dark:bg-[#0A0A0E] cursor-pointer"
            >
              <div
                style={{ width: `${totalWidthPx}px` }}
                className="relative min-w-full"
              >
                {/* RULER TRACK (Click & Drag Scrubbing + Hover Timecode Guide) */}
                <div
                  onPointerDown={handleRulerPointerDown}
                  onPointerMove={(e) => setHoverTimeMs(clientXToTimeMs(e.clientX))}
                  onPointerLeave={() => setHoverTimeMs(null)}
                  className="relative h-7 border-b border-border bg-secondary/70 dark:bg-secondary/25 cursor-ew-resize touch-none"
                >
                  {timeTicks.map((tickMs) => (
                    <div
                      key={tickMs}
                      className="absolute top-0 bottom-0 border-l border-border/80 pl-1 pt-1 font-mono text-[10px] text-muted-foreground tabular-nums pointer-events-none"
                      style={{ left: `${(tickMs / 1000) * pxPerSec}px` }}
                    >
                      {formatTimestampPrecise(tickMs).slice(0, 5)}
                    </div>
                  ))}
                </div>

                {/* T1 CAPTIONS / SUBTITLE CUES TRACK (Top row under Ruler, CapCut Pill Style) */}
                <div
                  className={`relative h-9 border-b border-border bg-zinc-200/80 dark:bg-[#0D0B16] transition-opacity ${subtitleStyle.enabled ? "opacity-100" : "opacity-40"
                    }`}
                >
                  {subtitleCues.length === 0 ? (
                    <div className="flex h-full items-center px-3 font-mono text-[10px] text-muted-foreground">
                      Run alignment in Narration to populate T1 caption pills
                    </div>
                  ) : (
                    subtitleCues.map((cue) => {
                      const leftPx = (cue.start_ms / 1000) * pxPerSec;
                      const widthPx = Math.max(
                        10,
                        ((cue.end_ms - cue.start_ms) / 1000) * pxPerSec
                      );
                      const isActive = activeSubtitle?.id === cue.id;
                      return (
                        <button
                          key={cue.id}
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            seekToMs(cue.start_ms);
                            setInspectorTab("subtitles");
                          }}
                          style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                          title={cue.text}
                          className={`absolute top-1 bottom-1 flex items-center gap-1 overflow-hidden rounded-[4px] border px-1.5 text-left text-[10px] font-medium transition-all ${isActive
                            ? "border-brand-lavender bg-brand-violet text-white dark:bg-brand-lavender/30 dark:text-white font-semibold z-10 shadow-[0_0_10px_rgba(169,112,255,0.3)]"
                            : "border-brand-lavender/35 bg-[#2A1B45]/90 text-purple-100 hover:border-brand-lavender/80 hover:bg-[#352157]"
                            }`}
                        >
                          {widthPx > 32 && (
                            <span className="shrink-0 rounded-[2px] bg-brand-lavender/25 px-1 py-[0.5px] font-mono text-[8px] font-bold text-brand-lavender">
                              AT
                            </span>
                          )}
                          <span className="truncate font-sans text-[10px]">
                            {cue.text}
                          </span>
                        </button>
                      );
                    })
                  )}
                </div>

                {/* V1 VIDEO SCENE STRIP (CapCut Repeating Filmstrip & Seam Transitions) */}
                <div
                  className={`relative h-[68px] border-b border-border bg-zinc-200/75 dark:bg-[#0B0913] transition-opacity ${isV1Locked ? "opacity-90" : "opacity-100"
                    }`}
                >
                  {sections.map((sec, idx) => {
                    const startMs = boundaries[idx]?.timeMs ?? 0;
                    const endMs = boundaries[idx + 1]?.timeMs ?? totalDurationMs;
                    const leftPx = (startMs / 1000) * pxPerSec;
                    const widthPx = Math.max(8, ((endMs - startMs) / 1000) * pxPerSec);
                    const durSec = ((endMs - startMs) / 1000).toFixed(1);

                    const assign = assignments.find((a) => a.sectionId === sec.id);
                    const assignedMediaItem = assign?.mediaId
                      ? media.find((m) => m.id === assign.mediaId)
                      : undefined;
                    const thumb = assignedMediaItem?.thumbUrl;
                    const clipTitle =
                      assignedMediaItem?.filename ||
                      sec.text?.slice(0, 28) ||
                      `Scene ${idx + 1}`;
                    const isSelected = idx === selectedSceneIndex;
                    const isActivePlayhead = idx === activeSceneIndex;

                    return (
                      <div
                        key={sec.id}
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedSceneIndex(idx);
                          seekToMs(startMs);
                        }}
                        style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                        className={`group/clip absolute top-1 bottom-1 flex flex-col overflow-hidden rounded-[5px] border transition-all ${thumb ? "bg-zinc-950" : "bg-card dark:bg-zinc-900"
                          } ${isSelected
                            ? "border-brand-lavender ring-1 ring-brand-lavender/70 z-10 shadow-[0_0_12px_rgba(169,112,255,0.25)]"
                            : isActivePlayhead
                              ? "border-brand-lavender/60"
                              : "border-border dark:border-zinc-700/90 hover:border-brand-violet dark:hover:border-brand-lavender/75"
                          }`}
                      >
                        {/* CapCut Top Clip Header Bar */}
                        <div
                          className={`h-4 shrink-0 px-1.5 flex items-center justify-between gap-1 border-b ${isSelected
                            ? "bg-brand-violet/90 dark:bg-brand-lavender/25 border-brand-lavender/40 text-white"
                            : "bg-[#141022]/90 border-white/10 text-zinc-200"
                            }`}
                        >
                          <div className="flex items-center gap-1 min-w-0">
                            <span className="font-mono text-[9.5px] font-bold text-brand-lavender shrink-0">
                              S{String(idx + 1).padStart(2, "0")}
                            </span>
                            {widthPx > 54 && (
                              <span className="font-mono text-[9px] text-zinc-200/90 truncate">
                                {clipTitle}
                              </span>
                            )}
                          </div>
                          {widthPx > 42 && (
                            <span className="font-mono text-[9px] text-zinc-300 tabular-nums shrink-0">
                              {durSec}s
                            </span>
                          )}
                        </div>

                        {/* Repeating Horizontal Filmstrip Body */}
                        <div className="relative flex-1 w-full overflow-hidden bg-zinc-950">
                          {thumb ? (
                            <>
                              <div
                                className="absolute inset-0 opacity-85 transition-opacity group-hover/clip:opacity-100"
                                style={{
                                  backgroundImage: `url("${thumb}")`,
                                  backgroundRepeat: "repeat-x",
                                  backgroundSize: "auto 100%",
                                  backgroundPosition: "left center",
                                }}
                              />
                              {/* Subtle Film Frame Separators */}
                              <div
                                className="pointer-events-none absolute inset-0"
                                style={{
                                  backgroundImage:
                                    "repeating-linear-gradient(90deg, transparent, transparent 66px, rgba(10, 8, 18, 0.55) 66px, rgba(10, 8, 18, 0.55) 68px)",
                                }}
                              />
                            </>
                          ) : (
                            <div className="flex h-full w-full items-center px-2 bg-secondary/30">
                              <span className="font-mono text-[9px] text-muted-foreground truncate">
                                No image assigned
                              </span>
                            </div>
                          )}

                          {/* Bottom Script Snippet Overlay on Wide Clips */}
                          {widthPx > 110 && sec.text && (
                            <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/85 via-black/45 to-transparent px-1.5 py-0.5">
                              <p className="truncate text-[9px] text-zinc-200/90 font-medium">
                                {sec.text}
                              </p>
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}

                  {/* Draggable Cut Dividers & Clickable CapCut Transition Seam Badges */}
                  {boundaries.slice(1, -1).map((b) => {
                    const leftPx = (b.timeMs / 1000) * pxPerSec;
                    const isDragging = draggingBoundaryIndex === b.index;
                    const trans = parseBoundaryTransition(b.transition);
                    const hasTransition = trans.kind !== "cut";
                    const isFlagged = Boolean(b.flags && b.flags.length > 0);
                    const isOutOfSelected = b.index === selectedSceneIndex + 1;

                    return (
                      <div
                        key={b.index}
                        onPointerDown={(e) => handleDividerPointerDown(e, b.index)}
                        onDoubleClick={(e) => {
                          e.stopPropagation();
                          openTimecodeEditor(b.index);
                        }}
                        onClick={(e) => e.stopPropagation()}
                        title={`Drag cut #${b.index} · ${hasTransition
                          ? `${trans.kind === "fade_black"
                            ? "Fade Black"
                            : "Crossfade"
                          } (${trans.duration_ms}ms) · `
                          : "Hard Cut · "
                          }Click badge to cycle transition · Double-click for exact timecode`}
                        style={{ left: `${leftPx}px` }}
                        className="group/cut absolute top-0 bottom-0 z-20 -ml-2.5 w-5 flex justify-center cursor-col-resize touch-none"
                      >
                        {/* Seam Vertical Cut Line */}
                        <div
                          className={`h-full w-[2px] transition-colors ${isDragging
                            ? "bg-brand-lavender shadow-[0_0_8px_rgba(169,112,255,0.95)]"
                            : isFlagged
                              ? "bg-amber-500"
                              : hasTransition
                                ? "bg-brand-lavender/90"
                                : "bg-brand-violet/70 dark:bg-brand-lavender/65 group-hover/cut:bg-brand-lavender"
                            }`}
                        />

                        {/* Interactive CapCut Transition Pill Badge */}
                        <button
                          type="button"
                          onPointerDown={(e) => e.stopPropagation()}
                          onClick={(e) => {
                            e.stopPropagation();
                            handleSeamTransitionClick(b.index);
                          }}
                          title={
                            selectedSceneIndex === b.index - 1
                              ? `Click to cycle transition (${trans.kind})`
                              : `Select Scene ${b.index} transition (${trans.kind})`
                          }
                          className={`absolute top-1/2 -translate-y-1/2 flex h-5 min-w-[18px] items-center justify-center rounded-[4px] border px-1 font-mono text-[9px] font-bold shadow-md transition-all ${isDragging
                            ? "scale-110 border-white bg-brand-lavender text-[#140B29]"
                            : isFlagged
                              ? "border-amber-300 bg-amber-500 text-zinc-950"
                              : trans.kind === "crossfade"
                                ? "border-white/80 bg-brand-lavender text-[#140B29]"
                                : trans.kind === "fade_black"
                                  ? "border-brand-lavender bg-[#1B1136] text-brand-lavender"
                                  : isOutOfSelected
                                    ? "border-brand-lavender bg-brand-violet text-white"
                                    : "border-zinc-700 bg-zinc-900/95 text-zinc-300 opacity-85 group-hover/cut:opacity-100 group-hover/cut:border-brand-lavender group-hover/cut:text-white"
                            }`}
                        >
                          {trans.kind === "crossfade"
                            ? "⋈"
                            : trans.kind === "fade_black"
                              ? "◆"
                              : "||"}
                        </button>
                      </div>
                    );
                  })}
                </div>

                {/* A1 VOICEOVER NARRATION WAVEFORM TRACK (CapCut Audio Clip Style) */}
                <div
                  className={`relative h-14 border-b border-border bg-zinc-200/60 dark:bg-[#080B10] overflow-hidden transition-opacity ${isA1Muted || isMuted ? "opacity-45" : "opacity-100"
                    }`}
                >
                  <div className="absolute inset-x-0 top-1 bottom-1 rounded-[5px] border border-emerald-500/45 bg-[#0C1D24] dark:bg-[#0A1820] overflow-hidden flex flex-col">
                    {/* CapCut Top Audio Clip Header Strip */}
                    <div className="h-4 shrink-0 px-2 flex items-center justify-between gap-2 bg-[#0F2933]/90 border-b border-emerald-400/20">
                      <div className="flex items-center gap-1.5 min-w-0">
                        <Volume2 className="h-2.5 w-2.5 text-emerald-400 shrink-0" />
                        <span className="font-mono text-[9.5px] font-semibold text-emerald-100 truncate">
                          {audioFileName || "Narration Voiceover.wav"}
                        </span>
                      </div>
                      <span className="font-mono text-[9px] text-emerald-300/80 shrink-0">
                        Voiceover · -16 LUFS · {silences.length} pauses
                      </span>
                    </div>

                    {/* High-Density Vertical Waveform Bars & Pause Zones */}
                    <div className="relative flex-1 w-full overflow-hidden">
                      {/* Speech Silence / Pause Zones */}
                      {silences.map((s, idx) => {
                        const leftPx = (s.startMs / 1000) * pxPerSec;
                        const widthPx = Math.max(
                          2,
                          ((s.endMs - s.startMs) / 1000) * pxPerSec
                        );
                        return (
                          <div
                            key={idx}
                            title={`Natural speech pause (${s.durationMs}ms)`}
                            style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                            className="pointer-events-none absolute top-0 bottom-0 bg-amber-400/15 border-x border-amber-400/25"
                          />
                        );
                      })}

                      {/* Bottom-Aligned CapCut Vertical Amplitude Bars */}
                      <div className="pointer-events-none relative flex h-full w-full items-end justify-between gap-[1.5px] px-1 pb-0.5">
                        {waveformBars.map((bar, idx) => (
                          <div
                            key={idx}
                            className={`flex-1 rounded-t-[1px] ${bar.inSilence
                              ? "bg-amber-300/40"
                              : "bg-gradient-to-t from-emerald-500/80 via-emerald-400/90 to-teal-200/95"
                              }`}
                            style={{ height: `${bar.heightPct}%` }}
                          />
                        ))}
                      </div>
                    </div>
                  </div>
                </div>

                {/* A2 BACKGROUND MUSIC & AUTO-DUCKING TRACK */}
                <div
                  className={`relative h-12 bg-zinc-200/50 dark:bg-[#090812] overflow-hidden transition-opacity ${isA2Muted || isMuted ? "opacity-45" : "opacity-100"
                    }`}
                >
                  {musicFilename ? (
                    <div
                      onClick={(e) => {
                        e.stopPropagation();
                        setInspectorTab("music");
                      }}
                      title="Click to inspect background music & auto-ducking settings"
                      className={`absolute inset-x-0 top-1 bottom-1 rounded-[5px] border overflow-hidden flex flex-col cursor-pointer transition-colors ${inspectorTab === "music"
                        ? "border-brand-lavender ring-1 ring-brand-lavender/50 bg-[#1B1333]"
                        : "border-brand-lavender/40 bg-[#140E26] hover:border-brand-lavender/80"
                        }`}
                    >
                      {/* A2 Music Clip Header Strip */}
                      <div className="h-3.5 shrink-0 px-2 flex items-center justify-between gap-2 bg-[#231742]/90 border-b border-brand-lavender/20">
                        <div className="flex items-center gap-1.5 min-w-0">
                          <Music className="h-2.5 w-2.5 text-brand-lavender shrink-0" />
                          <span className="font-mono text-[9px] font-semibold text-purple-100 truncate">
                            {musicFilename}
                          </span>
                        </div>
                        <span className="font-mono text-[8.5px] text-brand-lavender/90 shrink-0">
                          {musicSettings.volume_db} dB ·{" "}
                          {musicSettings.ducking_enabled
                            ? `Auto-Duck ${musicSettings.duck_db} dB`
                            : "Ducking OFF"}{" "}
                          · {musicSettings.loop_enabled ? "Loop ON" : "Single"}
                        </span>
                      </div>

                      {/* A2 Music Ducked Waveform Bars */}
                      <div className="pointer-events-none relative flex-1 flex h-full w-full items-end justify-between gap-[1.5px] px-1 pb-0.5">
                        {musicWaveformBars.map((bar, idx) => (
                          <div
                            key={idx}
                            className={`flex-1 rounded-t-[1px] ${bar.isDucked
                              ? "bg-brand-lavender/45"
                              : "bg-gradient-to-t from-brand-violet/90 via-brand-lavender/85 to-purple-200/95"
                              }`}
                            style={{ height: `${bar.heightPct}%` }}
                          />
                        ))}
                      </div>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setInspectorTab("music");
                      }}
                      className="absolute inset-x-0 top-1 bottom-1 rounded-[5px] border border-dashed border-border/80 dark:border-zinc-800 hover:border-brand-lavender/60 bg-secondary/20 dark:bg-[#110E1C]/75 flex items-center px-3 gap-2 text-left transition-colors"
                    >
                      <Music className="h-3 w-3 text-brand-lavender/80 shrink-0" />
                      <span className="font-mono text-[10px] text-muted-foreground hover:text-foreground">
                        + Add Background Music Track (Auto-ducked under voiceover · -14 LUFS)
                      </span>
                    </button>
                  )}
                </div>

                {/* HOVER GHOST GUIDE LINE & TIMECODE TOOLTIP */}
                {hoverTimeMs !== null &&
                  !isScrubbingPlayhead &&
                  draggingBoundaryIndex === null && (
                    <div
                      className="pointer-events-none absolute top-0 bottom-0 z-20 w-[1px] bg-brand-lavender/45"
                      style={{ left: `${(hoverTimeMs / 1000) * pxPerSec}px` }}
                    >
                      <div className="ml-1.5 mt-0.5 whitespace-nowrap rounded bg-zinc-900/95 border border-brand-lavender/40 px-1.5 py-0.5 font-mono text-[9px] text-brand-lavender shadow-sm">
                        {formatTimestampPrecise(hoverTimeMs)}
                      </div>
                    </div>
                  )}

                {/* FULL-HEIGHT PLAYHEAD NEEDLE WITH CAPCUT PENTAGON CAP */}
                <div
                  className="pointer-events-none absolute top-0 bottom-0 z-30 w-[2px] bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.85)]"
                  style={{ left: `${(currentTimeMs / 1000) * pxPerSec}px` }}
                >
                  <div
                    onPointerDown={handleRulerPointerDown}
                    title={`Playhead ${formatTimestampPrecise(
                      currentTimeMs
                    )} — Drag to scrub`}
                    className="pointer-events-auto -ml-[6px] w-3.5 h-4 cursor-ew-resize flex items-start justify-center touch-none"
                  >
                    <svg
                      viewBox="0 0 14 16"
                      className="w-3.5 h-4 drop-shadow-[0_1px_4px_rgba(0,0,0,0.85)]"
                    >
                      <path
                        d="M1 1 H13 V10 L7 15 L1 10 Z"
                        fill="#EF4444"
                        stroke="#FFFFFF"
                        strokeWidth="1.25"
                      />
                    </svg>
                  </div>
                  {isScrubbingPlayhead && (
                    <div className="ml-2.5 -mt-3.5 whitespace-nowrap rounded bg-red-500 px-1.5 py-0.5 font-mono text-[9.5px] font-bold text-white shadow-md">
                      {formatTimestampPrecise(currentTimeMs)}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* MOBILE CAPCUT CONTEXT TOOLBAR (< lg) */}
      <div className="lg:hidden mx-2 mb-2 rounded-lg border border-border bg-card p-1.5">
        <div className="flex items-center gap-1 overflow-x-auto no-scrollbar">
          <button
            type="button"
            onClick={() => setShowMobileInspector((v) => !v)}
            className={`shrink-0 flex items-center gap-1.5 rounded px-2.5 py-2 text-xs font-medium transition-colors ${showMobileInspector
              ? "bg-brand-lavender text-black font-semibold"
              : "bg-secondary text-foreground"
              }`}
          >
            <Sliders className="h-3.5 w-3.5" />
            Scene {String(selectedSceneIndex + 1).padStart(2, "0")}
          </button>

          <button
            type="button"
            disabled={!outCutEditable}
            onClick={() => nudgeSelectedOutCut(-100)}
            className="shrink-0 rounded bg-secondary px-2.5 py-2 font-mono text-xs text-foreground disabled:opacity-35"
          >
            -0.1s Cut
          </button>

          <button
            type="button"
            disabled={!outCutEditable}
            onClick={() => nudgeSelectedOutCut(100)}
            className="shrink-0 rounded bg-secondary px-2.5 py-2 font-mono text-xs text-foreground disabled:opacity-35"
          >
            +0.1s Cut
          </button>

          <button
            type="button"
            disabled={!outCutEditable}
            onClick={() => openTimecodeEditor(selectedSceneIndex + 1)}
            className="shrink-0 flex items-center gap-1 rounded bg-secondary px-2.5 py-2 text-xs text-foreground disabled:opacity-35"
          >
            <Clock className="h-3.5 w-3.5" />
            Set Out
          </button>

          <button
            type="button"
            onClick={() => jumpScene(-1)}
            disabled={selectedSceneIndex === 0}
            className="shrink-0 flex items-center gap-1 rounded bg-secondary px-2.5 py-2 text-xs text-foreground disabled:opacity-35"
          >
            <ChevronLeft className="h-3.5 w-3.5" />
            Prev
          </button>

          <button
            type="button"
            onClick={() => jumpScene(1)}
            disabled={selectedSceneIndex >= sections.length - 1}
            className="shrink-0 flex items-center gap-1 rounded bg-secondary px-2.5 py-2 text-xs text-foreground disabled:opacity-35"
          >
            Next
            <ChevronRight className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Mobile Expandable Scene Effects & Details Card */}
        {showMobileInspector && (
          <div className="mt-2 border-t border-border pt-2.5 px-1.5 space-y-2.5 text-xs">
            <div className="flex items-center justify-between font-mono text-[11px]">
              <span className="text-muted-foreground">
                In {formatTimestampPrecise(currentSceneRange.start)}
              </span>
              <span className="text-brand-violet dark:text-brand-lavender font-semibold">
                Out {formatTimestampPrecise(currentSceneRange.end)}
              </span>
            </div>
            <div className="flex items-center rounded-md border border-border bg-secondary/40 p-0.5">
              {(["effects", "subtitles", "music"] as const).map((tab) => (
                <button
                  key={tab}
                  type="button"
                  onClick={() => setInspectorTab(tab)}
                  className={`flex-1 rounded-[4px] py-1 text-[11px] font-medium capitalize transition-colors ${inspectorTab === tab
                    ? "bg-brand-violet text-white dark:bg-brand-lavender dark:text-zinc-950 font-semibold"
                    : "text-muted-foreground hover:text-foreground"
                    }`}
                >
                  {tab === "effects" ? "Motion" : tab}
                </button>
              ))}
            </div>
            {inspectorTab === "subtitles" ? (
              <SubtitlesInspectorPanel
                aspectRatio={projectAspectRatio}
                cues={subtitleCues}
                activeCueId={activeSubtitle?.id}
                style={subtitleStyle}
                isBusy={isUpdatingSubtitles}
                onSeekToMs={seekToMs}
                onUpdateCue={handleUpdateSubtitleCue}
                onSplitCue={handleSplitSubtitleCue}
                onMergeWithNext={handleMergeSubtitleCues}
                onUpdateStyle={handleUpdateSubtitleStyle}
              />
            ) : inspectorTab === "music" ? (
              <MusicInspectorPanel
                filename={musicFilename}
                audioUrl={musicUrl}
                settings={musicSettings}
                isUploading={isUploadingMusic}
                uploadProgress={musicUploadProgress}
                onUploadMusicFile={handleUploadMusicFile}
                onUpdateSettings={handleUpdateMusicSettings}
                onDeleteMusic={handleDeleteMusic}
              />
            ) : (
              <SceneEffectsPanel
                sceneIndex={selectedSceneIndex}
                assignment={inspectedAssignment}
                hasAssignedMedia={Boolean(inspectedMedia || inspectedAssignment?.mediaId)}
                resolvedPreset={
                  inspectedResolved?.preset ??
                  inspectedAssignment?.resolvedPreset ??
                  "still"
                }
                resolvedHasShake={
                  inspectedResolved?.hasShake ??
                  inspectedAssignment?.resolvedHasShake ??
                  false
                }
                isPickingFocal={isPickingFocal}
                outCutEditable={outCutEditable}
                outTransition={outTransition}
                isSavingEffect={isSavingEffect}
                onTogglePickFocal={() => setIsPickingFocal((v) => !v)}
                onUpdateEffect={handleUpdateSceneEffect}
                onUpdateOutTransition={handleUpdateOutTransition}
              />
            )}
            <p className="text-foreground/90 leading-relaxed border-t border-border/60 pt-2">
              {inspectedSection?.text || "No script text for this scene."}
            </p>
          </div>
        )}
      </div>
        </>
      )}

      {/* Exact Timecode Edit Modal */}
      <Dialog
        open={editingBoundaryIndex !== null}
        onOpenChange={(open) => !open && setEditingBoundaryIndex(null)}
      >
        <DialogContent onClose={() => setEditingBoundaryIndex(null)}>
          <DialogHeader>
            <DialogTitle>Edit Cut Boundary #{editingBoundaryIndex}</DialogTitle>
            <DialogDescription>
              Adjusts the end of Scene {editingBoundaryIndex} and start of Scene{" "}
              {(editingBoundaryIndex ?? 0) + 1} without shifting later scenes.
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={saveExactTimecode} className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-foreground">
                Timecode (mm:ss.mmm)
              </label>
              <Input
                value={editTimeInput}
                onChange={(e) => setEditTimeInput(e.target.value)}
                placeholder="00:15.500"
                className="font-mono"
                autoFocus
                error={editError || undefined}
              />
            </div>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setEditingBoundaryIndex(null)}
              >
                Cancel
              </Button>
              <Button type="submit">Apply Cut</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
