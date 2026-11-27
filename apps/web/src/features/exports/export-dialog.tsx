"use client";

import React, { useEffect, useMemo, useState } from "react";
import type { ExportResolution, ExportResponse } from "@pace/shared-types";
import {
  CheckCircle2,
  Download,
  Film,
  Loader2,
  RefreshCw,
  AlertTriangle,
  Clock,
  HardDrive,
} from "lucide-react";
import { api } from "@/lib/api-client";
import { formatBytes } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  estimateExportStats,
  formatDurationShort,
  formatRenderStage,
  getResolutionOptions,
} from "./export-utils";

interface ExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectId: string;
  projectName: string;
  aspectRatio: "16:9" | "9:16";
  fps: number;
  durationMs: number;
}

export function ExportDialog({
  open,
  onOpenChange,
  projectId,
  projectName,
  aspectRatio,
  fps,
  durationMs,
}: ExportDialogProps) {
  const [selectedRes, setSelectedRes] = useState<ExportResolution>("720p");
  const [exportsList, setExportsList] = useState<ExportResponse[]>([]);
  const [activeExport, setActiveExport] = useState<ExportResponse | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [liveDurationMs, setLiveDurationMs] = useState<number>(durationMs);
  const [unassignedScenes, setUnassignedScenes] = useState<number>(0);

  const resolutionOptions = useMemo(
    () => getResolutionOptions(aspectRatio),
    [aspectRatio]
  );

  const estimate = useMemo(
    () => estimateExportStats(selectedRes, liveDurationMs),
    [selectedRes, liveDurationMs]
  );

  useEffect(() => {
    if (!open) return;
    let active = true;
    setLoadingList(true);
    setErrorMsg(null);
    setLiveDurationMs(durationMs);

    Promise.all([
      api.exports.list(projectId),
      api.timeline.get(projectId).catch(() => null),
    ])
      .then(([rows, timeline]) => {
        if (!active) return;
        setExportsList(rows);
        setActiveExport(rows[0] ?? null);
        if (timeline) {
          if (timeline.duration_ms > 0) {
            setLiveDurationMs(timeline.duration_ms);
          }
          setUnassignedScenes(
            timeline.scenes.filter((s) => !s.media_id).length
          );
        }
      })
      .catch((err) => {
        if (active) {
          setErrorMsg(err?.message || "Could not load export history");
        }
      })
      .finally(() => {
        if (active) setLoadingList(false);
      });

    return () => {
      active = false;
    };
  }, [open, projectId, durationMs]);

  // Live SSE progress subscription with polling fallback while activeExport is queued/running
  useEffect(() => {
    if (!open || !activeExport) return;
    if (activeExport.status !== "queued" && activeExport.status !== "running") {
      return;
    }

    let closed = false;
    const exportId = activeExport.id;

    const applyUpdate = (updated: ExportResponse) => {
      if (closed) return;
      setActiveExport(updated);
      setExportsList((prev) => {
        const exists = prev.some((item) => item.id === updated.id);
        if (!exists) return [updated, ...prev];
        return prev.map((item) => (item.id === updated.id ? updated : item));
      });
    };

    let source: EventSource | null = null;
    let timer: number | null = null;

    const startFallbackPoll = () => {
      if (timer !== null || closed) return;
      timer = window.setInterval(() => {
        api.exports
          .get(projectId, exportId)
          .then(applyUpdate)
          .catch(() => {});
      }, 2500);
    };

    if (typeof EventSource !== "undefined") {
      try {
        source = new EventSource(api.exports.eventsUrl(projectId, exportId), {
          withCredentials: true,
        });
        source.addEventListener("progress", (ev: MessageEvent) => {
          try {
            const parsed = JSON.parse(ev.data) as ExportResponse;
            applyUpdate(parsed);
          } catch {
            // Ignore malformed chunk
          }
        });
        source.onerror = () => {
          if (source) {
            source.close();
            source = null;
          }
          startFallbackPoll();
        };
      } catch {
        source = null;
        startFallbackPoll();
      }
    } else {
      startFallbackPoll();
    }

    return () => {
      closed = true;
      if (timer !== null) window.clearInterval(timer);
      if (source) source.close();
    };
  }, [open, projectId, activeExport?.id, activeExport?.status]);

  const handleStartExport = async () => {
    setSubmitting(true);
    setErrorMsg(null);
    try {
      const created = await api.exports.start(projectId, {
        resolution: selectedRes,
      });
      setActiveExport(created);
      setExportsList((prev) => [created, ...prev]);
    } catch (err: unknown) {
      const msg =
        err instanceof Error ? err.message : "Failed to start MP4 export";
      setErrorMsg(msg);
    } finally {
      setSubmitting(false);
    }
  };

  const handleRetry = async (exportId: string) => {
    setSubmitting(true);
    setErrorMsg(null);
    try {
      const retried = await api.exports.retry(projectId, exportId);
      setActiveExport(retried);
      setExportsList((prev) =>
        prev.map((item) => (item.id === retried.id ? retried : item))
      );
    } catch (err: unknown) {
      const msg =
        err instanceof Error ? err.message : "Failed to retry MP4 export";
      setErrorMsg(msg);
    } finally {
      setSubmitting(false);
    }
  };

  const isExportInFlight =
    activeExport?.status === "queued" || activeExport?.status === "running";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        onClose={() => onOpenChange(false)}
        className="max-h-[90vh] overflow-y-auto"
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <Film className="h-4 w-4 text-brand-violet dark:text-brand-lavender" />
            <span>Export Video</span>
          </DialogTitle>
          <DialogDescription>
            Render <span className="font-medium text-foreground">{projectName}</span>{" "}
            ({aspectRatio} · {fps} FPS · H.264 + AAC MP4)
          </DialogDescription>
        </DialogHeader>

        {/* Resolution Selector */}
        <div className="mt-4 space-y-2.5">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-mono uppercase tracking-wider text-muted-foreground">
              Resolution Tier (Max 1080p)
            </span>
            <span className="text-[11px] font-mono text-muted-foreground">
              {resolutionOptions.find((r) => r.id === selectedRes)?.width} ×{" "}
              {resolutionOptions.find((r) => r.id === selectedRes)?.height}
            </span>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
            {resolutionOptions.map((opt) => {
              const selected = selectedRes === opt.id;
              return (
                  <button
                  key={opt.id}
                  type="button"
                  onClick={() => setSelectedRes(opt.id)}
                  className={`flex flex-col items-start rounded-xl border px-3.5 py-2.5 text-left transition-all duration-150 ${
                    selected
                      ? "border-brand-lavender bg-brand-lavender/12 text-foreground ring-1 ring-brand-lavender/40"
                      : "border-border/70 bg-secondary/40 text-muted-foreground hover:text-foreground hover:bg-secondary/75"
                  }`}
                >
                  <span className="text-sm font-semibold text-foreground">
                    {opt.label}
                  </span>
                  <span className="text-[11px] font-mono text-muted-foreground">
                    {opt.width}×{opt.height}
                  </span>
                  <span className="mt-1 text-[11px] font-medium text-brand-violet dark:text-brand-lavender">
                    {opt.tag}
                  </span>
                </button>
              );
            })}
          </div>

          {/* Time & File Size Estimate */}
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl bg-secondary/45 px-3.5 py-2.5 text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <Clock className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender" />
              Est. render:{" "}
              <strong className="text-foreground font-mono tabular-nums">
                ~{formatDurationShort(estimate.estimatedRenderSeconds)}
              </strong>
            </span>
            <span className="flex items-center gap-1.5">
              <HardDrive className="h-3.5 w-3.5 text-brand-violet dark:text-brand-lavender" />
              Est. file:{" "}
              <strong className="text-foreground font-mono tabular-nums">
                ~{formatBytes(estimate.estimatedBytes)}
              </strong>
            </span>
          </div>
        </div>

        {unassignedScenes > 0 && (
          <div className="mt-3 flex items-start gap-2 rounded-xl border border-amber-500/30 bg-amber-500/10 px-3.5 py-2.5 text-xs text-amber-800 dark:text-amber-300">
            <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
            <span>
              {unassignedScenes} scene(s) do not have an assigned image yet. Assign images in Media before rendering.
            </span>
          </div>
        )}

        {errorMsg && (
          <div className="mt-3 flex items-start gap-2 rounded-xl border border-destructive/35 bg-destructive/10 px-3.5 py-2.5 text-xs text-destructive">
            <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
            <span>{errorMsg}</span>
          </div>
        )}

        <div className="mt-4">
          <Button
            type="button"
            className="w-full h-10 font-semibold"
            onClick={handleStartExport}
            disabled={submitting || isExportInFlight || unassignedScenes > 0}
          >
            {submitting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Queueing render job...
              </>
            ) : isExportInFlight ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Rendering {activeExport?.resolution}...
              </>
            ) : (
              <>
                <Film className="h-4 w-4 mr-2" />
                Render {selectedRes} MP4
              </>
            )}
          </Button>
        </div>

        {/* Active Export Live Status */}
        {activeExport && (
          <div className="mt-4 rounded-xl border border-border/70 bg-secondary/35 p-4 space-y-2.5">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                {activeExport.status === "succeeded" ? (
                  <CheckCircle2 className="h-4 w-4 text-emerald-500 shrink-0" />
                ) : activeExport.status === "failed" ? (
                  <AlertTriangle className="h-4 w-4 text-destructive shrink-0" />
                ) : (
                  <Loader2 className="h-4 w-4 text-brand-violet dark:text-brand-lavender animate-spin shrink-0" />
                )}
                <span>
                  {activeExport.resolution} ({activeExport.width}×
                  {activeExport.height})
                </span>
              </div>
              <span className="text-[11px] font-mono tabular-nums uppercase tracking-wider text-muted-foreground">
                {activeExport.status} · {activeExport.progress}%
              </span>
            </div>

            {(activeExport.status === "queued" ||
              activeExport.status === "running") && (
              <>
                <div className="h-2 w-full overflow-hidden rounded-full bg-background/70">
                  <div
                    className="h-full rounded-full bg-brand-violet dark:bg-brand-lavender transition-all duration-300"
                    style={{ width: `${Math.max(4, activeExport.progress)}%` }}
                  />
                </div>
                <p className="text-xs font-mono text-muted-foreground">
                  {formatRenderStage(activeExport.stage)}
                </p>
              </>
            )}

            {activeExport.status === "succeeded" && activeExport.download_url && (
              <div className="space-y-2 pt-1">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                  <div className="text-[11px] font-mono text-muted-foreground">
                    {activeExport.size_bytes
                      ? `${formatBytes(activeExport.size_bytes)} · `
                      : ""}
                    {activeExport.validation?.video_codec?.toUpperCase() || "H264"}{" "}
                    +{" "}
                    {activeExport.validation?.audio_codec?.toUpperCase() || "AAC"}{" "}
                    verified
                  </div>
                  <a
                    href={activeExport.download_url}
                    download={activeExport.download_filename || "pace-export.mp4"}
                    className="inline-flex items-center justify-center gap-1.5 rounded-[10px] bg-brand-violet dark:bg-brand-lavender dark:text-[#140B29] px-4 py-2 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
                  >
                    <Download className="h-3.5 w-3.5" />
                    <span>Download MP4</span>
                  </a>
                </div>

                {/* STO-02: 3-day export retention & expiry warning */}
                {(() => {
                  const createdAt = new Date(activeExport.created_at).getTime();
                  const expiresAt = createdAt + 3 * 24 * 60 * 60 * 1000;
                  const msRemaining = expiresAt - Date.now();
                  const hoursRemaining = Math.max(0, Math.floor(msRemaining / (1000 * 60 * 60)));
                  const isExpiringSoon = hoursRemaining < 24;

                  return isExpiringSoon ? (
                    <div className="flex items-center gap-1.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-300">
                      <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-400" />
                      <span>
                        Expires in {hoursRemaining}h — exports are automatically cleaned up after 3 days. Download now to keep a local copy.
                      </span>
                    </div>
                  ) : (
                    <div className="text-[11px] font-mono text-muted-foreground/75 flex flex-wrap items-center justify-between gap-1 border-t border-border/40 pt-2">
                      <span>Exports kept for 3 days · Expires {new Date(expiresAt).toLocaleDateString()}</span>
                      <span className="text-muted-foreground/60">(re-renders remain instant via cache)</span>
                    </div>
                  );
                })()}
              </div>
            )}

            {activeExport.status === "failed" && (
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-1">
                <p className="text-xs text-destructive">
                  {activeExport.error_message || "Render worker reported an error"}
                </p>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => handleRetry(activeExport.id)}
                  disabled={submitting}
                  className="shrink-0"
                >
                  <RefreshCw className="h-3.5 w-3.5 mr-1.5" />
                  Retry (Resume Cache)
                </Button>
              </div>
            )}
          </div>
        )}

        {/* Recent Exports History */}
        {exportsList.length > 1 && (
          <div className="mt-4 pt-3 border-t border-border space-y-2">
            <div className="flex items-center justify-between text-[11px] font-mono uppercase tracking-wider text-muted-foreground">
              <span>Previous Exports</span>
              {loadingList && <span>Refreshing...</span>}
            </div>
            <div className="space-y-1.5 max-h-40 overflow-y-auto pr-1">
              {exportsList.slice(1).map((exp) => (
                <div
                  key={exp.id}
                  className="flex items-center justify-between gap-2 rounded-md border border-border bg-secondary/20 px-2.5 py-1.5 text-xs"
                >
                  <div className="min-w-0">
                    <span className="font-mono font-semibold text-foreground">
                      {exp.resolution}
                    </span>{" "}
                    <span className="text-muted-foreground">
                      ({exp.width}×{exp.height}) · {exp.status}
                    </span>
                  </div>
                  {exp.status === "succeeded" && exp.download_url ? (
                    <a
                      href={exp.download_url}
                      download={exp.download_filename || "pace-export.mp4"}
                      className="inline-flex items-center gap-1 rounded border border-border bg-card px-2 py-1 text-[11px] font-medium text-foreground hover:bg-secondary"
                    >
                      <Download className="h-3 w-3" />
                      MP4
                    </a>
                  ) : exp.status === "failed" ? (
                    <button
                      type="button"
                      onClick={() => handleRetry(exp.id)}
                      className="inline-flex items-center gap-1 rounded border border-border bg-card px-2 py-1 text-[11px] font-medium text-foreground hover:bg-secondary"
                    >
                      <RefreshCw className="h-3 w-3" />
                      Retry
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
