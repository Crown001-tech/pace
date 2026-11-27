"use client";

import React from "react";
import { Plus, Sparkles, Trash2 } from "lucide-react";
import type {
  StickerAnimation,
  StickerKind,
  StickerPlacementSpec,
} from "@pace/shared-types";

const STICKER_CATALOG: { kind: StickerKind; label: string }[] = [
  { kind: "subscribe", label: "Subscribe" },
  { kind: "like", label: "Like" },
  { kind: "comment", label: "Comment" },
  { kind: "bell", label: "Bell" },
  { kind: "follow", label: "Follow" },
];

const ANIMATION_OPTIONS: { id: StickerAnimation; label: string }[] = [
  { id: "pop_in", label: "Pop In" },
  { id: "bounce", label: "Bounce" },
  { id: "float", label: "Float" },
  { id: "none", label: "Static" },
];

const POSITION_PRESETS: { label: string; x: number; y: number }[] = [
  { label: "Bottom", x: 0.5, y: 0.84 },
  { label: "Top", x: 0.5, y: 0.16 },
  { label: "Top-R", x: 0.78, y: 0.16 },
  { label: "Bot-R", x: 0.78, y: 0.84 },
];

interface StickersPanelProps {
  stickers: StickerPlacementSpec[];
  currentTimeMs: number;
  totalDurationMs: number;
  isSaving?: boolean;
  onChangeStickers: (next: StickerPlacementSpec[]) => void;
  onSeekToMs?: (ms: number) => void;
}

function formatSec(ms: number): string {
  return (Math.max(0, ms) / 1000).toFixed(1);
}

export function StickersPanel({
  stickers,
  currentTimeMs,
  totalDurationMs,
  isSaving = false,
  onChangeStickers,
  onSeekToMs,
}: StickersPanelProps) {
  const safeDuration = Math.max(500, totalDurationMs);

  const handleAddSticker = (kind: StickerKind) => {
    const startMs = Math.max(
      0,
      Math.min(safeDuration - 500, Math.round(currentTimeMs))
    );
    const endMs = Math.min(safeDuration, startMs + 3500);
    const next: StickerPlacementSpec = {
      id: `stk-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      kind,
      start_ms: startMs,
      end_ms: Math.max(startMs + 500, endMs),
      x: 0.5,
      y: 0.84,
      scale: 1.0,
      animation: "pop_in",
    };
    onChangeStickers([...stickers, next]);
  };

  const updateOne = (id: string, patch: Partial<StickerPlacementSpec>) => {
    onChangeStickers(
      stickers.map((s) => (s.id === id ? { ...s, ...patch } : s))
    );
  };

  const removeOne = (id: string) => {
    onChangeStickers(stickers.filter((s) => s.id !== id));
  };

  return (
    <div className="space-y-3.5 text-xs">
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1 text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            <Sparkles className="h-3 w-3 text-brand-violet dark:text-brand-lavender" />
            Add Overlay Sticker at {formatSec(currentTimeMs)}s
          </span>
          {isSaving && (
            <span className="font-mono text-[10px] text-muted-foreground">
              Saving...
            </span>
          )}
        </div>

        <div className="grid grid-cols-2 gap-1.5">
          {STICKER_CATALOG.map((item) => (
            <button
              key={item.kind}
              type="button"
              onClick={() => handleAddSticker(item.kind)}
              className="flex items-center gap-2 rounded-md border border-border bg-card px-2 py-1.5 text-left hover:bg-secondary/60 transition-colors touch-manipulation"
            >
              <img
                src={`/stickers/${item.kind}.png`}
                alt={item.label}
                className="h-5 w-auto object-contain shrink-0"
              />
              <span className="text-[11px] font-medium text-foreground flex-1 truncate">
                {item.label}
              </span>
              <Plus className="h-3 w-3 text-muted-foreground shrink-0" />
            </button>
          ))}
        </div>
      </div>

      {stickers.length === 0 ? (
        <div className="rounded-md border border-dashed border-border bg-secondary/20 px-3 py-4 text-center text-[11px] text-muted-foreground">
          No stickers placed yet. Tap any sticker above to add it at the
          playhead.
        </div>
      ) : (
        <div className="space-y-2.5">
          <div className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
            Timeline Stickers ({stickers.length})
          </div>
          {stickers.map((st, idx) => (
            <div
              key={st.id || idx}
              className="rounded-md border border-border bg-secondary/25 p-2.5 space-y-2"
            >
              <div className="flex items-center justify-between gap-2">
                <button
                  type="button"
                  onClick={() => onSeekToMs?.(st.start_ms)}
                  className="flex items-center gap-2 text-left hover:opacity-80"
                  title="Jump playhead to sticker start"
                >
                  <img
                    src={`/stickers/${st.kind}.png`}
                    alt={st.kind}
                    className="h-5 w-auto object-contain"
                  />
                  <span className="font-mono text-[10px] text-muted-foreground">
                    {formatSec(st.start_ms)}s – {formatSec(st.end_ms)}s
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => removeOne(st.id)}
                  className="rounded p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                  aria-label="Remove sticker"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <label className="space-y-0.5">
                  <span className="text-[10px] font-mono text-muted-foreground">
                    Start (s)
                  </span>
                  <input
                    type="number"
                    step="0.1"
                    min={0}
                    max={Number(formatSec(safeDuration - 500))}
                    value={Number(formatSec(st.start_ms))}
                    onChange={(e) => {
                      const nextStart = Math.max(
                        0,
                        Math.min(
                          safeDuration - 500,
                          Math.round(Number(e.target.value || 0) * 1000)
                        )
                      );
                      updateOne(st.id, {
                        start_ms: nextStart,
                        end_ms: Math.max(nextStart + 500, st.end_ms),
                      });
                    }}
                    className="w-full rounded border border-input bg-background px-2 py-1 font-mono text-[11px]"
                  />
                </label>
                <label className="space-y-0.5">
                  <span className="text-[10px] font-mono text-muted-foreground">
                    End (s)
                  </span>
                  <input
                    type="number"
                    step="0.1"
                    min={Number(formatSec(st.start_ms + 500))}
                    max={Number(formatSec(safeDuration))}
                    value={Number(formatSec(st.end_ms))}
                    onChange={(e) => {
                      const nextEnd = Math.max(
                        st.start_ms + 500,
                        Math.min(
                          safeDuration,
                          Math.round(Number(e.target.value || 0) * 1000)
                        )
                      );
                      updateOne(st.id, { end_ms: nextEnd });
                    }}
                    className="w-full rounded border border-input bg-background px-2 py-1 font-mono text-[11px]"
                  />
                </label>
              </div>

              <div className="grid grid-cols-4 gap-1">
                {ANIMATION_OPTIONS.map((anim) => (
                  <button
                    key={anim.id}
                    type="button"
                    onClick={() => updateOne(st.id, { animation: anim.id })}
                    className={`min-h-[26px] rounded border px-1 py-0.5 text-[10px] font-medium transition-colors ${
                      st.animation === anim.id
                        ? "border-brand-violet bg-brand-violet/15 text-brand-violet dark:border-brand-lavender dark:bg-brand-lavender/20 dark:text-brand-lavender font-semibold"
                        : "border-border bg-card text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {anim.label}
                  </button>
                ))}
              </div>

              <div className="grid grid-cols-4 gap-1">
                {POSITION_PRESETS.map((pos) => {
                  const active =
                    Math.abs(st.x - pos.x) < 0.05 &&
                    Math.abs(st.y - pos.y) < 0.05;
                  return (
                    <button
                      key={pos.label}
                      type="button"
                      onClick={() => updateOne(st.id, { x: pos.x, y: pos.y })}
                      className={`min-h-[24px] rounded border px-1 py-0.5 text-[10px] transition-colors ${
                        active
                          ? "border-brand-lavender bg-brand-lavender/15 text-foreground font-semibold"
                          : "border-border bg-card text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {pos.label}
                    </button>
                  );
                })}
              </div>

              <div className="space-y-1">
                <div className="flex items-center justify-between font-mono text-[10px] text-muted-foreground">
                  <span>Scale</span>
                  <span>{Math.round(st.scale * 100)}%</span>
                </div>
                <input
                  type="range"
                  min={0.4}
                  max={2.0}
                  step={0.05}
                  value={st.scale}
                  onChange={(e) =>
                    updateOne(st.id, { scale: Number(e.target.value) })
                  }
                  className="w-full accent-brand-lavender cursor-pointer"
                />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
