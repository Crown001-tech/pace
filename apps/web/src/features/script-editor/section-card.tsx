"use client";

import React, { useRef, useState } from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Textarea } from "@/components/ui/textarea";
import { SectionItem } from "@/stores/script-editor-store";
import { formatDuration } from "@/lib/utils";
import {
  GripVertical,
  Scissors,
  ArrowDownUp,
  Trash2,
  Clock,
  SplitSquareVertical,
} from "lucide-react";

interface SectionCardProps {
  section: SectionItem;
  index: number;
  isLast: boolean;
  onUpdateText: (id: string, text: string) => void;
  onSplit: (id: string, cursorPos: number) => void;
  onSplitParagraphs: (id: string, overrideText?: string) => void;
  onMergeNext: (id: string) => void;
  onDelete: (id: string) => void;
}

export function SectionCard({
  section,
  index,
  isLast,
  onUpdateText,
  onSplit,
  onSplitParagraphs,
  onMergeNext,
  onDelete,
}: SectionCardProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [cursorPos, setCursorPos] = useState(0);

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: section.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    zIndex: isDragging ? 50 : undefined,
  };

  const handleSelectOrClick = () => {
    if (textareaRef.current) {
      setCursorPos(textareaRef.current.selectionStart);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      if (textareaRef.current) {
        onSplit(section.id, textareaRef.current.selectionStart);
      }
    }
  };

  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = e.clipboardData?.getData("text");
    if (pasted && /\r?\n\s*\r?\n/.test(pasted) && !section.text.trim()) {
      e.preventDefault();
      onSplitParagraphs(section.id, pasted);
    }
  };

  const canSplitAtCursor =
    cursorPos > 0 &&
    cursorPos < section.text.length &&
    section.text.slice(0, cursorPos).trim().length > 0 &&
    section.text.slice(cursorPos).trim().length > 0;

  const paragraphCount = section.text
    .replace(/\r\n/g, "\n")
    .split(/\n\s*\n+/)
    .map((p) => p.trim())
    .filter(Boolean).length;

  const sceneLabel = String(index + 1).padStart(2, "0");

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`group relative rounded-2xl border bg-card studio-card transition-all duration-150 ${
        isDragging
          ? "opacity-95 border-brand-lavender shadow-2xl ring-2 ring-brand-lavender/30"
          : "border-border/65 hover:border-brand-lavender/40"
      }`}
    >
      {/* Card Header Strip */}
      <div className="flex items-center justify-between gap-2 border-b border-border/50 bg-secondary/30 px-4 py-2.5 rounded-t-2xl">
        <div className="flex items-center gap-2.5">
          <button
            type="button"
            className="cursor-grab active:cursor-grabbing rounded-[8px] p-1 text-muted-foreground hover:bg-secondary hover:text-foreground touch-none transition-colors"
            aria-label={`Reorder scene ${index + 1}`}
            {...attributes}
            {...listeners}
          >
            <GripVertical className="h-4 w-4" />
          </button>

          <span className="rounded-[8px] bg-brand-violet/10 dark:bg-brand-lavender/15 px-2 py-0.5 font-mono text-[11px] font-semibold text-brand-violet dark:text-brand-lavender">
            Scene {sceneLabel}
          </span>

          <div className="flex items-center gap-2 text-xs font-mono text-muted-foreground tabular-nums">
            <span>{section.wordCount}w</span>
            <span className="opacity-40">·</span>
            <span className="inline-flex items-center gap-1">
              <Clock className="h-3 w-3 opacity-75" />
              ~{formatDuration(section.estimatedDurationSecs)}
            </span>
          </div>
        </div>

        {/* Scene Actions */}
        <div className="flex items-center gap-1">
          {paragraphCount > 1 && (
            <button
              type="button"
              onClick={() => onSplitParagraphs(section.id)}
              title={`Split ${paragraphCount} paragraphs into individual scenes`}
              className="inline-flex items-center gap-1 rounded-[8px] px-2.5 py-1 text-xs font-medium bg-brand-lavender/15 text-brand-violet dark:text-brand-lavender hover:bg-brand-lavender/25 transition-colors"
            >
              <SplitSquareVertical className="h-3.5 w-3.5" />
              <span>Split {paragraphCount}</span>
            </button>
          )}

          <button
            type="button"
            onClick={() => onSplit(section.id, cursorPos)}
            disabled={!canSplitAtCursor}
            title="Place cursor inside text to split scene (Ctrl+Enter)"
            className="inline-flex items-center gap-1 rounded-[8px] px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-35 disabled:pointer-events-none transition-colors"
          >
            <Scissors className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">Split</span>
          </button>

          {!isLast && (
            <button
              type="button"
              onClick={() => onMergeNext(section.id)}
              title="Merge with next scene"
              className="inline-flex items-center gap-1 rounded-[8px] px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
            >
              <ArrowDownUp className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">Merge</span>
            </button>
          )}

          <button
            type="button"
            onClick={() => onDelete(section.id)}
            title="Delete scene"
            aria-label={`Delete scene ${index + 1}`}
            className="inline-flex items-center justify-center rounded-[8px] p-1.5 text-muted-foreground hover:bg-red-500/10 hover:text-red-500 transition-colors"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Text Body */}
      <div className="p-3 sm:p-4">
        <Textarea
          ref={textareaRef}
          value={section.text}
          onChange={(e) => onUpdateText(section.id, e.target.value)}
          onClick={handleSelectOrClick}
          onKeyUp={handleSelectOrClick}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder="Write or paste narration for this scene. Use Ctrl+Enter at cursor to split..."
          rows={Math.max(2, Math.min(8, section.text.split("\n").length + 1))}
          className="border-0 bg-transparent px-1.5 py-1 shadow-none focus-visible:ring-0 text-sm sm:text-[15px] leading-relaxed resize-none"
        />
      </div>
    </div>
  );
}
