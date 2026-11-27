"use client";

import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Captions,
  CheckCircle2,
  Eye,
  EyeOff,
  Merge,
  Music,
  Pause,
  Pencil,
  Play,
  Scissors,
  Trash2,
  UploadCloud,
  Volume2,
} from "lucide-react";
import type {
  MusicSettingsDto,
  SubtitleCueDto,
  SubtitleStyleDto,
  UpdateMusicSettingsRequest,
} from "@pace/shared-types";
import { Button } from "@/components/ui/button";
import { formatTimestampPrecise } from "./timeline-engine";
import {
  DEFAULT_MUSIC_SETTINGS,
  DEFAULT_SUBTITLE_STYLE,
  evaluateSubtitleCueRules,
  maxCharsForAspectRatio,
  wrapSubtitlePreviewLines,
} from "./subtitle-rules";

export {
  DEFAULT_MUSIC_SETTINGS,
  DEFAULT_SUBTITLE_STYLE,
  evaluateSubtitleCueRules,
  maxCharsForAspectRatio,
  wrapSubtitlePreviewLines,
};

interface SubtitlesInspectorPanelProps {
  aspectRatio: "16:9" | "9:16";
  cues: SubtitleCueDto[];
  activeCueId?: string;
  style: SubtitleStyleDto;
  isBusy: boolean;
  onSeekToMs: (timeMs: number) => void;
  onUpdateCue: (
    cueId: string,
    text: string,
    startMs?: number,
    endMs?: number
  ) => Promise<void>;
  onSplitCue: (cueId: string) => Promise<void>;
  onMergeWithNext: (firstCueId: string, secondCueId: string) => Promise<void>;
  onUpdateStyle: (next: SubtitleStyleDto) => Promise<void>;
}

const COLOR_SWATCHES = [
  { label: "White", hex: "#FFFFFF" },
  { label: "Broadcast Yellow", hex: "#FACC15" },
  { label: "Cyan", hex: "#22D3EE" },
  { label: "Emerald", hex: "#4ADE80" },
];

export function SubtitlesInspectorPanel({
  aspectRatio,
  cues,
  activeCueId,
  style,
  isBusy,
  onSeekToMs,
  onUpdateCue,
  onSplitCue,
  onMergeWithNext,
  onUpdateStyle,
}: SubtitlesInspectorPanelProps) {
  const [subView, setSubView] = useState<"cues" | "style">("cues");
  const [editingCueId, setEditingCueId] = useState<string | null>(null);
  const [draftText, setDraftText] = useState("");
  const [draftStartSec, setDraftStartSec] = useState("");
  const [draftEndSec, setDraftEndSec] = useState("");

  const charLimit = maxCharsForAspectRatio(aspectRatio);

  const startEditing = (cue: SubtitleCueDto) => {
    setEditingCueId(cue.id);
    setDraftText(cue.text);
    setDraftStartSec((cue.start_ms / 1000).toFixed(2));
    setDraftEndSec((cue.end_ms / 1000).toFixed(2));
  };

  const commitEdit = async (cue: SubtitleCueDto) => {
    const trimmed = draftText.trim();
    if (!trimmed) return;
    const parsedStart = Math.round(Number(draftStartSec) * 1000);
    const parsedEnd = Math.round(Number(draftEndSec) * 1000);
    const validStart = Number.isFinite(parsedStart) && parsedStart >= 0 ? parsedStart : cue.start_ms;
    const validEnd =
      Number.isFinite(parsedEnd) && parsedEnd > validStart + 200
        ? parsedEnd
        : cue.end_ms;
    await onUpdateCue(cue.id, trimmed, validStart, validEnd);
    setEditingCueId(null);
  };

  const violationCount = useMemo(() => {
    return cues.filter((c) => evaluateSubtitleCueRules(c, aspectRatio).violations.length > 0)
      .length;
  }, [cues, aspectRatio]);

  return (
    <div className="space-y-3 text-xs">
      {/* Top Switcher: Cues vs Burn-in Style */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center rounded-md border border-border bg-secondary/40 p-0.5">
          <button
            type="button"
            onClick={() => setSubView("cues")}
            className={`rounded-[4px] px-2.5 py-1 text-[11px] font-medium transition-colors ${
              subView === "cues"
                ? "bg-card text-foreground font-semibold shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            Cues ({cues.length})
          </button>
          <button
            type="button"
            onClick={() => setSubView("style")}
            className={`rounded-[4px] px-2.5 py-1 text-[11px] font-medium transition-colors ${
              subView === "style"
                ? "bg-card text-foreground font-semibold shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            Burn-in Style
          </button>
        </div>

        <button
          type="button"
          onClick={() => void onUpdateStyle({ ...style, enabled: !style.enabled })}
          title={
            style.enabled
              ? "Subtitles enabled for preview and MP4 burn-in"
              : "Subtitles hidden"
          }
          className={`inline-flex items-center gap-1 rounded-md border px-2 py-1 font-mono text-[10px] transition-colors ${
            style.enabled
              ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 font-semibold"
              : "border-border bg-secondary/40 text-muted-foreground"
          }`}
        >
          {style.enabled ? (
            <>
              <Eye className="h-3 w-3" />
              Burn-in ON
            </>
          ) : (
            <>
              <EyeOff className="h-3 w-3" />
              Burn-in OFF
            </>
          )}
        </button>
      </div>

      {/* Broadcast Rule Summary Banner */}
      <div className="flex items-center justify-between rounded-md border border-border bg-secondary/25 px-2.5 py-1.5 font-mono text-[10px] text-muted-foreground">
        <span>
          ≤ 2 lines · ≤ {charLimit} chars/line · 1–7s
        </span>
        {cues.length > 0 && (
          <span
            className={
              violationCount === 0
                ? "text-emerald-600 dark:text-emerald-400 font-semibold"
                : "text-amber-600 dark:text-amber-400 font-semibold"
            }
          >
            {violationCount === 0 ? "All valid" : `${violationCount} to review`}
          </span>
        )}
      </div>

      {subView === "cues" ? (
        cues.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-4 text-center text-muted-foreground">
            <Captions className="h-5 w-5 mx-auto mb-1.5 opacity-60" />
            <p className="text-xs font-medium text-foreground">No subtitle cues yet</p>
            <p className="text-[11px] mt-0.5">
              Upload and align narration audio in Step 03 to generate word-timed subtitle cues.
            </p>
          </div>
        ) : (
          <div className="space-y-2 max-h-[285px] overflow-y-auto pr-0.5">
            {cues.map((cue, idx) => {
              const check = evaluateSubtitleCueRules(cue, aspectRatio);
              const isEditing = editingCueId === cue.id;
              const isActive = activeCueId === cue.id;
              const nextCue = cues[idx + 1];
              const canSplit =
                cue.text.trim().split(/\s+/).filter(Boolean).length >= 2 &&
                check.durationMs > 400;

              return (
                <div
                  key={cue.id}
                  className={`rounded-md border p-2 transition-colors ${
                    isActive
                      ? "border-brand-lavender bg-brand-lavender/12"
                      : "border-border bg-secondary/25 hover:bg-secondary/45"
                  }`}
                >
                  <div className="flex items-center justify-between gap-1.5 mb-1">
                    <button
                      type="button"
                      onClick={() => onSeekToMs(cue.start_ms)}
                      className="font-mono text-[10px] font-semibold text-brand-violet dark:text-brand-lavender hover:underline"
                    >
                      #{idx + 1} · {formatTimestampPrecise(cue.start_ms).slice(0, 8)} →{" "}
                      {formatTimestampPrecise(cue.end_ms).slice(0, 8)}
                    </button>

                    <div className="flex items-center gap-1">
                      <span
                        className={`rounded px-1 py-0.5 font-mono text-[9px] ${
                          check.violations.length === 0
                            ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
                            : "bg-amber-500/15 text-amber-700 dark:text-amber-300"
                        }`}
                      >
                        {check.maxLineChars}/{check.charLimit}c ·{" "}
                        {(check.durationMs / 1000).toFixed(1)}s
                      </span>
                    </div>
                  </div>

                  {isEditing ? (
                    <div className="space-y-2 mt-1.5">
                      <textarea
                        rows={2}
                        value={draftText}
                        onChange={(e) => setDraftText(e.target.value)}
                        className="w-full rounded border border-input bg-background px-2 py-1.5 text-xs text-foreground font-medium focus:outline-none focus:ring-1 focus:ring-brand-lavender"
                      />
                      <div className="grid grid-cols-2 gap-1.5">
                        <div>
                          <label className="block font-mono text-[9px] uppercase text-muted-foreground">
                            Start (sec)
                          </label>
                          <input
                            type="number"
                            step="0.1"
                            min="0"
                            value={draftStartSec}
                            onChange={(e) => setDraftStartSec(e.target.value)}
                            className="w-full rounded border border-input bg-background px-1.5 py-1 font-mono text-[11px]"
                          />
                        </div>
                        <div>
                          <label className="block font-mono text-[9px] uppercase text-muted-foreground">
                            End (sec)
                          </label>
                          <input
                            type="number"
                            step="0.1"
                            min="0.2"
                            value={draftEndSec}
                            onChange={(e) => setDraftEndSec(e.target.value)}
                            className="w-full rounded border border-input bg-background px-1.5 py-1 font-mono text-[11px]"
                          />
                        </div>
                      </div>
                      <div className="flex items-center justify-end gap-1.5">
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          onClick={() => setEditingCueId(null)}
                          className="h-6 px-2 text-[11px]"
                        >
                          Cancel
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          disabled={isBusy}
                          onClick={() => void commitEdit(cue)}
                          className="h-6 px-2.5 text-[11px]"
                        >
                          Save Cue
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <p className="text-xs font-medium text-foreground whitespace-pre-line leading-snug">
                        {check.lines.join("\n") || cue.text}
                      </p>

                      {check.violations.length > 0 && (
                        <div className="mt-1 flex items-center gap-1 text-[10px] font-mono text-amber-600 dark:text-amber-400">
                          <AlertTriangle className="h-3 w-3 shrink-0" />
                          <span>{check.violations.join(" · ")}</span>
                        </div>
                      )}

                      <div className="mt-1.5 flex items-center justify-end gap-1 border-t border-border/60 pt-1.5">
                        <button
                          type="button"
                          onClick={() => startEditing(cue)}
                          className="inline-flex items-center gap-1 rounded border border-border bg-card px-1.5 py-0.5 text-[10px] font-medium text-foreground hover:bg-secondary"
                        >
                          <Pencil className="h-2.5 w-2.5" />
                          Edit
                        </button>
                        <button
                          type="button"
                          disabled={!canSplit || isBusy}
                          onClick={() => void onSplitCue(cue.id)}
                          title="Split cue into two consecutive cues"
                          className="inline-flex items-center gap-1 rounded border border-border bg-card px-1.5 py-0.5 text-[10px] font-medium text-foreground hover:bg-secondary disabled:opacity-35"
                        >
                          <Scissors className="h-2.5 w-2.5" />
                          Split
                        </button>
                        {nextCue && (
                          <button
                            type="button"
                            disabled={isBusy}
                            onClick={() => void onMergeWithNext(cue.id, nextCue.id)}
                            title="Merge with next subtitle cue"
                            className="inline-flex items-center gap-1 rounded border border-border bg-card px-1.5 py-0.5 text-[10px] font-medium text-foreground hover:bg-secondary disabled:opacity-35"
                          >
                            <Merge className="h-2.5 w-2.5" />
                            Merge ↓
                          </button>
                        )}
                      </div>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )
      ) : (
        /* Burn-in Style Configuration Panel */
        <div className="space-y-3">
          {/* Font Family */}
          <div className="space-y-1">
            <label className="text-[10px] font-mono uppercase text-muted-foreground">
              Curated Font Family
            </label>
            <div className="grid grid-cols-2 gap-1.5">
              {(
                [
                  { id: "sans", label: "Ubuntu Sans Bold" },
                  { id: "mono", label: "Ubuntu Mono" },
                ] as const
              ).map((f) => (
                <button
                  key={f.id}
                  type="button"
                  onClick={() => void onUpdateStyle({ ...style, font: f.id })}
                  className={`rounded-md border px-2 py-1.5 text-[11px] font-medium transition-colors ${
                    style.font === f.id
                      ? "border-brand-violet bg-brand-violet/15 text-brand-violet dark:border-brand-lavender dark:bg-brand-lavender/15 dark:text-brand-lavender font-semibold"
                      : "border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {f.label}
                </button>
              ))}
            </div>
          </div>

          {/* Vertical Safe-Area Position */}
          <div className="space-y-1">
            <label className="text-[10px] font-mono uppercase text-muted-foreground">
              Vertical Safe Zone
            </label>
            <div className="grid grid-cols-3 gap-1.5">
              {(["bottom", "center", "top"] as const).map((pos) => (
                <button
                  key={pos}
                  type="button"
                  onClick={() => void onUpdateStyle({ ...style, position: pos })}
                  className={`rounded-md border py-1.5 text-[11px] font-medium capitalize transition-colors ${
                    style.position === pos
                      ? "border-brand-lavender bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                      : "border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {pos}
                </button>
              ))}
            </div>
          </div>

          {/* Font Size Scale (70% - 150%) */}
          <div className="space-y-1">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-mono uppercase text-muted-foreground">
                Size Scale
              </span>
              <span className="font-mono text-[11px] font-semibold text-foreground">
                {style.size_pct}%
              </span>
            </div>
            <input
              type="range"
              min={70}
              max={150}
              step={5}
              value={style.size_pct}
              onChange={(e) =>
                void onUpdateStyle({
                  ...style,
                  size_pct: Number(e.target.value),
                })
              }
              className="w-full accent-brand-lavender cursor-pointer"
            />
          </div>

          {/* Text Color Swatches */}
          <div className="space-y-1">
            <label className="text-[10px] font-mono uppercase text-muted-foreground">
              Text Color
            </label>
            <div className="grid grid-cols-4 gap-1.5">
              {COLOR_SWATCHES.map((sw) => {
                const selected =
                  style.text_color.toUpperCase() === sw.hex.toUpperCase();
                return (
                  <button
                    key={sw.hex}
                    type="button"
                    onClick={() =>
                      void onUpdateStyle({ ...style, text_color: sw.hex })
                    }
                    className={`flex items-center justify-center gap-1.5 rounded-md border py-1.5 text-[10px] font-mono transition-colors ${
                      selected
                        ? "border-brand-lavender bg-brand-lavender/15 text-foreground font-semibold"
                        : "border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <span
                      className="h-2.5 w-2.5 rounded-sm border border-black/40"
                      style={{ backgroundColor: sw.hex }}
                    />
                    <span>{sw.label.split(" ").pop()}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Outline Width & Backdrop Opacity */}
          <div className="grid grid-cols-2 gap-2.5">
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-mono uppercase text-muted-foreground">
                  Outline
                </span>
                <span className="font-mono text-[11px] text-foreground">
                  {style.outline_width_px}px
                </span>
              </div>
              <input
                type="range"
                min={0}
                max={6}
                step={1}
                value={style.outline_width_px}
                onChange={(e) =>
                  void onUpdateStyle({
                    ...style,
                    outline_width_px: Number(e.target.value),
                  })
                }
                className="w-full accent-brand-lavender cursor-pointer"
              />
            </div>

            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-mono uppercase text-muted-foreground">
                  Box Opacity
                </span>
                <span className="font-mono text-[11px] text-foreground">
                  {style.bg_opacity_pct}%
                </span>
              </div>
              <input
                type="range"
                min={0}
                max={100}
                step={5}
                value={style.bg_opacity_pct}
                onChange={(e) =>
                  void onUpdateStyle({
                    ...style,
                    bg_opacity_pct: Number(e.target.value),
                  })
                }
                className="w-full accent-brand-lavender cursor-pointer"
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface MusicInspectorPanelProps {
  filename: string | null;
  audioUrl: string | null;
  settings: MusicSettingsDto;
  isUploading: boolean;
  uploadProgress: number;
  onUploadMusicFile: (file: File) => Promise<void>;
  onUpdateSettings: (patch: UpdateMusicSettingsRequest) => Promise<void>;
  onDeleteMusic: () => Promise<void>;
}

export function MusicInspectorPanel({
  filename,
  audioUrl,
  settings,
  isUploading,
  uploadProgress,
  onUploadMusicFile,
  onUpdateSettings,
  onDeleteMusic,
}: MusicInspectorPanelProps) {
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const previewAudioRef = useRef<HTMLAudioElement | null>(null);
  const [isPreviewPlaying, setIsPreviewPlaying] = useState(false);

  useEffect(() => {
    if (previewAudioRef.current) {
      previewAudioRef.current.pause();
      previewAudioRef.current = null;
      setIsPreviewPlaying(false);
    }
    return () => {
      if (previewAudioRef.current) {
        previewAudioRef.current.pause();
        previewAudioRef.current = null;
      }
    };
  }, [audioUrl]);

  useEffect(() => {
    if (previewAudioRef.current) {
      previewAudioRef.current.loop = settings.loop_enabled;
    }
  }, [settings.loop_enabled]);

  const togglePreviewAudio = () => {
    if (!audioUrl) return;
    if (!previewAudioRef.current) {
      const a = new Audio(audioUrl);
      a.loop = settings.loop_enabled;
      a.volume = Math.min(1, Math.max(0.02, Math.pow(10, settings.volume_db / 20)));
      a.onended = () => setIsPreviewPlaying(false);
      previewAudioRef.current = a;
    }
    if (isPreviewPlaying) {
      previewAudioRef.current.pause();
      setIsPreviewPlaying(false);
    } else {
      previewAudioRef.current.volume = Math.min(
        1,
        Math.max(0.02, Math.pow(10, settings.volume_db / 20))
      );
      void previewAudioRef.current.play();
      setIsPreviewPlaying(true);
    }
  };

  return (
    <div className="space-y-3 text-xs">
      <input
        ref={fileInputRef}
        type="file"
        accept="audio/*"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) void onUploadMusicFile(f);
          e.target.value = "";
        }}
      />

      {/* Track Status / Upload Box */}
      {filename ? (
        <div className="rounded-md border border-border bg-secondary/35 p-2.5 space-y-2">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-1.5 min-w-0">
              <Music className="h-3.5 w-3.5 text-brand-lavender shrink-0" />
              <span className="font-mono text-[11px] font-semibold text-foreground truncate">
                {filename}
              </span>
            </div>
            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500 shrink-0" />
          </div>

          <div className="flex items-center justify-between gap-1.5">
            {audioUrl && (
              <button
                type="button"
                onClick={togglePreviewAudio}
                className="inline-flex items-center gap-1 rounded border border-border bg-card px-2 py-1 text-[11px] font-medium text-foreground hover:bg-secondary"
              >
                {isPreviewPlaying ? (
                  <>
                    <Pause className="h-3 w-3" />
                    Stop Preview
                  </>
                ) : (
                  <>
                    <Play className="h-3 w-3" />
                    Audition Track
                  </>
                )}
              </button>
            )}
            <div className="flex items-center gap-1 ml-auto">
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="rounded border border-border bg-card px-2 py-1 text-[10px] font-medium text-muted-foreground hover:text-foreground"
              >
                Replace
              </button>
              <button
                type="button"
                onClick={() => {
                  if (previewAudioRef.current) {
                    previewAudioRef.current.pause();
                    previewAudioRef.current = null;
                  }
                  setIsPreviewPlaying(false);
                  void onDeleteMusic();
                }}
                title="Remove background music"
                className="rounded border border-border bg-card p-1 text-red-400 hover:bg-red-500/10"
              >
                <Trash2 className="h-3 w-3" />
              </button>
            </div>
          </div>
        </div>
      ) : (
        <button
          type="button"
          disabled={isUploading}
          onClick={() => fileInputRef.current?.click()}
          className="w-full rounded-md border border-dashed border-border hover:border-brand-lavender bg-secondary/30 p-4 text-center transition-colors"
        >
          <UploadCloud className="h-5 w-5 mx-auto mb-1 text-muted-foreground" />
          <div className="text-xs font-medium text-foreground">
            {isUploading
              ? `Uploading Music (${uploadProgress}%)...`
              : "Upload Background Music"}
          </div>
          <div className="text-[10px] font-mono text-muted-foreground mt-0.5">
            MP3 · WAV · M4A · OGG · FLAC (looped & ducked to -14 LUFS)
          </div>
        </button>
      )}

      {/* Music Level (-40 dB to 0 dB) */}
      <div className="space-y-1">
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1 text-[10px] font-mono uppercase text-muted-foreground">
            <Volume2 className="h-3 w-3" />
            Music Volume
          </span>
          <span className="font-mono text-[11px] font-semibold text-foreground">
            {settings.volume_db.toFixed(1)} dB
          </span>
        </div>
        <input
          type="range"
          min={-40}
          max={0}
          step={0.5}
          value={settings.volume_db}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (previewAudioRef.current) {
              previewAudioRef.current.volume = Math.min(
                1,
                Math.max(0.02, Math.pow(10, v / 20))
              );
            }
            void onUpdateSettings({ volume_db: v });
          }}
          className="w-full accent-brand-lavender cursor-pointer"
        />
      </div>

      {/* Auto-Ducking under Voice */}
      <div className="rounded-md border border-border bg-secondary/25 p-2.5 space-y-2">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-[11px] font-semibold text-foreground">
              Auto-Duck Under Speech
            </div>
            <div className="text-[10px] text-muted-foreground">
              Sidechain lowers music while narration speaks
            </div>
          </div>
          <button
            type="button"
            onClick={() =>
              void onUpdateSettings({
                ducking_enabled: !settings.ducking_enabled,
              })
            }
            className={`rounded-md border px-2 py-1 font-mono text-[10px] transition-colors ${
              settings.ducking_enabled
                ? "border-brand-lavender bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender font-semibold"
                : "border-border bg-card text-muted-foreground"
            }`}
          >
            {settings.ducking_enabled ? "Enabled" : "Off"}
          </button>
        </div>

        {settings.ducking_enabled && (
          <div className="space-y-1 pt-1 border-t border-border/60">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-mono uppercase text-muted-foreground">
                Ducking Depth
              </span>
              <span className="font-mono text-[11px] text-foreground">
                {settings.duck_db.toFixed(1)} dB
              </span>
            </div>
            <input
              type="range"
              min={-18}
              max={-4}
              step={1}
              value={settings.duck_db}
              onChange={(e) =>
                void onUpdateSettings({ duck_db: Number(e.target.value) })
              }
              className="w-full accent-brand-lavender cursor-pointer"
            />
          </div>
        )}
      </div>

      {/* Loop & Fade In / Out */}
      <div className="space-y-2 rounded-md border border-border bg-secondary/25 p-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-medium text-foreground">
            Seamless Track Loop
          </span>
          <button
            type="button"
            onClick={() =>
              void onUpdateSettings({ loop_enabled: !settings.loop_enabled })
            }
            className={`rounded-md border px-2 py-0.5 font-mono text-[10px] ${
              settings.loop_enabled
                ? "border-brand-violet bg-brand-violet/15 text-brand-violet dark:border-brand-lavender dark:text-brand-lavender font-semibold"
                : "border-border bg-card text-muted-foreground"
            }`}
          >
            {settings.loop_enabled ? "Loop ON" : "Play Once"}
          </button>
        </div>

        {settings.loop_enabled && (
          <div className="space-y-1 pt-1 border-t border-border/60">
            <div className="flex items-center justify-between text-[10px] font-mono text-muted-foreground">
              <span>Loop Crossfade</span>
              <span>{(settings.crossfade_ms / 1000).toFixed(2)}s</span>
            </div>
            <input
              type="range"
              min={0}
              max={3000}
              step={250}
              value={settings.crossfade_ms}
              onChange={(e) =>
                void onUpdateSettings({ crossfade_ms: Number(e.target.value) })
              }
              className="w-full accent-brand-lavender cursor-pointer"
            />
          </div>
        )}

        <div className="grid grid-cols-2 gap-2 pt-1">
          <div>
            <div className="flex items-center justify-between text-[10px] font-mono text-muted-foreground">
              <span>Fade In</span>
              <span>{(settings.fade_in_ms / 1000).toFixed(1)}s</span>
            </div>
            <input
              type="range"
              min={0}
              max={5000}
              step={250}
              value={settings.fade_in_ms}
              onChange={(e) =>
                void onUpdateSettings({ fade_in_ms: Number(e.target.value) })
              }
              className="w-full accent-brand-lavender cursor-pointer"
            />
          </div>
          <div>
            <div className="flex items-center justify-between text-[10px] font-mono text-muted-foreground">
              <span>Fade Out</span>
              <span>{(settings.fade_out_ms / 1000).toFixed(1)}s</span>
            </div>
            <input
              type="range"
              min={0}
              max={5000}
              step={250}
              value={settings.fade_out_ms}
              onChange={(e) =>
                void onUpdateSettings({ fade_out_ms: Number(e.target.value) })
              }
              className="w-full accent-brand-lavender cursor-pointer"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
