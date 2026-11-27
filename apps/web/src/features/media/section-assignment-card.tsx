"use client";

import React, { useState } from "react";
import { Image as ImageIcon, X, Crosshair, ArrowLeftRight, Plus } from "lucide-react";
import { MediaItem, SectionAssignment, SectionRef } from "./media-store";

interface SectionAssignmentCardProps {
  section: SectionRef;
  assignment?: SectionAssignment;
  assignedMedia?: MediaItem;
  projectAspectRatio: "16:9" | "9:16";
  pendingMediaSelected?: boolean;
  onTapAssign?: (sectionId: string) => void;
  onAssignMedia: (sectionId: string, mediaId: string) => void;
  onUnassign: (sectionId: string) => void;
  onSwapWithSection: (sectionIdA: string, sectionIdB: string) => void;
  onUpdateFocalPoint: (sectionId: string, focalX: number, focalY: number) => void;
  onToggleFitMode: (sectionId: string) => void;
}

export function SectionAssignmentCard({
  section,
  assignment,
  assignedMedia,
  pendingMediaSelected = false,
  onTapAssign,
  onAssignMedia,
  onUnassign,
  onSwapWithSection,
  onUpdateFocalPoint,
  onToggleFitMode,
}: SectionAssignmentCardProps) {
  const [isDragOver, setIsDragOver] = useState(false);
  const [isSettingFocal, setIsSettingFocal] = useState(false);

  const isEmpty = !assignment || !assignment.mediaId || !assignedMedia;

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    try {
      const dataStr = e.dataTransfer.getData("text/plain");
      if (!dataStr) return;
      const data = JSON.parse(dataStr);

      if (data.sourceSectionId) {
        onSwapWithSection(data.sourceSectionId, section.id);
      } else if (data.mediaId) {
        onAssignMedia(section.id, data.mediaId);
      }
    } catch {}
  };

  const handleDragStartFromSection = (e: React.DragEvent) => {
    if (!assignedMedia) return;
    e.dataTransfer.setData(
      "text/plain",
      JSON.stringify({
        sourceSectionId: section.id,
        mediaId: assignedMedia.id,
      })
    );
    e.dataTransfer.effectAllowed = "copyMove";
  };

  const handleFocalClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isSettingFocal) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    onUpdateFocalPoint(section.id, parseFloat(x.toFixed(3)), parseFloat(y.toFixed(3)));
    setIsSettingFocal(false);
  };

  const sceneNum = String(section.position + 1).padStart(2, "0");

  return (
    <div
      id={`section-assign-${section.id}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={`relative flex flex-col sm:flex-row gap-3.5 rounded-lg border p-3.5 transition-all ${
        isEmpty
          ? "border-red-500/40 bg-red-500/[0.04]"
          : "border-border bg-card hover:border-zinc-400 dark:hover:border-zinc-700"
      } ${isDragOver ? "ring-2 ring-brand-lavender border-brand-lavender" : ""} ${
        pendingMediaSelected ? "ring-1 ring-brand-lavender/60" : ""
      }`}
    >
      {/* Left: Scene Text & Controls */}
      <div className="flex-1 flex flex-col justify-between min-w-0 gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-2 mb-1.5">
            <span className="rounded border border-border bg-secondary px-1.5 py-0.5 font-mono text-[11px] font-semibold text-foreground">
              SCENE {sceneNum}
            </span>
            {isEmpty && (
              <span className="rounded border border-red-500/35 bg-red-500/10 px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase text-red-500">
                Missing Visual
              </span>
            )}
          </div>

          <p className="text-xs sm:text-sm text-foreground/90 line-clamp-3 leading-relaxed">
            {section.text || <span className="italic text-muted-foreground">Empty scene script</span>}
          </p>
        </div>

        {/* Visual Framing Controls */}
        {assignedMedia ? (
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <button
              type="button"
              onClick={() => onToggleFitMode(section.id)}
              className="rounded border border-border bg-secondary px-2 py-1 text-[11px] font-medium text-foreground hover:bg-secondary/80 transition-colors"
            >
              Framing: {assignment?.fitMode === "fit_blur" ? "Fit Blur" : "Fill"}
            </button>
            <button
              type="button"
              onClick={() => setIsSettingFocal(!isSettingFocal)}
              className={`inline-flex items-center gap-1 rounded border px-2 py-1 text-[11px] font-medium transition-colors ${
                isSettingFocal
                  ? "border-brand-lavender bg-brand-lavender text-[#140B29] font-semibold"
                  : "border-border bg-secondary text-foreground hover:bg-secondary/80"
              }`}
            >
              <Crosshair className="h-3 w-3" />
              {isSettingFocal ? "Tap Image" : "Focal"}
            </button>
            <span className="font-mono text-[11px] text-muted-foreground">
              {Math.round((assignment?.focalX ?? 0.5) * 100)}% ,{" "}
              {Math.round((assignment?.focalY ?? 0.5) * 100)}%
            </span>
          </div>
        ) : (
          pendingMediaSelected &&
          onTapAssign && (
            <div>
              <button
                type="button"
                onClick={() => onTapAssign(section.id)}
                className="inline-flex items-center gap-1.5 rounded bg-brand-lavender px-2.5 py-1 text-xs font-semibold text-black hover:opacity-90"
              >
                <Plus className="h-3.5 w-3.5" />
                Assign Selected Image
              </button>
            </div>
          )
        )}
      </div>

      {/* Right: Visual Slot */}
      <div className="w-full sm:w-48 shrink-0">
        {assignedMedia ? (
          <div
            draggable
            onDragStart={handleDragStartFromSection}
            onClick={(e) => {
              if (isSettingFocal) {
                handleFocalClick(e);
              } else if (pendingMediaSelected && onTapAssign) {
                onTapAssign(section.id);
              }
            }}
            className="group relative aspect-video w-full overflow-hidden rounded-md border border-border bg-zinc-950 cursor-grab active:cursor-grabbing"
          >
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={assignedMedia.thumbUrl}
              alt={assignedMedia.filename}
              className="h-full w-full object-cover"
            />

            {/* Architectural Crosshair Focal Reticle */}
            <div
              className="pointer-events-none absolute h-5 w-5 -ml-2.5 -mt-2.5 flex items-center justify-center border border-brand-lavender bg-black/25 shadow-sm"
              style={{
                left: `${(assignment?.focalX ?? 0.5) * 100}%`,
                top: `${(assignment?.focalY ?? 0.5) * 100}%`,
              }}
            >
              <span className="h-1 w-1 bg-brand-lavender" />
            </div>

            <div className="absolute top-1.5 left-1.5 rounded bg-black/80 p-1 text-zinc-200 opacity-0 group-hover:opacity-100 transition-opacity">
              <ArrowLeftRight className="h-3 w-3" />
            </div>

            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onUnassign(section.id);
              }}
              title="Unassign image"
              aria-label={`Unassign image from scene ${section.position + 1}`}
              className="absolute top-1.5 right-1.5 rounded border border-zinc-700/80 bg-black/85 p-1 text-zinc-300 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 hover:text-red-400 transition-opacity"
            >
              <X className="h-3 w-3" />
            </button>

            <div className="absolute bottom-0 inset-x-0 truncate bg-black/80 px-2 py-1 font-mono text-[10px] text-zinc-200">
              {assignedMedia.filename}
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => onTapAssign?.(section.id)}
            className={`aspect-video w-full rounded-md border border-dashed flex flex-col items-center justify-center p-3 text-center transition-colors ${
              isDragOver
                ? "border-brand-lavender bg-brand-lavender/10 text-brand-lavender"
                : pendingMediaSelected
                ? "border-brand-lavender bg-brand-lavender/12 text-brand-violet dark:text-brand-lavender"
                : "border-red-500/45 bg-red-500/5 text-red-600 dark:text-red-400 hover:bg-red-500/10"
            }`}
          >
            <ImageIcon className="h-5 w-5 mb-1 opacity-80" />
            <span className="text-xs font-medium">
              {pendingMediaSelected ? "Tap to Place Here" : "Assign Visual"}
            </span>
            <span className="text-[10px] opacity-75 mt-0.5 font-mono">
              Drag image or tap
            </span>
          </button>
        )}
      </div>
    </div>
  );
}
