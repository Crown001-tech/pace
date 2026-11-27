"use client";

import React, { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Wand2,
  ArrowDownAZ,
  AlertTriangle,
  Images,
  Layers,
  Search,
  Sparkles,
  UploadCloud,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useScriptEditorStore } from "@/stores/script-editor-store";
import {
  useMediaStore,
  calculateMismatchSummary,
  validateExportReadiness,
  naturalSortFiles,
  MediaItem,
  SectionRef,
} from "./media-store";
import { useMediaUpload } from "./use-media-upload";
import { Dropzone } from "./dropzone";
import { MediaCard } from "./media-card";
import { SectionAssignmentCard } from "./section-assignment-card";
import { MediaSourcesPanel, toStoreMediaItem } from "./media-sources-panel";
import { api } from "@/lib/api-client";

interface StepMediaViewProps {
  projectId: string;
  projectAspectRatio: "16:9" | "9:16";
  onNextStep: () => void;
}

export function StepMediaView({
  projectId,
  projectAspectRatio,
  onNextStep,
}: StepMediaViewProps) {
  const scriptSections = useScriptEditorStore((s) => s.sections);

  const {
    media,
    assignments,
    sortMode,
    setMedia,
    setSortMode,
    removeMediaItem,
    setAssignments,
    assignToSection,
    unassignSection,
    autoAssign,
    swapSections,
    updateFocalPoint,
    updateFitMode,
  } = useMediaStore();

  const [mobileTab, setMobileTab] = useState<"scenes" | "library">("scenes");
  const [binMode, setBinMode] = useState<"upload" | "stock" | "ai">("upload");
  const [selectedMediaId, setSelectedMediaId] = useState<string | null>(null);
  const [targetSectionId, setTargetSectionId] = useState<string | null>(null);

  const sectionRefs: SectionRef[] = useMemo(
    () =>
      scriptSections.map((sec, idx) => ({
        id: sec.id,
        position: idx,
        text: sec.text,
      })),
    [scriptSections]
  );

  // Hydrate media library & assignments from backend
  useEffect(() => {
    let active = true;

    async function load() {
      try {
        const [mediaResp, assignResp] = await Promise.all([
          api.media.list(projectId),
          api.assignments.list(projectId),
        ]);
        if (!active) return;

        const mappedMedia: MediaItem[] = mediaResp.map((m) => {
          const w = m.width ?? 16;
          const h = m.height ?? 9;
          return {
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
          };
        });
        setMedia(mappedMedia);

        const mappedAssignments = sectionRefs.map((sec) => {
          const hit = assignResp.find((a) => a.section_id === sec.id);
          return hit
            ? {
              sectionId: sec.id,
              mediaId: hit.media_id,
              fitMode: (hit.fit_mode === "fit_blur" ? "fit_blur" : "fill") as
                | "fill"
                | "fit_blur",
              focalX: hit.focal_x,
              focalY: hit.focal_y,
              effect: hit.effect,
              effectOverridden: hit.effect_overridden,
            }
            : {
              sectionId: sec.id,
              mediaId: null,
              fitMode: "fill" as const,
              focalX: 0.5,
              focalY: 0.5,
            };
        });
        setAssignments(mappedAssignments);
      } catch {
        // Offline / local fallback: ensure assignments match sections
        if (!active) return;
        const next = sectionRefs.map((sec) => {
          const existing = assignments.find((a) => a.sectionId === sec.id);
          return (
            existing ?? {
              sectionId: sec.id,
              mediaId: null,
              fitMode: "fill" as const,
              focalX: 0.5,
              focalY: 0.5,
            }
          );
        });
        setAssignments(next);
      }
    }

    load();
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, sectionRefs.length]);

  const { handleUploadFiles, isProcessing } = useMediaUpload({
    projectId,
    projectAspectRatio,
  });

  const sortedMedia = useMemo(() => {
    if (sortMode === "upload") return media;
    const names = naturalSortFiles(media.map((m) => m.filename));
    return [...media].sort(
      (a, b) => names.indexOf(a.filename) - names.indexOf(b.filename)
    );
  }, [media, sortMode]);

  const mismatch = useMemo(
    () => calculateMismatchSummary(sectionRefs.length, media.length),
    [sectionRefs.length, media.length]
  );

  const readiness = useMemo(
    () => validateExportReadiness(assignments),
    [assignments]
  );

  const handleAssignMedia = async (sectionId: string, mediaId: string) => {
    assignToSection(sectionId, mediaId);
    setSelectedMediaId(null);
    setTargetSectionId(null);
    try {
      await api.assignments.assign(projectId, sectionId, {
        media_id: mediaId,
        fit_mode: "fill",
        focal_x: 0.5,
        focal_y: 0.5,
      });
    } catch { }
  };

  const handleUnassign = async (sectionId: string) => {
    unassignSection(sectionId);
    try {
      await api.assignments.unassign(projectId, sectionId);
    } catch { }
  };

  const handleAutoAssign = async () => {
    autoAssign(sectionRefs);
    try {
      await api.assignments.auto(projectId);
    } catch { }
  };

  const handleSwap = async (sectionIdA: string, sectionIdB: string) => {
    swapSections(sectionIdA, sectionIdB);
    try {
      await api.assignments.swap(projectId, {
        section_id_a: sectionIdA,
        section_id_b: sectionIdB,
      });
    } catch { }
  };

  const handleUpdateFocalPoint = async (
    sectionId: string,
    focalX: number,
    focalY: number
  ) => {
    updateFocalPoint(sectionId, focalX, focalY);
    const current = assignments.find((a) => a.sectionId === sectionId);
    if (!current?.mediaId) return;
    try {
      await api.assignments.assign(projectId, sectionId, {
        media_id: current.mediaId,
        fit_mode: current.fitMode,
        focal_x: focalX,
        focal_y: focalY,
      });
    } catch { }
  };

  const handleToggleFitMode = async (sectionId: string) => {
    const current = assignments.find((a) => a.sectionId === sectionId);
    const nextFit = current?.fitMode === "fit_blur" ? "fill" : "fit_blur";
    updateFitMode(sectionId, nextFit);
    if (!current?.mediaId) return;
    try {
      await api.assignments.assign(projectId, sectionId, {
        media_id: current.mediaId,
        fit_mode: nextFit,
        focal_x: current.focalX,
        focal_y: current.focalY,
      });
    } catch { }
  };

  const handleDeleteMedia = async (mediaId: string) => {
    removeMediaItem(mediaId);
    if (selectedMediaId === mediaId) setSelectedMediaId(null);
    try {
      await api.media.delete(projectId, mediaId);
    } catch { }
  };

  // Touch-friendly 2-tap assignment helper
  const handleSectionTap = (sectionId: string) => {
    if (selectedMediaId) {
      handleAssignMedia(sectionId, selectedMediaId);
      return;
    }
    setTargetSectionId(sectionId);
    setMobileTab("library");
  };

  const handleMediaSelect = (item: MediaItem) => {
    if (targetSectionId) {
      handleAssignMedia(targetSectionId, item.id);
      setMobileTab("scenes");
      return;
    }
    setSelectedMediaId((prev) => (prev === item.id ? null : item.id));
  };

  const findAssignedSectionPos = (mediaId: string): number | undefined => {
    const found = assignments.find((a) => a.mediaId === mediaId);
    if (!found) return undefined;
    return sectionRefs.findIndex((s) => s.id === found.sectionId);
  };

  const emptySections = useMemo(() => {
    const assignedSet = new Set(
      assignments.filter((a) => Boolean(a.mediaId)).map((a) => a.sectionId)
    );
    return sectionRefs.filter((s) => !assignedSet.has(s.id));
  }, [assignments, sectionRefs]);

  const handleRemoteMediaImported = (
    saved: import("@pace/shared-types").MediaResponse,
    assignedSectionId?: string
  ) => {
    const item = toStoreMediaItem(saved, projectAspectRatio);
    useMediaStore.getState().addMediaItem(item);
    if (assignedSectionId) {
      assignToSection(assignedSectionId, item.id);
      setTargetSectionId(null);
    }
  };

  const reloadAssignmentsAndMedia = async () => {
    try {
      const [mediaResp, assignResp] = await Promise.all([
        api.media.list(projectId),
        api.assignments.list(projectId),
      ]);
      setMedia(mediaResp.map((m) => toStoreMediaItem(m, projectAspectRatio)));
      const mapped = sectionRefs.map((sec) => {
        const hit = assignResp.find((a) => a.section_id === sec.id);
        return hit
          ? {
              sectionId: sec.id,
              mediaId: hit.media_id,
              fitMode: (hit.fit_mode === "fit_blur" ? "fit_blur" : "fill") as
                | "fill"
                | "fit_blur",
              focalX: hit.focal_x,
              focalY: hit.focal_y,
              effect: hit.effect,
              effectOverridden: hit.effect_overridden,
            }
          : {
              sectionId: sec.id,
              mediaId: null,
              fitMode: "fill" as const,
              focalX: 0.5,
              focalY: 0.5,
            };
      });
      setAssignments(mapped);
    } catch { }
  };

  return (
    <div className="mx-auto w-full max-w-[1600px] px-4 sm:px-6 py-5 sm:py-6 space-y-5">
      {/* Top Toolbar */}
      <div className="flex flex-col gap-3 rounded-2xl border border-border/65 bg-card p-3.5 sm:px-5 sm:py-4 studio-card">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2.5">
            <h2 className="text-sm font-semibold text-foreground">
              Scene Visuals
            </h2>
            <Badge variant={mismatch.hasEmptySections ? "warning" : "success"}>
              {mismatch.text}
            </Badge>
            {selectedMediaId && (
              <Badge variant="default">
                Image selected: tap any scene to assign
              </Badge>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                setSortMode(sortMode === "natural" ? "upload" : "natural")
              }
            >
              <ArrowDownAZ className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">
                Sort: {sortMode === "natural" ? "Natural" : "Upload Order"}
              </span>
              <span className="sm:hidden">
                {sortMode === "natural" ? "A–Z" : "Order"}
              </span>
            </Button>

            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={handleAutoAssign}
              disabled={media.length === 0}
            >
              <Wand2 className="h-3.5 w-3.5" />
              <span>1:1 Auto-Assign</span>
            </Button>

            <Button type="button" size="sm" onClick={onNextStep}>
              <span>Narration</span>
              <ArrowRight className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        {!readiness.canExport && (
          <div className="flex items-center gap-2 rounded-[10px] border border-amber-500/30 bg-amber-500/10 px-3.5 py-2 text-xs text-amber-800 dark:text-amber-300">
            <AlertTriangle className="h-4 w-4 shrink-0 text-amber-500" />
            <span>
              {readiness.emptySectionIds.length} scene
              {readiness.emptySectionIds.length === 1 ? "" : "s"} still need an
              assigned image before export.
            </span>
          </div>
        )}
      </div>

      {/* Mobile View Switcher (< lg) */}
      <div className="grid grid-cols-2 gap-1 rounded-[12px] bg-secondary/75 p-1 lg:hidden">
        <button
          type="button"
          onClick={() => setMobileTab("scenes")}
          className={`flex items-center justify-center gap-2 rounded-[8px] py-2 text-xs font-semibold transition-all ${mobileTab === "scenes"
            ? "bg-card text-foreground shadow-sm"
            : "text-muted-foreground"
            }`}
        >
          <Layers className="h-3.5 w-3.5" />
          Scenes ({sectionRefs.length})
        </button>
        <button
          type="button"
          onClick={() => setMobileTab("library")}
          className={`flex items-center justify-center gap-2 rounded-[8px] py-2 text-xs font-semibold transition-all ${mobileTab === "library"
            ? "bg-card text-foreground shadow-sm"
            : "text-muted-foreground"
            }`}
        >
          <Images className="h-3.5 w-3.5" />
          Asset Bin ({media.length})
        </button>
      </div>

      {/* Desktop 2-Pane Workbench / Mobile Switched Pane */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-5 items-start">
        {/* Left: Scene Sequence Slots */}
        <div
          className={`lg:col-span-7 space-y-3 ${mobileTab === "scenes" ? "block" : "hidden lg:block"
            }`}
        >
          {sectionRefs.map((section) => {
            const assignment = assignments.find((a) => a.sectionId === section.id);
            const assignedMedia = assignment?.mediaId
              ? media.find((m) => m.id === assignment.mediaId)
              : undefined;

            return (
              <SectionAssignmentCard
                key={section.id}
                section={section}
                assignment={assignment}
                assignedMedia={assignedMedia}
                projectAspectRatio={projectAspectRatio}
                pendingMediaSelected={Boolean(selectedMediaId)}
                onTapAssign={handleSectionTap}
                onAssignMedia={handleAssignMedia}
                onUnassign={handleUnassign}
                onSwapWithSection={handleSwap}
                onUpdateFocalPoint={handleUpdateFocalPoint}
                onToggleFitMode={handleToggleFitMode}
              />
            );
          })}
        </div>

        {/* Right: Media Asset Bin */}
        <div
          className={`lg:col-span-5 lg:sticky lg:top-28 space-y-4 rounded-2xl border border-border/65 bg-card p-4 sm:p-5 studio-card ${mobileTab === "library" ? "block" : "hidden lg:block"
            }`}
        >
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-xs font-mono font-semibold uppercase tracking-wider text-foreground">
                Media Asset Bin
              </h3>
              <p className="text-[11px] text-muted-foreground mt-0.5">
                {targetSectionId
                  ? "Tap an image below to assign it to the selected scene"
                  : "Drag onto a scene or tap an image then tap a scene"}
              </p>
            </div>
            {targetSectionId && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setTargetSectionId(null)}
              >
                Cancel
              </Button>
            )}
          </div>

          <div className="grid grid-cols-3 gap-1 rounded-[10px] bg-secondary/75 p-1">
            {[
              { id: "upload" as const, label: "Gallery", icon: UploadCloud },
              { id: "stock" as const, label: "Stock $0", icon: Search },
              { id: "ai" as const, label: "AI Gen $0", icon: Sparkles },
            ].map((tab) => {
              const Icon = tab.icon;
              const active = binMode === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setBinMode(tab.id)}
                  className={`flex min-h-[34px] items-center justify-center gap-1.5 rounded-[7px] text-[11px] font-semibold transition-all touch-manipulation ${
                    active
                      ? "bg-card text-foreground shadow-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <Icon className="h-3.5 w-3.5" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>

          {binMode === "upload" ? (
            <Dropzone onFilesSelected={handleUploadFiles} disabled={isProcessing} />
          ) : (
            <MediaSourcesPanel
              mode={binMode}
              projectId={projectId}
              projectAspectRatio={projectAspectRatio}
              targetSectionId={targetSectionId}
              emptySections={emptySections}
              onMediaImported={handleRemoteMediaImported}
              onBatchComplete={reloadAssignmentsAndMedia}
            />
          )}

          {sortedMedia.length > 0 && (
            <div className="grid grid-cols-2 gap-2.5 max-h-[54vh] overflow-y-auto pr-1">
              {sortedMedia.map((item) => {
                const pos = findAssignedSectionPos(item.id);
                return (
                  <MediaCard
                    key={item.id}
                    item={item}
                    projectAspectRatio={projectAspectRatio}
                    isAssigned={pos !== undefined && pos >= 0}
                    assignedSectionPosition={pos}
                    selected={selectedMediaId === item.id}
                    onSelect={handleMediaSelect}
                    onDelete={handleDeleteMedia}
                  />
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
