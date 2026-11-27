"use client";

import React from "react";
import { Trash2, AlertTriangle, Check } from "lucide-react";
import { MediaItem } from "./media-store";
import { formatBytes } from "@/lib/utils";

interface MediaCardProps {
  item: MediaItem;
  projectAspectRatio: "16:9" | "9:16";
  isAssigned?: boolean;
  assignedSectionPosition?: number;
  selected?: boolean;
  onDelete?: (id: string) => void;
  onSelect?: (item: MediaItem) => void;
}

export function MediaCard({
  item,
  projectAspectRatio,
  isAssigned,
  assignedSectionPosition,
  selected = false,
  onDelete,
  onSelect,
}: MediaCardProps) {
  const isMismatch = item.aspectRatio !== projectAspectRatio;

  const handleDragStart = (e: React.DragEvent) => {
    e.dataTransfer.setData("text/plain", JSON.stringify({ mediaId: item.id }));
    e.dataTransfer.effectAllowed = "copyMove";
  };

  return (
    <div
      draggable
      onDragStart={handleDragStart}
      onClick={() => onSelect?.(item)}
      className={`group relative flex flex-col overflow-hidden rounded-md border bg-card transition-all cursor-grab active:cursor-grabbing ${
        selected
          ? "border-brand-lavender ring-2 ring-brand-lavender/40"
          : "border-border hover:border-brand-lavender/70"
      }`}
    >
      <div className="relative aspect-video w-full overflow-hidden bg-zinc-950 flex items-center justify-center">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={item.thumbUrl}
          alt={item.filename}
          className="h-full w-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
          loading="lazy"
        />

        {/* Top Left Tags */}
        <div className="absolute top-1.5 left-1.5 flex items-center gap-1">
          <span className="rounded bg-black/80 px-1.5 py-0.5 font-mono text-[10px] font-medium text-zinc-200 border border-zinc-700/70">
            {item.aspectRatio}
          </span>
          {isMismatch && (
            <span
              title={`Image aspect ratio (${item.aspectRatio}) differs from project (${projectAspectRatio}).`}
              className="inline-flex items-center gap-1 rounded bg-amber-500 px-1.5 py-0.5 text-[10px] font-semibold text-black"
            >
              <AlertTriangle className="h-2.5 w-2.5" />
              Crop
            </span>
          )}
        </div>

        {/* Assigned or Selected Indicator */}
        {selected ? (
          <div className="absolute top-1.5 right-1.5 flex items-center gap-1 rounded bg-brand-lavender px-1.5 py-0.5 font-mono text-[10px] font-bold text-[#140B29]">
            <Check className="h-3 w-3" />
            Picked
          </div>
        ) : (
          isAssigned &&
          assignedSectionPosition !== undefined && (
            <div className="absolute top-1.5 right-1.5 rounded bg-brand-violet/90 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-white border border-brand-lavender/40">
              S{String(assignedSectionPosition + 1).padStart(2, "0")}
            </div>
          )
        )}

        {/* Delete Button */}
        {onDelete && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(item.id);
            }}
            title="Delete image"
            aria-label={`Delete ${item.filename}`}
            className="absolute bottom-1.5 right-1.5 rounded border border-zinc-700/80 bg-black/80 p-1 text-zinc-300 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 hover:text-red-400 transition-opacity"
          >
            <Trash2 className="h-3 w-3" />
          </button>
        )}
      </div>

      {/* Footer Info */}
      <div className="flex items-center justify-between gap-2 border-t border-border bg-secondary/40 px-2 py-1.5 text-[11px]">
        <span
          className="truncate font-mono text-foreground"
          title={item.filename}
        >
          {item.filename}
        </span>
        <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
          {formatBytes(item.bytes)}
        </span>
      </div>
    </div>
  );
}
