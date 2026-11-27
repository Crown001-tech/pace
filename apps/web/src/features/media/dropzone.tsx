"use client";

import React, { useState, useRef } from "react";
import { UploadCloud, AlertCircle, Smartphone } from "lucide-react";
import { useMediaStore } from "./media-store";

interface DropzoneProps {
  onFilesSelected: (files: FileList | File[]) => void;
  disabled?: boolean;
}

export function Dropzone({ onFilesSelected, disabled }: DropzoneProps) {
  const [isDragOver, setIsDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { uploadQueue, duplicateWarnings, clearDuplicateWarnings } = useMediaStore();

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (!disabled) setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    if (disabled || !e.dataTransfer.files) return;
    onFilesSelected(e.dataTransfer.files);
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files.length > 0) {
      onFilesSelected(e.target.files);
      e.target.value = "";
    }
  };

  return (
    <div className="space-y-3">
      {duplicateWarnings.length > 0 && (
        <div className="flex items-start justify-between rounded-md border border-amber-500/35 bg-amber-500/10 p-3 text-xs text-amber-800 dark:text-amber-200">
          <div className="flex items-start gap-2">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
            <div>
              <p className="font-semibold">Duplicate files skipped</p>
              <p className="mt-0.5 text-[11px] opacity-85">
                Identical SHA-256 checksums already exist in this project:
              </p>
              <ul className="mt-1 list-disc list-inside font-mono text-[11px]">
                {duplicateWarnings.slice(0, 4).map((name) => (
                  <li key={name}>{name}</li>
                ))}
                {duplicateWarnings.length > 4 && (
                  <li>...and {duplicateWarnings.length - 4} more</li>
                )}
              </ul>
            </div>
          </div>
          <button
            type="button"
            onClick={clearDuplicateWarnings}
            className="ml-3 shrink-0 font-medium underline hover:opacity-80"
          >
            Dismiss
          </button>
        </div>
      )}

      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        onClick={() => !disabled && fileInputRef.current?.click()}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if ((e.key === "Enter" || e.key === " ") && !disabled) {
            e.preventDefault();
            fileInputRef.current?.click();
          }
        }}
        className={`relative flex flex-col items-center justify-center rounded-lg border border-dashed p-5 sm:p-6 text-center cursor-pointer transition-colors touch-manipulation ${
          isDragOver
            ? "border-brand-lavender bg-brand-lavender/10"
            : "border-border bg-secondary/40 hover:border-zinc-400 dark:hover:border-zinc-600"
        } ${disabled ? "opacity-50 cursor-not-allowed" : ""}`}
      >
        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept="image/jpeg,image/png,image/webp,image/*"
          className="hidden"
          onChange={handleFileChange}
          disabled={disabled}
        />
        <div className="mb-2 flex h-9 w-9 items-center justify-center rounded-md border border-border bg-card text-muted-foreground">
          <UploadCloud className="h-4 w-4" />
        </div>
        <p className="text-xs sm:text-sm font-medium text-foreground">
          Drop images here or{" "}
          <span className="text-brand-violet dark:text-brand-lavender font-semibold">
            browse
          </span>
        </p>
        <p className="mt-1 text-[11px] font-mono text-muted-foreground">
          JPG · PNG · WebP · up to 20 MB each
        </p>

        <button
          type="button"
          disabled={disabled}
          onClick={(e) => {
            e.stopPropagation();
            if (!disabled) fileInputRef.current?.click();
          }}
          className="mt-3 inline-flex min-h-[40px] sm:hidden items-center gap-1.5 rounded-lg border border-brand-lavender/40 bg-brand-lavender/15 px-3.5 py-2 text-xs font-semibold text-brand-violet dark:text-brand-lavender touch-manipulation"
        >
          <Smartphone className="h-3.5 w-3.5" />
          Select Photos from Phone Gallery
        </button>
      </div>

      {uploadQueue.length > 0 && (
        <div className="max-h-36 overflow-y-auto rounded-md border border-border bg-card p-2 space-y-1.5">
          <p className="px-1 text-[10px] font-mono font-semibold uppercase tracking-wider text-muted-foreground">
            Queue ({uploadQueue.length})
          </p>
          {uploadQueue.map((item) => (
            <div
              key={item.filename}
              className="flex items-center justify-between gap-2 rounded bg-secondary/70 px-2.5 py-1.5 text-xs"
            >
              <span className="truncate max-w-[180px] font-mono text-[11px] text-foreground">
                {item.filename}
              </span>
              <span
                className={`font-mono text-[11px] truncate ${
                  item.status === "error"
                    ? "text-red-500"
                    : item.status === "done"
                    ? "text-emerald-500"
                    : "text-muted-foreground"
                }`}
                title={item.errorMessage}
              >
                {item.status === "hashing"
                  ? "Hashing"
                  : item.status === "uploading"
                  ? `${item.progress}%`
                  : item.status === "done"
                  ? "Ready"
                  : item.errorMessage || "Failed"}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
