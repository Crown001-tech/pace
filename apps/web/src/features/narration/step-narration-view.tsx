import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  Mic,
  Pause,
  Pencil,
  Play,
  RefreshCw,
  RotateCcw,
  AudioLines,
  UploadCloud,
  Volume2,
  X,
} from 'lucide-react';
import type {
  BoundaryDto,
  JobStatusDto,
  NarrationStateResponse,
  SubtitleCueDto,
} from '@pace/shared-types';
import { WaveformViewer } from './waveform-viewer';
import { BoundaryTable } from './boundary-table';
import { Button } from '@/components/ui/button';
import { api, ApiError } from '@/lib/api-client';
import { useScriptEditorStore } from '@/stores/script-editor-store';
import { formatDuration } from '@/lib/utils';
import type { BoundaryOutput, SilenceGap } from './alignment-engine';

interface StepNarrationViewProps {
  projectId: string;
  projectAspectRatio: '16:9' | '9:16';
  onProceedToTimeline?: () => void;
}

type UiSyncStage =
  | 'idle'
  | 'uploading'
  | 'preparing'
  | 'transcribing'
  | 'aligning'
  | 'completed'
  | 'failed';

function resolveAudioMime(file: File): string | null {
  const mime = file.type.split(';')[0]?.trim().toLowerCase() || '';
  if (
    [
      'audio/mpeg',
      'audio/mp3',
      'audio/wav',
      'audio/x-wav',
      'audio/wave',
      'audio/mp4',
      'audio/x-m4a',
      'audio/m4a',
      'audio/aac',
      'audio/ogg',
      'audio/opus',
      'audio/flac',
      'audio/x-flac',
      'audio/webm',
    ].includes(mime)
  ) {
    return mime;
  }

  const lower = file.name.toLowerCase();
  if (lower.endsWith('.mp3')) return 'audio/mpeg';
  if (lower.endsWith('.wav')) return 'audio/wav';
  if (lower.endsWith('.m4a')) return 'audio/mp4';
  if (lower.endsWith('.aac')) return 'audio/aac';
  if (lower.endsWith('.ogg')) return 'audio/ogg';
  if (lower.endsWith('.opus')) return 'audio/opus';
  if (lower.endsWith('.flac')) return 'audio/flac';
  if (lower.endsWith('.webm')) return 'audio/webm';
  return null;
}

async function computeSha256Hex(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const hashBuffer = await crypto.subtle.digest('SHA-256', buf);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

function uploadWithProgress(
  url: string,
  file: File,
  contentType: string,
  onProgress: (pct: number) => void
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('PUT', url, true);
    xhr.setRequestHeader('Content-Type', contentType);
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
        reject(new Error(`Audio upload failed with HTTP ${xhr.status}`));
      }
    };
    xhr.onerror = () => reject(new Error('Network error while uploading audio to storage'));
    xhr.send(file);
  });
}

function toBoundaryOutputs(dtos: BoundaryDto[]): BoundaryOutput[] {
  return dtos.map((b) => {
    const conf = b.confidence ?? 0.5;
    const confidenceLevel: 'High' | 'Medium' | 'Low' =
      conf >= 0.85 ? 'High' : conf >= 0.6 ? 'Medium' : 'Low';
    return {
      id: b.id,
      index: b.index,
      timeMs: b.time_ms,
      source: b.source,
      confidence: conf,
      confidenceLevel,
      flags: b.flags,
    };
  });
}

function mapJobToStage(job: JobStatusDto | null | undefined): UiSyncStage {
  if (!job) return 'idle';
  if (job.status === 'succeeded') return 'completed';
  if (job.status === 'failed') return 'failed';
  if (job.status === 'cancelled') return 'idle';
  if (job.stage === 'transcribing') return 'transcribing';
  if (job.stage === 'aligning') return 'aligning';
  return 'preparing';
}

export function StepNarrationView({
  projectId,
  projectAspectRatio,
  onProceedToTimeline,
}: StepNarrationViewProps) {
  const sections = useScriptEditorStore((s) => s.sections);

  const [isInitialLoading, setIsInitialLoading] = useState(true);
  const [narrationState, setNarrationState] = useState<NarrationStateResponse | null>(null);
  const [peaks, setPeaks] = useState<[number, number][] | undefined>(undefined);
  const [syncStage, setSyncStage] = useState<UiSyncStage>('idle');
  const [progress, setProgress] = useState(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [activeJob, setActiveJob] = useState<JobStatusDto | null>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [playTimeMs, setPlayTimeMs] = useState(0);
  const [playbackDurationMs, setPlaybackDurationMs] = useState(0);

  const [editingCueId, setEditingCueId] = useState<string | null>(null);
  const [editingCueText, setEditingCueText] = useState('');

  const fileInputRef = useRef<HTMLInputElement>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);

  const boundaries: BoundaryOutput[] = toBoundaryOutputs(narrationState?.boundaries ?? []);
  const silences: SilenceGap[] = (narrationState?.silences ?? []).map((s) => ({
    startMs: s.start_ms,
    endMs: s.end_ms,
    durationMs: s.duration_ms,
  }));
  const subtitleCues: SubtitleCueDto[] = narrationState?.subtitle_cues ?? [];
  const durationMs = narrationState?.duration_ms ?? playbackDurationMs;

  const hydrateNarration = useCallback(async () => {
    try {
      const state = await api.narration.get(projectId);
      setNarrationState(state);
      if (state.duration_ms) {
        setPlaybackDurationMs(state.duration_ms);
      }

      if (state.has_peaks) {
        try {
          const peaksResp = await api.narration.peaks(projectId);
          setPeaks(peaksResp.peaks);
        } catch {
          setPeaks(undefined);
        }
      } else {
        setPeaks(undefined);
      }

      const job = state.active_job ?? null;
      setActiveJob(job);

      if (job && (job.status === 'queued' || job.status === 'running')) {
        setSyncStage(mapJobToStage(job));
        setProgress(job.progress);
        setErrorMessage(null);
      } else if (job && job.status === 'failed') {
        setSyncStage('failed');
        setProgress(job.progress);
        setErrorMessage(job.error_message || 'Audio processing failed.');
      } else if (state.boundaries.length >= 2) {
        setSyncStage('completed');
        setProgress(100);
        setErrorMessage(null);
      } else if (state.media_id) {
        setSyncStage('preparing');
        setProgress(10);
      } else {
        setSyncStage('idle');
        setProgress(0);
      }
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load narration state.';
      setErrorMessage(msg);
    } finally {
      setIsInitialLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    setIsInitialLoading(true);
    setNarrationState(null);
    setPeaks(undefined);
    setIsPlaying(false);
    setPlayTimeMs(0);
    setPlaybackDurationMs(0);
    setErrorMessage(null);
    void hydrateNarration();
  }, [projectId, hydrateNarration]);

  // Poll running/queued audio jobs cleanly until completion or failure
  useEffect(() => {
    if (!activeJob || (activeJob.status !== 'queued' && activeJob.status !== 'running')) {
      return;
    }

    let cancelled = false;
    const interval = setInterval(async () => {
      try {
        const latest = await api.jobs.get(projectId, activeJob.id);
        if (cancelled) return;
        setActiveJob(latest);
        setSyncStage(mapJobToStage(latest));
        setProgress(latest.progress);

        if (latest.status === 'succeeded') {
          clearInterval(interval);
          await hydrateNarration();
        } else if (latest.status === 'failed') {
          clearInterval(interval);
          setErrorMessage(latest.error_message || 'Audio processing job failed.');
          setSyncStage('failed');
        }
      } catch {
        // Transient network hiccup during polling; retry on next tick
      }
    }, 1200);

    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [activeJob, projectId, hydrateNarration]);

  // Keep <audio> element synced cleanly to presigned audio_url
  useEffect(() => {
    const audioUrl = narrationState?.audio_url;
    if (!audioUrl) {
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
      setIsPlaying(false);
      return;
    }

    const audio = new Audio(audioUrl);
    audioRef.current = audio;

    const handleLoaded = () => {
      if (Number.isFinite(audio.duration) && audio.duration > 0) {
        setPlaybackDurationMs((prev) => (prev > 0 ? prev : Math.round(audio.duration * 1000)));
      }
    };
    const handleTimeUpdate = () => setPlayTimeMs(Math.round(audio.currentTime * 1000));
    const handleEnded = () => {
      setIsPlaying(false);
      setPlayTimeMs(0);
    };

    audio.addEventListener('loadedmetadata', handleLoaded);
    audio.addEventListener('timeupdate', handleTimeUpdate);
    audio.addEventListener('ended', handleEnded);

    return () => {
      audio.pause();
      audio.removeEventListener('loadedmetadata', handleLoaded);
      audio.removeEventListener('timeupdate', handleTimeUpdate);
      audio.removeEventListener('ended', handleEnded);
      audioRef.current = null;
    };
  }, [narrationState?.audio_url]);

  const handleFileUpload = async (file: File) => {
    const mime = resolveAudioMime(file);
    if (!mime) {
      setErrorMessage(
        'Unsupported audio format. Please upload MP3, WAV, M4A, AAC, OGG, FLAC, or WebM.'
      );
      return;
    }
    if (file.size > 250 * 1024 * 1024) {
      setErrorMessage('Audio file exceeds maximum size of 250 MB.');
      return;
    }

    setErrorMessage(null);
    setSyncStage('uploading');
    setProgress(2);
    setIsPlaying(false);

    try {
      const sha256 = await computeSha256Hex(file);
      const presign = await api.narration.presign(projectId, {
        filename: file.name,
        content_type: mime,
        bytes: file.size,
      });

      await uploadWithProgress(presign.upload_url, file, mime, (pct) => {
        setProgress(Math.max(5, Math.min(95, pct)));
      });

      const registered = await api.narration.register(projectId, {
        media_id: presign.media_id,
        storage_key: presign.storage_key,
        original_filename: file.name,
        mime,
        bytes: file.size,
        sha256,
      });

      setActiveJob(registered.job);
      setSyncStage(mapJobToStage(registered.job));
      setProgress(registered.job.progress || 5);
      await hydrateNarration();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? err.message
          : err instanceof Error
          ? err.message
          : 'Failed to upload narration.';
      setErrorMessage(msg);
      setSyncStage(narrationState?.media_id ? 'completed' : 'failed');
    }
  };

  const handleRealign = async () => {
    setErrorMessage(null);
    try {
      const resp = await api.narration.realign(projectId, {
        keep_manual_boundaries: true,
      });
      setActiveJob(resp.job);
      setSyncStage(mapJobToStage(resp.job));
      setProgress(resp.job.progress || 10);
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Failed to trigger re-alignment.');
    }
  };

  const handleDeleteNarration = async () => {
    if (audioRef.current) {
      audioRef.current.pause();
    }
    setErrorMessage(null);
    try {
      await api.narration.delete(projectId);
      setNarrationState(null);
      setPeaks(undefined);
      setActiveJob(null);
      setSyncStage('idle');
      setProgress(0);
      setPlayTimeMs(0);
      setPlaybackDurationMs(0);
      fileInputRef.current?.click();
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Failed to remove narration.');
    }
  };

  const handleNudgeBoundary = async (index: number, deltaMs: number) => {
    const target = boundaries.find((b) => b.index === index);
    if (!target) return;
    const candidateMs = Math.max(0, target.timeMs + deltaMs);

    try {
      const updated = await api.boundaries.update(projectId, index, {
        time_ms: candidateMs,
      });
      setNarrationState((prev) =>
        prev
          ? {
              ...prev,
              boundaries: prev.boundaries.map((b) => (b.index === index ? updated : b)),
            }
          : prev
      );
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Could not update boundary timing.');
    }
  };

  const handleSaveSubtitleCue = async (cue: SubtitleCueDto) => {
    const trimmed = editingCueText.trim();
    if (!trimmed) return;
    try {
      const updated = await api.subtitles.update(projectId, cue.id, {
        text: trimmed,
      });
      setNarrationState((prev) =>
        prev
          ? {
              ...prev,
              subtitle_cues: prev.subtitle_cues.map((c) => (c.id === cue.id ? updated : c)),
            }
          : prev
      );
      setEditingCueId(null);
      setEditingCueText('');
      setErrorMessage(null);
    } catch (err) {
      setErrorMessage(err instanceof ApiError ? err.message : 'Could not save subtitle edit.');
    }
  };

  const togglePlay = () => {
    if (!audioRef.current) return;
    if (isPlaying) {
      audioRef.current.pause();
      setIsPlaying(false);
    } else {
      void audioRef.current.play();
      setIsPlaying(true);
    }
  };

  const seekTo = (timeMs: number) => {
    setPlayTimeMs(timeMs);
    if (audioRef.current) {
      audioRef.current.currentTime = timeMs / 1000;
    }
  };

  const isBusy =
    syncStage === 'uploading' ||
    syncStage === 'preparing' ||
    syncStage === 'transcribing' ||
    syncStage === 'aligning';

  const hasNarration = Boolean(narrationState?.media_id);

  if (isInitialLoading) {
    return (
      <div className="py-20 text-center text-sm text-muted-foreground font-mono">
        Loading narration state...
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-5xl px-3 sm:px-6 py-4 sm:py-6 space-y-5">
      {/* Header Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-border pb-4">
        <div>
          <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
            <Mic className="w-4 h-4 text-brand-violet dark:text-brand-lavender" />
            <span>Narration & Audio Synchronization</span>
          </h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            Upload voiceover audio to normalize loudness, align speech to scenes, and generate timed cues ({sections.length} scenes · {projectAspectRatio}).
          </p>
        </div>

        <div className="flex items-center gap-2">
          {hasNarration && !isBusy && (
            <>
              <Button variant="outline" size="sm" onClick={handleRealign} className="text-xs">
                <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
                Re-align
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={handleDeleteNarration}
                className="text-xs"
              >
                Change Audio
              </Button>
            </>
          )}

          {syncStage === 'completed' && onProceedToTimeline && (
            <Button size="sm" onClick={onProceedToTimeline} className="text-xs">
              Proceed to Timeline
              <ArrowRight className="w-3.5 h-3.5 ml-1.5" />
            </Button>
          )}
        </div>
      </div>

      {errorMessage && (
        <div className="p-3 rounded-lg border border-red-300 bg-red-50 dark:border-red-900/60 dark:bg-red-950/30 flex items-start justify-between gap-2.5 text-xs text-red-800 dark:text-red-300">
          <div className="flex items-start gap-2.5">
            <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
            <span>{errorMessage}</span>
          </div>
          <button
            type="button"
            onClick={() => setErrorMessage(null)}
            className="p-0.5 hover:opacity-75 transition-opacity text-red-800 dark:text-red-300 shrink-0"
            aria-label="Dismiss error"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      <input
        ref={fileInputRef}
        type="file"
        accept="audio/*"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) void handleFileUpload(f);
          e.target.value = '';
        }}
      />

      {/* Empty Upload Zone */}
      {!hasNarration && syncStage !== 'uploading' && (
        <div
          onClick={() => fileInputRef.current?.click()}
          onDragOver={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const f = e.dataTransfer.files?.[0];
            if (f) void handleFileUpload(f);
          }}
          className="border border-dashed border-border hover:border-brand-violet dark:hover:border-brand-lavender bg-secondary/40 rounded-lg p-10 text-center cursor-pointer transition-colors group"
        >
          <div className="w-12 h-12 rounded-lg border border-border bg-card flex items-center justify-center mx-auto mb-3 group-hover:border-brand-violet dark:group-hover:border-brand-lavender transition-colors">
            <UploadCloud className="w-5 h-5 text-muted-foreground group-hover:text-brand-violet dark:group-hover:text-brand-lavender transition-colors" />
          </div>
          <h3 className="text-sm font-medium text-foreground">
            Drop narration audio here or click to browse
          </h3>
          <p className="text-xs text-muted-foreground mt-1 max-w-sm mx-auto font-mono">
            MP3 · WAV · M4A · OGG · FLAC · WebM (up to 45 min / 250 MB)
          </p>
        </div>
      )}

      {/* Progress Banner */}
      {isBusy && (
        <div className="border border-border bg-card rounded-lg p-5 space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 rounded-md border border-border bg-brand-violet/10 dark:bg-brand-lavender/15 flex items-center justify-center">
                <AudioLines className="w-4 h-4 text-brand-violet dark:text-brand-lavender" />
              </div>
              <div>
                <h4 className="text-sm font-medium text-foreground">
                  {syncStage === 'uploading' && 'Uploading Narration to Storage...'}
                  {syncStage === 'preparing' && 'Checking Audio & Measuring Loudness...'}
                  {syncStage === 'transcribing' && 'Transcribing Spoken Words...'}
                  {syncStage === 'aligning' && 'Aligning Narration & Snapping Pauses...'}
                </h4>
                <p className="text-xs text-muted-foreground">
                  {narrationState?.filename || 'Preparing audio'} · Stage{' '}
                  {syncStage === 'uploading'
                    ? '1'
                    : syncStage === 'preparing'
                    ? '2'
                    : syncStage === 'transcribing'
                    ? '3'
                    : '4'}{' '}
                  of 4
                </p>
              </div>
            </div>
            <span className="text-sm font-mono font-semibold text-brand-violet dark:text-brand-lavender">
              {progress}%
            </span>
          </div>

          <div className="w-full bg-secondary h-2 rounded-sm overflow-hidden">
            <div
              className="bg-brand-violet dark:bg-brand-lavender h-full transition-all duration-300 rounded-sm"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}

      {/* Player & Results */}
      {hasNarration && syncStage !== 'uploading' && (
        <div className="space-y-6">
          <div className="border border-border bg-card rounded-lg p-4 space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-2 pb-2.5 border-b border-border">
              <div className="flex items-center gap-2.5 flex-wrap">
                <Volume2 className="w-4 h-4 text-brand-violet dark:text-brand-lavender" />
                <span className="text-xs font-semibold text-foreground">
                  {narrationState?.filename}
                </span>
                {narrationState?.bytes != null && (
                  <span className="text-[11px] font-mono text-muted-foreground">
                    ({(narrationState.bytes / (1024 * 1024)).toFixed(2)} MB)
                  </span>
                )}
                {narrationState?.loudness_lufs != null && (
                  <span className="text-[11px] font-mono text-muted-foreground">
                    · {narrationState.loudness_lufs.toFixed(1)} LUFS
                  </span>
                )}
                {narrationState?.transcript && (
                  <span className="text-[11px] font-mono text-muted-foreground">
                    · {narrationState.transcript.engine} ({narrationState.transcript.word_count}{' '}
                    words)
                  </span>
                )}
              </div>

              {syncStage === 'completed' && (
                <div className="flex items-center gap-1.5 text-emerald-700 dark:text-emerald-300 text-xs font-mono font-medium bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/30">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  <span>Alignment Complete</span>
                </div>
              )}
            </div>

            {durationMs > 0 && (
              <>
                <WaveformViewer
                  durationMs={durationMs}
                  currentPlayTimeMs={playTimeMs}
                  boundaries={boundaries}
                  silences={silences}
                  peaks={peaks}
                  onSeek={seekTo}
                />

                <div className="flex items-center justify-between pt-1">
                  <div className="flex items-center gap-2">
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={togglePlay}
                      disabled={!narrationState?.audio_url}
                      className="h-8 px-3 text-xs"
                    >
                      {isPlaying ? (
                        <>
                          <Pause className="w-3.5 h-3.5 mr-1.5" />
                          Pause
                        </>
                      ) : (
                        <>
                          <Play className="w-3.5 h-3.5 mr-1.5" />
                          Play Narration
                        </>
                      )}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => seekTo(0)}
                      className="h-8 px-2 text-xs text-zinc-600 dark:text-zinc-400"
                    >
                      <RotateCcw className="w-3.5 h-3.5 mr-1" />
                      Restart
                    </Button>
                  </div>

                  <span className="text-xs font-mono text-zinc-500">
                    {formatDuration(playTimeMs / 1000)} / {formatDuration(durationMs / 1000)}
                  </span>
                </div>
              </>
            )}
          </div>

          {boundaries.length >= 2 && (
            <BoundaryTable
              boundaries={boundaries}
              narrationDurationMs={durationMs}
              onJumpToTime={seekTo}
              onNudgeBoundary={(index, deltaMs) => void handleNudgeBoundary(index, deltaMs)}
            />
          )}

          {subtitleCues.length > 0 && (
            <div className="border border-border bg-card rounded-lg overflow-hidden">
              <div className="px-4 py-3 border-b border-border bg-secondary/50 flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-foreground">
                  Generated Script Subtitles ({subtitleCues.length} Cues)
                </h3>
                <span className="text-xs text-muted-foreground">
                  Synced to spoken words · Click edit to refine any line
                </span>
              </div>
              <div className="max-h-64 overflow-y-auto divide-y divide-border p-2 space-y-1">
                {subtitleCues.map((cue) => {
                  const isEditing = editingCueId === cue.id;
                  return (
                    <div
                      key={cue.id}
                      className="flex items-start justify-between gap-3 px-2 py-2 hover:bg-secondary/50 rounded text-xs"
                    >
                      <button
                        type="button"
                        onClick={() => seekTo(cue.start_ms)}
                        className="font-mono text-muted-foreground hover:text-brand-violet dark:hover:text-brand-lavender shrink-0 pt-0.5 text-left"
                        title="Jump playback to subtitle"
                      >
                        [{formatDuration(cue.start_ms / 1000)} →{' '}
                        {formatDuration(cue.end_ms / 1000)}]
                      </button>

                      {isEditing ? (
                        <div className="flex-1 flex items-center gap-2">
                          <input
                            type="text"
                            value={editingCueText}
                            onChange={(e) => setEditingCueText(e.target.value)}
                            className="flex-1 px-2 py-1 rounded border border-input bg-background text-foreground text-xs"
                          />
                          <Button
                            size="sm"
                            onClick={() => void handleSaveSubtitleCue(cue)}
                            className="h-7 px-2.5 text-xs"
                          >
                            Save
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                              setEditingCueId(null);
                              setEditingCueText('');
                            }}
                            className="h-7 px-2 text-xs"
                          >
                            Cancel
                          </Button>
                        </div>
                      ) : (
                        <div className="flex-1 flex items-start justify-between gap-2">
                          <span className="text-foreground whitespace-pre-line font-medium">
                            {cue.text}
                            {cue.edited && (
                              <span className="ml-2 text-[10px] font-mono text-brand-violet dark:text-brand-lavender">
                                (edited)
                              </span>
                            )}
                          </span>
                          <button
                            type="button"
                            onClick={() => {
                              setEditingCueId(cue.id);
                              setEditingCueText(cue.text);
                            }}
                            className="text-muted-foreground hover:text-foreground p-1 shrink-0"
                            title="Edit subtitle text"
                          >
                            <Pencil className="w-3 h-3" />
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
