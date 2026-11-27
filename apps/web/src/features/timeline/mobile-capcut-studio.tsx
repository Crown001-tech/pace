"use client";

import React, { useState, useRef, useEffect, useCallback } from "react";
import {
  X,
  Play,
  Pause,
  Maximize2,
  Minimize2,
  Repeat,
  Shuffle,
  Volume2,
  VolumeX,
  Plus,
  Scissors,
  Music,
  Type,
  Sparkles,
  Image as ImageIcon,
  Captions,
  Check,
  ZoomIn,
  ZoomOut,
  SkipBack,
  SkipForward,
  Clock,
  Magnet,
  Layers,
  ChevronDown,
  FileText,
  Mic,
  FolderOpen,
  Sticker,
} from "lucide-react";
import type {
  BoundaryTransitionDto,
  MediaResponse,
  MusicSettingsDto,
  RenderSpecResponse,
  StickerPlacementSpec,
  SubtitleCueDto,
  SubtitleStyleDto,
  UpdateMusicSettingsRequest,
  UpdateSceneEffectRequest,
} from "@pace/shared-types";
import type { MediaItem, SectionAssignment } from "@/features/media/media-store";
import { useMediaStore } from "@/features/media/media-store";
import { useMediaUpload } from "@/features/media/use-media-upload";
import { Dropzone } from "@/features/media/dropzone";
import {
  MediaSourcesPanel,
  toStoreMediaItem,
} from "@/features/media/media-sources-panel";
import { PreviewPlayer, PreviewSpecMeta } from "./preview-player";
import { SceneEffectsPanel } from "./scene-effects-panel";
import { StickersPanel } from "./stickers-panel";
import {
  MusicInspectorPanel,
  SubtitlesInspectorPanel,
} from "./subtitles-music-panel";
import {
  formatTimestampPrecise,
  TimelineBoundaryItem,
} from "./timeline-engine";
import { parseBoundaryTransition } from "./preview-engine";

export type MobileStudioDrawer =
  | null
  | "edit"
  | "audio"
  | "script"
  | "effects"
  | "stickers"
  | "media"
  | "captions";

interface MobileCapCutStudioProps {
  projectId: string;
  projectName?: string;
  projectAspectRatio: "16:9" | "9:16";
  fps: number;
  currentTimeMs: number;
  totalDurationMs: number;
  isPlaying: boolean;
  isLoopingScene: boolean;
  isMuted: boolean;
  isA1Muted: boolean;
  showSafeGrid: boolean;
  isPickingFocal: boolean;
  effectSeed: number;
  isRerolling: boolean;
  isSavingEffect: boolean;
  zoomLevel: number;
  pxPerSec: number;
  totalWidthPx: number;
  timeTicks: number[];
  sections: { id: string; position?: number; text: string }[];
  boundaries: TimelineBoundaryItem[];
  assignments: SectionAssignment[];
  media: MediaItem[];
  compiledRenderSpec: RenderSpecResponse | null;
  preferLocalSpec: boolean;
  activeSceneIndex: number;
  selectedSceneIndex: number;
  currentSceneRange: { start: number; end: number };
  outCutEditable: boolean;
  outTransition: BoundaryTransitionDto;
  inspectedAssignment?: SectionAssignment;
  inspectedMedia?: MediaItem;
  inspectedResolved?: { preset: string; hasShake: boolean };
  activeSubtitle?: SubtitleCueDto;
  subtitleCues: SubtitleCueDto[];
  subtitleStyle: SubtitleStyleDto;
  isUpdatingSubtitles: boolean;
  stickers?: StickerPlacementSpec[];
  isSavingStickers?: boolean;
  includeTiltInAuto?: boolean;
  onUpdateStickers?: (next: StickerPlacementSpec[]) => void;
  onToggleIncludeTiltInAuto?: (next: boolean) => void;
  waveformBars: { heightPct: number; inSilence: boolean }[];
  audioFileName: string;
  musicFilename: string | null;
  musicUrl: string | null;
  musicSettings: MusicSettingsDto;
  isUploadingMusic: boolean;
  musicUploadProgress: number;
  isPauseSnapEnabled: boolean;
  snappedFeedback: string | null;
  onTogglePlay: () => void;
  onSeekToMs: (ms: number) => void;
  onJumpScene: (dir: -1 | 1) => void;
  onSelectScene: (idx: number) => void;
  onSetZoomLevel: React.Dispatch<React.SetStateAction<number>>;
  onToggleLoopScene: () => void;
  onToggleMute: () => void;
  onToggleA1Mute: () => void;
  onTogglePauseSnap: () => void;
  onTogglePickFocal: () => void;
  onPickFocalPoint: (x: number, y: number) => void;
  onSpecMetaChange: (meta: PreviewSpecMeta) => void;
  onRerollEffects: () => void;
  onNudgeOutCut: (deltaMs: number) => void;
  onOpenTimecodeEditor: (boundaryIndex: number) => void;
  onApplyBoundaryMove: (
    boundaryIndex: number,
    targetTimeMs: number,
    commitApi: boolean
  ) => void;
  onUpdateSceneEffect: (patch: UpdateSceneEffectRequest) => void;
  onUpdateOutTransition: (transition: BoundaryTransitionDto) => void;
  onAssignMediaInStudio: (mediaId: string) => void;
  onUpdateSubtitleCue: (
    cueId: string,
    text: string,
    startMs?: number,
    endMs?: number
  ) => Promise<void>;
  onSplitSubtitleCue: (cueId: string) => Promise<void>;
  onMergeSubtitleCues: (firstId: string, secondId: string) => Promise<void>;
  onUpdateSubtitleStyle: (style: SubtitleStyleDto) => Promise<void>;
  onUploadMusicFile: (file: File) => Promise<void>;
  onUpdateMusicSettings: (patch: UpdateMusicSettingsRequest) => Promise<void>;
  onDeleteMusic: () => Promise<void>;
  onOpenExport?: () => void;
  onChangeStep?: (step: "script" | "media" | "narration" | "timeline") => void;
  onExitProject?: () => void;
}

function formatShortClock(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

export function MobileCapCutStudio({
  projectId,
  projectName,
  projectAspectRatio,
  fps,
  currentTimeMs,
  totalDurationMs,
  isPlaying,
  isLoopingScene,
  isMuted,
  isA1Muted,
  showSafeGrid,
  isPickingFocal,
  effectSeed,
  isRerolling,
  isSavingEffect,
  zoomLevel,
  pxPerSec,
  totalWidthPx,
  timeTicks,
  sections,
  boundaries,
  assignments,
  media,
  compiledRenderSpec,
  preferLocalSpec,
  activeSceneIndex,
  selectedSceneIndex,
  currentSceneRange,
  outCutEditable,
  outTransition,
  inspectedAssignment,
  inspectedMedia,
  inspectedResolved,
  activeSubtitle,
  subtitleCues,
  subtitleStyle,
  isUpdatingSubtitles,
  stickers = [],
  isSavingStickers = false,
  includeTiltInAuto = false,
  onUpdateStickers,
  onToggleIncludeTiltInAuto,
  waveformBars,
  audioFileName,
  musicFilename,
  musicUrl,
  musicSettings,
  isUploadingMusic,
  musicUploadProgress,
  isPauseSnapEnabled,
  snappedFeedback,
  onTogglePlay,
  onSeekToMs,
  onJumpScene,
  onSelectScene,
  onSetZoomLevel,
  onToggleLoopScene,
  onToggleMute,
  onToggleA1Mute,
  onTogglePauseSnap,
  onTogglePickFocal,
  onPickFocalPoint,
  onSpecMetaChange,
  onRerollEffects,
  onNudgeOutCut,
  onOpenTimecodeEditor,
  onApplyBoundaryMove,
  onUpdateSceneEffect,
  onUpdateOutTransition,
  onAssignMediaInStudio,
  onUpdateSubtitleCue,
  onSplitSubtitleCue,
  onMergeSubtitleCues,
  onUpdateSubtitleStyle,
  onUploadMusicFile,
  onUpdateMusicSettings,
  onDeleteMusic,
  onOpenExport,
  onChangeStep,
  onExitProject,
}: MobileCapCutStudioProps) {
  const [activeDrawer, setActiveDrawer] = useState<MobileStudioDrawer>(null);
  const [mediaSubTab, setMediaSubTab] = useState<
    "bin" | "upload" | "stock" | "ai"
  >("bin");
  const [showNavMenu, setShowNavMenu] = useState(false);
  const [isFullscreenPreview, setIsFullscreenPreview] = useState(false);
  const [draggingCutIdx, setDraggingCutIdx] = useState<number | null>(null);
  const [isScrubbing, setIsScrubbing] = useState(false);

  const trackScrollRef = useRef<HTMLDivElement>(null);

  const { handleUploadFiles, isProcessing: isUploadingGallery } = useMediaUpload({
    projectId,
    projectAspectRatio,
  });

  // Auto-follow playhead in mobile track strip while playing
  useEffect(() => {
    const el = trackScrollRef.current;
    if (!el || !isPlaying) return;
    const playheadX = (currentTimeMs / 1000) * pxPerSec;
    const viewW = el.clientWidth;
    if (viewW <= 0) return;
    if (playheadX > el.scrollLeft + viewW - 44 || playheadX < el.scrollLeft) {
      el.scrollLeft = Math.max(0, playheadX - viewW * 0.35);
    }
  }, [currentTimeMs, pxPerSec, isPlaying]);

  const clientXToMs = useCallback(
    (clientX: number) => {
      const el = trackScrollRef.current;
      if (!el) return 0;
      const rect = el.getBoundingClientRect();
      const offsetX = Math.max(0, clientX - rect.left + el.scrollLeft);
      return Math.max(
        0,
        Math.min(totalDurationMs, Math.round((offsetX / pxPerSec) * 1000))
      );
    },
    [pxPerSec, totalDurationMs]
  );

  // Playhead touch scrubbing
  useEffect(() => {
    if (!isScrubbing) return;
    const onMove = (e: PointerEvent) => {
      onSeekToMs(clientXToMs(e.clientX));
    };
    const onUp = () => setIsScrubbing(false);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [isScrubbing, clientXToMs, onSeekToMs]);

  // Cut boundary touch dragging
  useEffect(() => {
    if (draggingCutIdx === null) return;
    const onMove = (e: PointerEvent) => {
      onApplyBoundaryMove(draggingCutIdx, clientXToMs(e.clientX), false);
    };
    const onUp = (e: PointerEvent) => {
      onApplyBoundaryMove(draggingCutIdx, clientXToMs(e.clientX), true);
      setDraggingCutIdx(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [draggingCutIdx, clientXToMs, onApplyBoundaryMove]);

  const toggleDrawer = (drawer: Exclude<MobileStudioDrawer, null>) => {
    setActiveDrawer((prev) => (prev === drawer ? null : drawer));
  };

  const emptySectionRefs = sections
    .map((s, idx) => ({ id: s.id, position: idx, text: s.text }))
    .filter((s) => {
      const a = assignments.find((x) => x.sectionId === s.id);
      return !a?.mediaId;
    });

  const handleRemoteMediaImported = (
    saved: MediaResponse,
    assignedSectionId?: string
  ) => {
    const item = toStoreMediaItem(saved, projectAspectRatio);
    useMediaStore.getState().addMediaItem(item);
    if (assignedSectionId) {
      useMediaStore.getState().assignToSection(assignedSectionId, item.id);
    } else {
      onAssignMediaInStudio(item.id);
    }
  };

  const DOCK_ITEMS: {
    id: Exclude<MobileStudioDrawer, null>;
    label: string;
    icon: React.ElementType;
  }[] = [
    { id: "edit", label: "Edit", icon: Scissors },
    { id: "audio", label: "Audio", icon: Music },
    { id: "script", label: "Script", icon: Type },
    { id: "effects", label: "Effects", icon: Sparkles },
    { id: "stickers", label: "Stickers", icon: Sticker },
    { id: "media", label: "Media", icon: ImageIcon },
    { id: "captions", label: "Captions", icon: Captions },
  ];

  const inspectedSection = sections[selectedSceneIndex];

  return (
    <div className="fixed inset-0 z-50 flex h-[100dvh] w-full flex-col overflow-hidden bg-[#0D0D0F] text-zinc-100 select-none lg:hidden">
      {/* BAND 1: CAPCUT CLEAN MOBILE TOP HEADER */}
      <header className="relative z-40 flex h-12 shrink-0 items-center justify-between px-3 bg-[#0D0D0F]">
        <div className="flex items-center gap-1.5 min-w-0">
          <button
            type="button"
            onClick={() => {
              if (onChangeStep) {
                onChangeStep("media");
              } else if (onExitProject) {
                onExitProject();
              }
            }}
            aria-label="Close Studio"
            className="flex h-9 w-9 items-center justify-center rounded-md text-zinc-200 hover:bg-zinc-800/80 active:scale-95 transition-all touch-manipulation"
          >
            <X className="h-5 w-5 stroke-[1.8]" />
          </button>

          <button
            type="button"
            onClick={() => setShowNavMenu((v) => !v)}
            className="flex items-center gap-1 rounded-md bg-[#1A1A20] border border-zinc-800/90 px-2.5 py-1 text-xs font-medium text-zinc-200 active:scale-[0.98] touch-manipulation max-w-[145px]"
          >
            <span className="truncate">{projectName || "Studio"}</span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-zinc-400" />
          </button>

          {showNavMenu && (
            <>
              <div
                onClick={() => setShowNavMenu(false)}
                className="fixed inset-0 z-40"
              />
              <div className="absolute left-3 top-12 z-50 w-48 rounded-lg border border-zinc-800 bg-[#18181F] p-1.5 shadow-2xl space-y-0.5">
                {onChangeStep && (
                  <>
                    <button
                      type="button"
                      onClick={() => {
                        setShowNavMenu(false);
                        onChangeStep("script");
                      }}
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-zinc-200 hover:bg-zinc-800"
                    >
                      <FileText className="h-3.5 w-3.5 text-zinc-400" />
                      1. Edit Script
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setShowNavMenu(false);
                        onChangeStep("media");
                      }}
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-zinc-200 hover:bg-zinc-800"
                    >
                      <ImageIcon className="h-3.5 w-3.5 text-zinc-400" />
                      2. Scene Visuals
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setShowNavMenu(false);
                        onChangeStep("narration");
                      }}
                      className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-zinc-200 hover:bg-zinc-800"
                    >
                      <Mic className="h-3.5 w-3.5 text-zinc-400" />
                      3. Voiceover Sync
                    </button>
                  </>
                )}
                {onExitProject && (
                  <button
                    type="button"
                    onClick={() => {
                      setShowNavMenu(false);
                      onExitProject();
                    }}
                    className="flex w-full items-center gap-2 rounded-md border-t border-zinc-800/80 mt-1 pt-2 px-2.5 py-2 text-xs font-medium text-zinc-300 hover:bg-zinc-800"
                  >
                    <FolderOpen className="h-3.5 w-3.5 text-zinc-400" />
                    All Projects
                  </button>
                )}
              </div>
            </>
          )}
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onOpenExport}
            className="flex h-8 items-center gap-1 rounded-md border border-zinc-800 bg-[#1C1C22] px-2.5 font-mono text-[11px] font-medium text-zinc-200 active:scale-[0.98] touch-manipulation"
          >
            <span>720p</span>
            <span className="text-zinc-500">·</span>
            <span>{projectAspectRatio}</span>
          </button>

          <button
            type="button"
            onClick={onOpenExport}
            className="flex h-8 items-center justify-center rounded-md bg-[#25F4EE] px-3.5 text-xs font-bold text-zinc-950 hover:bg-[#1ee0da] active:scale-[0.98] transition-all touch-manipulation"
          >
            Export
          </button>
        </div>
      </header>

      {/* BAND 2: EDGE-TO-EDGE MATTE BLACK VIDEO STAGE */}
      <div
        className={
          isFullscreenPreview
            ? "fixed inset-0 z-50 flex flex-col bg-black"
            : "relative flex-1 min-h-0 w-full bg-[#0D0D0F] flex items-center justify-center overflow-hidden"
        }
      >
        {snappedFeedback && (
          <div className="pointer-events-none absolute top-2.5 left-1/2 -translate-x-1/2 z-30 rounded-md border border-zinc-700/80 bg-zinc-900/95 px-2.5 py-1 font-mono text-[10px] font-medium text-zinc-100 shadow-lg">
            {snappedFeedback}
          </div>
        )}

        {isPickingFocal && (
          <div className="pointer-events-none absolute bottom-2.5 left-1/2 -translate-x-1/2 z-30 rounded-md bg-[#25F4EE] px-2.5 py-1 text-[11px] font-semibold text-zinc-950 shadow-lg">
            Tap anywhere on video to set focal point
          </div>
        )}

        <PreviewPlayer
          aspectRatio={projectAspectRatio}
          fps={fps}
          currentTimeMs={currentTimeMs}
          durationMs={totalDurationMs}
          effectSeed={effectSeed}
          sections={sections}
          boundaries={boundaries}
          assignments={assignments}
          media={media}
          compiledRenderSpec={compiledRenderSpec}
          preferLocalSpec={preferLocalSpec}
          activeSceneIndex={activeSceneIndex}
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
          onPickFocalPoint={onPickFocalPoint}
          onSpecMetaChange={onSpecMetaChange}
          isFullscreen={isFullscreenPreview}
        />

        {isFullscreenPreview && (
          <div className="absolute bottom-6 inset-x-4 flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-950/90 px-4 py-2.5">
            <span className="font-mono text-xs text-zinc-300 tabular-nums">
              {formatShortClock(currentTimeMs)} / {formatShortClock(totalDurationMs)}
            </span>
            <button
              type="button"
              onClick={onTogglePlay}
              className="flex h-9 w-9 items-center justify-center rounded-md bg-white text-zinc-950"
            >
              {isPlaying ? (
                <Pause className="h-4 w-4 fill-current" />
              ) : (
                <Play className="h-4 w-4 fill-current ml-0.5" />
              )}
            </button>
            <button
              type="button"
              onClick={() => setIsFullscreenPreview(false)}
              className="flex h-9 w-9 items-center justify-center rounded-md border border-zinc-700 text-zinc-200"
              aria-label="Exit fullscreen"
            >
              <Minimize2 className="h-4 w-4" />
            </button>
          </div>
        )}
      </div>

      {/* BAND 3: MINIMALIST CAPCUT TRANSPORT BAR */}
      <div className="flex h-10 shrink-0 items-center justify-between px-3.5 bg-[#0D0D0F]">
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => setIsFullscreenPreview(true)}
            aria-label="Fullscreen preview"
            className="flex h-8 w-8 items-center justify-center rounded-md text-zinc-300 hover:bg-zinc-800/70 active:scale-95 touch-manipulation"
          >
            <Maximize2 className="h-4 w-4 stroke-[1.75]" />
          </button>
          <span className="font-mono text-[11px] text-zinc-400 pl-1">
            S{String(selectedSceneIndex + 1).padStart(2, "0")}/{sections.length}
          </span>
        </div>

        {/* Center Play / Pause */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onJumpScene(-1)}
            disabled={selectedSceneIndex === 0}
            aria-label="Previous scene"
            className="flex h-8 w-8 items-center justify-center rounded-md text-zinc-400 hover:text-white disabled:opacity-30 touch-manipulation"
          >
            <SkipBack className="h-4 w-4" />
          </button>

          <button
            type="button"
            onClick={onTogglePlay}
            aria-label={isPlaying ? "Pause" : "Play"}
            className="flex h-9 w-10 items-center justify-center rounded-md text-white active:scale-95 transition-transform touch-manipulation"
          >
            {isPlaying ? (
              <Pause className="h-5 w-5 fill-current" />
            ) : (
              <Play className="h-5 w-5 fill-current ml-0.5" />
            )}
          </button>

          <button
            type="button"
            onClick={() => onJumpScene(1)}
            disabled={selectedSceneIndex >= sections.length - 1}
            aria-label="Next scene"
            className="flex h-8 w-8 items-center justify-center rounded-md text-zinc-400 hover:text-white disabled:opacity-30 touch-manipulation"
          >
            <SkipForward className="h-4 w-4" />
          </button>
        </div>

        {/* Right Utility Icons (Loop Scene + Reroll Motion) */}
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onToggleLoopScene}
            title="Loop selected scene"
            className={`relative flex h-8 w-8 items-center justify-center rounded-md transition-colors touch-manipulation ${
              isLoopingScene
                ? "text-[#25F4EE] bg-[#25F4EE]/10"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <Repeat className="h-4 w-4" />
            {isLoopingScene && (
              <span className="absolute -bottom-0.5 right-0.5 font-mono text-[7px] font-bold text-[#25F4EE]">
                ON
              </span>
            )}
          </button>

          <button
            type="button"
            disabled={isRerolling || sections.length === 0}
            onClick={onRerollEffects}
            title="Shuffle automatic motion presets"
            className="flex h-8 w-8 items-center justify-center rounded-md text-zinc-400 hover:text-zinc-200 disabled:opacity-40 touch-manipulation"
          >
            <Shuffle
              className={`h-4 w-4 ${isRerolling ? "animate-spin text-[#25F4EE]" : ""}`}
            />
          </button>
        </div>
      </div>

      {/* BAND 4: CAPCUT MULTI-TRACK TIMELINE STRIP */}
      <div className="relative flex h-[212px] shrink-0 flex-col border-t border-zinc-900 bg-[#111115]">
        {/* Top Timecode & Zoom Row */}
        <div className="flex h-7 shrink-0 items-center justify-between border-b border-zinc-800/60 px-3">
          <div className="flex items-center gap-1 font-mono text-[11px] tabular-nums">
            <span className="font-semibold text-zinc-100">
              {formatShortClock(currentTimeMs)}
            </span>
            <span className="text-zinc-600">/</span>
            <span className="text-zinc-500">
              {formatShortClock(totalDurationMs)}
            </span>
          </div>

          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() =>
                onSetZoomLevel((z) => Math.max(0.2, Number((z - 0.25).toFixed(2))))
              }
              aria-label="Zoom out timeline"
              className="flex h-6 w-6 items-center justify-center rounded text-zinc-400 hover:text-zinc-200 touch-manipulation"
            >
              <ZoomOut className="h-3.5 w-3.5" />
            </button>
            <span className="min-w-[34px] text-center font-mono text-[10px] text-zinc-400 tabular-nums">
              {Math.round(zoomLevel * 100)}%
            </span>
            <button
              type="button"
              onClick={() =>
                onSetZoomLevel((z) => Math.min(4, Number((z + 0.25).toFixed(2))))
              }
              aria-label="Zoom in timeline"
              className="flex h-6 w-6 items-center justify-center rounded text-zinc-400 hover:text-zinc-200 touch-manipulation"
            >
              <ZoomIn className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        {/* Left Gutter + Scrollable Tracks */}
        <div className="relative flex flex-1 min-h-0 overflow-hidden">
          {/* Compact CapCut Left Track Controls */}
          <div className="z-20 flex w-14 shrink-0 flex-col items-center justify-between border-r border-zinc-800/70 bg-[#111115] py-2">
            {/* Mute Clip Button */}
            <button
              type="button"
              onClick={() => {
                onToggleMute();
                if (!isMuted && isA1Muted) onToggleA1Mute();
              }}
              className="flex flex-col items-center gap-0.5 text-zinc-400 hover:text-zinc-100 touch-manipulation"
            >
              {isMuted || isA1Muted ? (
                <VolumeX className="h-4 w-4 text-red-400" />
              ) : (
                <Volume2 className="h-4 w-4" />
              )}
              <span className="text-[9px] leading-none">
                {isMuted || isA1Muted ? "Muted" : "Mute"}
              </span>
            </button>

            {/* Audio Track Quick Button */}
            <button
              type="button"
              onClick={() => toggleDrawer("audio")}
              aria-label="Audio settings"
              className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors touch-manipulation ${
                activeDrawer === "audio"
                  ? "border-[#25F4EE] bg-[#25F4EE]/15 text-[#25F4EE]"
                  : "border-zinc-800 bg-[#1B1B22] text-zinc-400"
              }`}
            >
              <Music className="h-3.5 w-3.5" />
            </button>

            {/* Captions Visibility Toggle */}
            <button
              type="button"
              onClick={() =>
                void onUpdateSubtitleStyle({
                  ...subtitleStyle,
                  enabled: !subtitleStyle.enabled,
                })
              }
              aria-label="Toggle captions"
              className={`flex h-7 w-7 items-center justify-center rounded-md border transition-colors touch-manipulation ${
                subtitleStyle.enabled
                  ? "border-zinc-700 bg-[#1B1B22] text-amber-400"
                  : "border-zinc-800 bg-[#16161B] text-zinc-600"
              }`}
            >
              <Type className="h-3.5 w-3.5" />
            </button>
          </div>

          {/* Scrollable Multi-Track Canvas */}
          <div
            ref={trackScrollRef}
            onClick={(e) => onSeekToMs(clientXToMs(e.clientX))}
            className="relative flex-1 overflow-x-auto overflow-y-hidden bg-[#111115] no-scrollbar touch-pan-x"
          >
            <div
              style={{ width: `${totalWidthPx + 96}px` }}
              className="relative h-full pt-5"
            >
              {/* Ruler Ticks */}
              <div
                onPointerDown={(e) => {
                  e.stopPropagation();
                  setIsScrubbing(true);
                  onSeekToMs(clientXToMs(e.clientX));
                }}
                className="absolute top-0 left-0 right-0 h-5 flex items-center touch-none"
              >
                {timeTicks.map((tickMs) => (
                  <span
                    key={tickMs}
                    style={{ left: `${(tickMs / 1000) * pxPerSec}px` }}
                    className="pointer-events-none absolute top-0.5 -translate-x-1/2 font-mono text-[9.5px] text-zinc-500 tabular-nums"
                  >
                    {formatShortClock(tickMs)}
                  </span>
                ))}
              </div>

              {/* TRACK 1: CAPCUT REPEATING PHOTO FILMSTRIP + WHITE TRANSITION SQUARES */}
              <div className="relative h-[56px] w-full">
                {sections.map((sec, idx) => {
                  const startMs = boundaries[idx]?.timeMs ?? 0;
                  const endMs = boundaries[idx + 1]?.timeMs ?? totalDurationMs;
                  const leftPx = (startMs / 1000) * pxPerSec;
                  const widthPx = Math.max(12, ((endMs - startMs) / 1000) * pxPerSec);
                  const assign = assignments.find((a) => a.sectionId === sec.id);
                  const thumb = assign?.mediaId
                    ? media.find((m) => m.id === assign.mediaId)?.thumbUrl
                    : undefined;
                  const isSelected = idx === selectedSceneIndex;

                  return (
                    <div
                      key={sec.id}
                      onClick={(e) => {
                        e.stopPropagation();
                        onSelectScene(idx);
                        onSeekToMs(startMs);
                      }}
                      style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                      className={`absolute top-0.5 bottom-0.5 overflow-hidden rounded-[4px] transition-shadow ${
                        isSelected
                          ? "ring-2 ring-white z-10"
                          : "border border-zinc-800"
                      } bg-zinc-900`}
                    >
                      {thumb ? (
                        <>
                          <div
                            className="absolute inset-0"
                            style={{
                              backgroundImage: `url("${thumb}")`,
                              backgroundRepeat: "repeat-x",
                              backgroundSize: "auto 100%",
                              backgroundPosition: "left center",
                            }}
                          />
                          <div
                            className="pointer-events-none absolute inset-0"
                            style={{
                              backgroundImage:
                                "repeating-linear-gradient(90deg, transparent, transparent 52px, rgba(0,0,0,0.45) 52px, rgba(0,0,0,0.45) 53px)",
                            }}
                          />
                        </>
                      ) : (
                        <div className="flex h-full items-center px-2 bg-[#1C1C24]">
                          <span className="font-mono text-[10px] text-zinc-400 truncate">
                            S{String(idx + 1).padStart(2, "0")} · Add visual
                          </span>
                        </div>
                      )}
                      <span className="pointer-events-none absolute bottom-1 left-1 rounded bg-black/75 px-1 font-mono text-[8.5px] font-semibold text-zinc-100">
                        {((endMs - startMs) / 1000).toFixed(1)}s
                      </span>
                    </div>
                  );
                })}

                {/* White CapCut Transition Seam Badges */}
                {boundaries.slice(1, -1).map((b) => {
                  const leftPx = (b.timeMs / 1000) * pxPerSec;
                  const trans = parseBoundaryTransition(b.transition);
                  const isActiveTrans = trans.kind !== "cut";
                  return (
                    <div
                      key={b.index}
                      onPointerDown={(e) => {
                        e.stopPropagation();
                        setDraggingCutIdx(b.index);
                      }}
                      onClick={(e) => e.stopPropagation()}
                      style={{ left: `${leftPx}px` }}
                      className="absolute top-0 bottom-0 z-20 -ml-3 w-6 flex items-center justify-center touch-none"
                    >
                      <button
                        type="button"
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectScene(b.index - 1);
                          setActiveDrawer("effects");
                        }}
                        aria-label={`Cut ${b.index} transition`}
                        className={`flex h-5 w-5 items-center justify-center rounded-[4px] text-[10px] font-bold shadow-md transition-transform active:scale-95 ${
                          isActiveTrans
                            ? "bg-[#25F4EE] text-zinc-950"
                            : "bg-white text-zinc-950"
                        }`}
                      >
                        |
                      </button>
                    </div>
                  );
                })}
              </div>

              {/* TRACK 2: CAPCUT CYAN/TEAL SYMMETRIC VOICEOVER WAVEFORM */}
              <div
                onClick={(e) => {
                  e.stopPropagation();
                  toggleDrawer("audio");
                }}
                style={{ width: `${totalWidthPx}px` }}
                className={`relative mt-2 h-[36px] overflow-hidden rounded-[4px] border border-[#25F4EE]/30 bg-[#0D353D] ${
                  isMuted || isA1Muted ? "opacity-40" : "opacity-100"
                }`}
              >
                {/* Center-Mirrored Cyan Waveform Bars */}
                <div className="pointer-events-none absolute inset-0 flex items-center justify-between gap-[1.5px] px-1">
                  {waveformBars.map((bar, idx) => (
                    <div
                      key={idx}
                      className={`flex-1 rounded-[1px] ${
                        bar.inSilence ? "bg-[#25F4EE]/20" : "bg-[#25F4EE]/85"
                      }`}
                      style={{ height: `${Math.max(12, bar.heightPct)}%` }}
                    />
                  ))}
                </div>
                <div className="pointer-events-none relative z-10 flex h-full items-center px-2">
                  <span className="truncate rounded bg-[#072026]/75 px-1.5 py-0.5 font-sans text-[10px] font-medium text-zinc-100">
                    {audioFileName || "Narration Voiceover.mp3"}
                  </span>
                </div>
              </div>

              {/* TRACK 3: CAPCUT WARM ORANGE CAPTION BLOCKS */}
              <div
                style={{ width: `${totalWidthPx}px` }}
                className={`relative mt-2 h-[30px] ${
                  subtitleStyle.enabled ? "opacity-100" : "opacity-40"
                }`}
              >
                {subtitleCues.length === 0 ? (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleDrawer("captions");
                    }}
                    className="flex h-full items-center rounded-[4px] border border-dashed border-amber-500/40 bg-amber-500/10 px-3 text-[10px] font-medium text-amber-300"
                  >
                    + Sync narration in Audio step to generate synced captions
                  </button>
                ) : (
                  subtitleCues.map((cue) => {
                    const leftPx = (cue.start_ms / 1000) * pxPerSec;
                    const widthPx = Math.max(
                      16,
                      ((cue.end_ms - cue.start_ms) / 1000) * pxPerSec
                    );
                    const isActive = activeSubtitle?.id === cue.id;
                    return (
                      <button
                        key={cue.id}
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onSeekToMs(cue.start_ms);
                          setActiveDrawer("captions");
                        }}
                        style={{ left: `${leftPx}px`, width: `${widthPx}px` }}
                        className={`absolute top-0 bottom-0 flex items-center overflow-hidden rounded-[4px] px-2 text-left text-[10.5px] font-medium text-white transition-all ${
                          isActive
                            ? "bg-[#EA7617] ring-1 ring-white z-10"
                            : "bg-[#C86010] border border-[#E58538]/40"
                        }`}
                      >
                        <span className="truncate">{cue.text}</span>
                      </button>
                    );
                  })
                )}
              </div>

              {/* CRISP WHITE VERTICAL PLAYHEAD LINE */}
              <div
                style={{ left: `${(currentTimeMs / 1000) * pxPerSec}px` }}
                className="pointer-events-none absolute top-0 bottom-0 z-30 w-[2px] bg-white shadow-[0_0_8px_rgba(255,255,255,0.85)]"
              >
                <div
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    setIsScrubbing(true);
                  }}
                  className="pointer-events-auto -ml-3 -mt-1 h-6 w-6 touch-none flex items-start justify-center"
                >
                  <div className="h-2.5 w-2.5 rounded-[2px] bg-white shadow" />
                </div>
              </div>
            </div>
          </div>

          {/* CapCut Floating White [+] Add/Swap Media Button */}
          <button
            type="button"
            onClick={() => setActiveDrawer("media")}
            aria-label="Add or replace scene media"
            className="absolute right-2.5 top-[34px] z-30 flex h-8 w-8 items-center justify-center rounded-md bg-white text-zinc-950 shadow-lg active:scale-95 transition-transform touch-manipulation"
          >
            <Plus className="h-4 w-4 stroke-[2.5]" />
          </button>
        </div>

        {/* CONTEXTUAL HALF-SCREEN SLIDE-UP SHEET (Keeps Video Visible Above!) */}
        {activeDrawer !== null && (
          <div className="fixed inset-x-0 bottom-14 z-40 flex max-h-[48dvh] flex-col rounded-t-lg border-t border-zinc-800 bg-[#16161C] shadow-[0_-16px_40px_rgba(0,0,0,0.75)]">
            {/* Sheet Header */}
            <div className="flex h-10 shrink-0 items-center justify-between border-b border-zinc-800/80 px-3.5">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold capitalize text-zinc-100">
                  {activeDrawer === "effects"
                    ? `Motion & Transitions · S${String(selectedSceneIndex + 1).padStart(2, "0")}`
                    : activeDrawer === "edit"
                    ? `Trim & Framing · S${String(selectedSceneIndex + 1).padStart(2, "0")}`
                    : activeDrawer === "media"
                    ? `Visuals · S${String(selectedSceneIndex + 1).padStart(2, "0")}`
                    : activeDrawer}
                </span>
              </div>

              <button
                type="button"
                onClick={() => setActiveDrawer(null)}
                aria-label="Close panel"
                className="flex h-7 w-7 items-center justify-center rounded-md bg-zinc-800/90 text-zinc-100 active:scale-95 touch-manipulation"
              >
                <Check className="h-4 w-4" />
              </button>
            </div>

            {/* Sheet Scrollable Body */}
            <div className="flex-1 overflow-y-auto p-3.5 text-xs">
              {activeDrawer === "effects" && (
                <div className="space-y-3">
                  {onToggleIncludeTiltInAuto && (
                    <label className="flex items-center justify-between rounded-md border border-zinc-800 bg-[#1B1B22] px-2.5 py-2 text-[11px] text-zinc-200 cursor-pointer">
                      <span>Include tilted presets in Auto Re-roll</span>
                      <input
                        type="checkbox"
                        checked={includeTiltInAuto}
                        onChange={(e) => onToggleIncludeTiltInAuto(e.target.checked)}
                        className="h-4 w-4 accent-[#25F4EE]"
                      />
                    </label>
                  )}
                  <SceneEffectsPanel
                    sceneIndex={selectedSceneIndex}
                    assignment={inspectedAssignment}
                    hasAssignedMedia={Boolean(
                      inspectedMedia || inspectedAssignment?.mediaId
                    )}
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
                    onTogglePickFocal={() => {
                      onTogglePickFocal();
                      setActiveDrawer(null);
                    }}
                    onUpdateEffect={onUpdateSceneEffect}
                    onUpdateOutTransition={onUpdateOutTransition}
                  />
                </div>
              )}

              {activeDrawer === "stickers" && (
                <StickersPanel
                  stickers={stickers}
                  currentTimeMs={currentTimeMs}
                  totalDurationMs={totalDurationMs}
                  isSaving={isSavingStickers}
                  onChangeStickers={(next) => onUpdateStickers?.(next)}
                  onSeekToMs={onSeekToMs}
                />
              )}

              {activeDrawer === "edit" && (
                <div className="space-y-3.5">
                  <div className="grid grid-cols-2 gap-2">
                    <div className="rounded-md border border-zinc-800 bg-[#1E1E26] p-2.5">
                      <span className="font-mono text-[10px] uppercase text-zinc-400">
                        Scene Start
                      </span>
                      <div className="mt-0.5 font-mono text-xs font-semibold text-zinc-100">
                        {formatTimestampPrecise(currentSceneRange.start)}
                      </div>
                    </div>
                    <div className="rounded-md border border-zinc-800 bg-[#1E1E26] p-2.5">
                      <div className="flex items-center justify-between">
                        <span className="font-mono text-[10px] uppercase text-zinc-400">
                          Out Cut
                        </span>
                        {outCutEditable && (
                          <button
                            type="button"
                            onClick={() =>
                              onOpenTimecodeEditor(selectedSceneIndex + 1)
                            }
                            className="inline-flex items-center gap-1 text-[10px] font-semibold text-[#25F4EE]"
                          >
                            <Clock className="h-3 w-3" />
                            Exact
                          </button>
                        )}
                      </div>
                      <div className="mt-0.5 font-mono text-xs font-semibold text-[#25F4EE]">
                        {formatTimestampPrecise(currentSceneRange.end)}
                      </div>
                    </div>
                  </div>

                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between text-[11px] text-zinc-400">
                      <span>Nudge Out Cut</span>
                      <span className="font-mono text-zinc-200">
                        {(
                          (currentSceneRange.end - currentSceneRange.start) /
                          1000
                        ).toFixed(2)}
                        s
                      </span>
                    </div>
                    <div className="grid grid-cols-4 gap-1.5">
                      {[-500, -100, 100, 500].map((delta) => (
                        <button
                          key={delta}
                          type="button"
                          disabled={!outCutEditable}
                          onClick={() => onNudgeOutCut(delta)}
                          className="min-h-[36px] rounded-md border border-zinc-800 bg-[#1E1E26] font-mono text-xs font-medium text-zinc-100 active:scale-[0.98] disabled:opacity-35 touch-manipulation"
                        >
                          {delta > 0 ? `+${delta / 1000}s` : `${delta / 1000}s`}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className="flex items-center justify-between rounded-md border border-zinc-800 bg-[#1E1E26] p-2.5">
                    <div className="flex items-center gap-2">
                      <Magnet className="h-4 w-4 text-[#25F4EE]" />
                      <div>
                        <div className="text-xs font-medium text-zinc-100">
                          Magnetic Pause Snap
                        </div>
                        <div className="text-[10px] text-zinc-400">
                          Snap dragged cuts to natural voice pauses
                        </div>
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={onTogglePauseSnap}
                      className={`min-h-[30px] rounded-md px-3 text-xs font-semibold touch-manipulation ${
                        isPauseSnapEnabled
                          ? "bg-[#25F4EE] text-zinc-950"
                          : "bg-zinc-800 text-zinc-300"
                      }`}
                    >
                      {isPauseSnapEnabled ? "ON" : "OFF"}
                    </button>
                  </div>

                  <div className="flex items-center justify-between rounded-md border border-zinc-800 bg-[#1E1E26] p-2.5">
                    <span className="text-xs font-medium text-zinc-200">
                      Framing Mode
                    </span>
                    <div className="flex gap-1 rounded-md bg-zinc-900 p-0.5">
                      {(["fill", "fit_blur"] as const).map((mode) => (
                        <button
                          key={mode}
                          type="button"
                          onClick={() => onUpdateSceneEffect({ fit_mode: mode })}
                          className={`min-h-[28px] rounded px-2.5 text-[11px] font-semibold touch-manipulation ${
                            (inspectedAssignment?.fitMode ?? "fill") === mode
                              ? "bg-white text-zinc-950"
                              : "text-zinc-400"
                          }`}
                        >
                          {mode === "fill" ? "Fill" : "Fit Blur"}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {activeDrawer === "audio" && (
                <MusicInspectorPanel
                  filename={musicFilename}
                  audioUrl={musicUrl}
                  settings={musicSettings}
                  isUploading={isUploadingMusic}
                  uploadProgress={musicUploadProgress}
                  onUploadMusicFile={onUploadMusicFile}
                  onUpdateSettings={onUpdateMusicSettings}
                  onDeleteMusic={onDeleteMusic}
                />
              )}

              {activeDrawer === "captions" && (
                <SubtitlesInspectorPanel
                  aspectRatio={projectAspectRatio}
                  cues={subtitleCues}
                  activeCueId={activeSubtitle?.id}
                  style={subtitleStyle}
                  isBusy={isUpdatingSubtitles}
                  onSeekToMs={onSeekToMs}
                  onUpdateCue={onUpdateSubtitleCue}
                  onSplitCue={onSplitSubtitleCue}
                  onMergeWithNext={onMergeSubtitleCues}
                  onUpdateStyle={onUpdateSubtitleStyle}
                />
              )}

              {activeDrawer === "script" && (
                <div className="space-y-3">
                  <div className="rounded-md border border-zinc-800 bg-[#1E1E26] p-3">
                    <div className="flex items-center justify-between font-mono text-[10px] text-zinc-400 mb-1">
                      <span>SCENE {String(selectedSceneIndex + 1).padStart(2, "0")}</span>
                      <span>
                        {formatShortClock(currentSceneRange.start)} -{" "}
                        {formatShortClock(currentSceneRange.end)}
                      </span>
                    </div>
                    <p className="text-xs leading-relaxed text-zinc-100">
                      {inspectedSection?.text || "No script text for this scene."}
                    </p>
                  </div>

                  <div className="space-y-1.5">
                    <div className="font-mono text-[10px] uppercase text-zinc-400">
                      Jump to Scene ({sections.length})
                    </div>
                    <div className="max-h-40 overflow-y-auto space-y-1">
                      {sections.map((sec, idx) => (
                        <button
                          key={sec.id}
                          type="button"
                          onClick={() => {
                            onSelectScene(idx);
                            onSeekToMs(boundaries[idx]?.timeMs ?? 0);
                          }}
                          className={`flex w-full items-center gap-2 rounded-md border px-2.5 py-2 text-left transition-colors ${
                            idx === selectedSceneIndex
                              ? "border-[#25F4EE] bg-[#25F4EE]/10 text-white"
                              : "border-zinc-800/80 bg-[#1B1B22] text-zinc-300"
                          }`}
                        >
                          <span className="font-mono text-[10px] font-bold text-[#25F4EE] shrink-0">
                            S{String(idx + 1).padStart(2, "0")}
                          </span>
                          <span className="truncate text-xs">{sec.text}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {activeDrawer === "media" && (
                <div className="space-y-3">
                  <div className="grid grid-cols-4 gap-1 rounded-md bg-zinc-900 p-1">
                    {(
                      [
                        { id: "bin", label: `Bin (${media.length})` },
                        { id: "upload", label: "Gallery" },
                        { id: "stock", label: "Stock $0" },
                        { id: "ai", label: "AI Gen $0" },
                      ] as const
                    ).map((t) => (
                      <button
                        key={t.id}
                        type="button"
                        onClick={() => setMediaSubTab(t.id)}
                        className={`min-h-[30px] rounded px-1.5 text-[11px] font-semibold transition-colors touch-manipulation ${
                          mediaSubTab === t.id
                            ? "bg-[#25F4EE] text-zinc-950"
                            : "text-zinc-400 hover:text-zinc-200"
                        }`}
                      >
                        {t.label}
                      </button>
                    ))}
                  </div>

                  {mediaSubTab === "bin" ? (
                    media.length === 0 ? (
                      <div className="rounded-md border border-dashed border-zinc-800 p-6 text-center text-xs text-zinc-400">
                        No visuals in bin yet. Switch to Gallery, Stock $0, or AI Gen $0 above.
                      </div>
                    ) : (
                      <div className="grid grid-cols-3 gap-2">
                        {media.map((item) => {
                          const isCurrent =
                            inspectedAssignment?.mediaId === item.id;
                          return (
                            <button
                              key={item.id}
                              type="button"
                              onClick={() => onAssignMediaInStudio(item.id)}
                              className={`relative aspect-video overflow-hidden rounded-md border bg-zinc-950 ${
                                isCurrent
                                  ? "border-[#25F4EE] ring-2 ring-[#25F4EE]/50"
                                  : "border-zinc-800"
                              }`}
                            >
                              {/* eslint-disable-next-line @next/next/no-img-element */}
                              <img
                                src={item.thumbUrl}
                                alt={item.filename}
                                className="h-full w-full object-cover"
                                loading="lazy"
                              />
                              {isCurrent && (
                                <span className="absolute top-1 left-1 rounded bg-[#25F4EE] px-1 py-0.5 font-mono text-[8px] font-bold text-zinc-950">
                                  Active
                                </span>
                              )}
                            </button>
                          );
                        })}
                      </div>
                    )
                  ) : mediaSubTab === "upload" ? (
                    <Dropzone
                      onFilesSelected={handleUploadFiles}
                      disabled={isUploadingGallery}
                    />
                  ) : (
                    <MediaSourcesPanel
                      mode={mediaSubTab}
                      projectId={projectId}
                      projectAspectRatio={projectAspectRatio}
                      targetSectionId={inspectedSection?.id ?? null}
                      emptySections={emptySectionRefs}
                      onMediaImported={handleRemoteMediaImported}
                      onBatchComplete={async () => {}}
                    />
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* BAND 5: CAPCUT 6-ICON BOTTOM TOOL DOCK */}
      <nav
        aria-label="Mobile Studio Tools"
        className="z-50 grid h-14 shrink-0 grid-cols-7 border-t border-zinc-800/80 bg-[#0D0D0F] pb-safe"
      >
        {DOCK_ITEMS.map((item) => {
          const Icon = item.icon;
          const isActive = activeDrawer === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => toggleDrawer(item.id)}
              className={`flex flex-col items-center justify-center gap-1 transition-colors active:scale-95 touch-manipulation ${
                isActive ? "text-[#25F4EE]" : "text-zinc-400 hover:text-zinc-100"
              }`}
            >
              <Icon className="h-[18px] w-[18px] stroke-[1.75]" />
              <span className="text-[10px] font-medium leading-none tracking-tight">
                {item.label}
              </span>
            </button>
          );
        })}
      </nav>
    </div>
  );
}
