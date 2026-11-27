"use client";

import React, { useRef, useEffect } from "react";
import { BoundaryOutput, SilenceGap } from "./alignment-engine";
import { formatDuration } from "@/lib/utils";
import { useTheme } from "@/hooks/use-theme";

interface WaveformViewerProps {
  durationMs: number;
  currentPlayTimeMs: number;
  boundaries: BoundaryOutput[];
  silences: SilenceGap[];
  peaks?: [number, number][];
  onSeek?: (timeMs: number) => void;
}

export function WaveformViewer({
  durationMs,
  currentPlayTimeMs,
  boundaries,
  silences,
  peaks,
  onSeek,
}: WaveformViewerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { theme } = useTheme();

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;
    ctx.clearRect(0, 0, width, height);

    const isDark = theme === "dark";

    // 1. Background
    ctx.fillStyle = isDark ? "#0A0A0E" : "#EEF0F5";
    ctx.fillRect(0, 0, width, height);

    if (durationMs <= 0) return;

    // 2. Silence pause regions
    ctx.fillStyle = isDark
      ? "rgba(56, 189, 248, 0.14)"
      : "rgba(14, 116, 144, 0.14)";
    for (const silence of silences) {
      const x1 = (silence.startMs / durationMs) * width;
      const x2 = (silence.endMs / durationMs) * width;
      ctx.fillRect(x1, 0, Math.max(2, x2 - x1), height);
    }

    // 3. Waveform bars
    const barWidth = 3;
    const barGap = 1;
    const totalBars = Math.floor(width / (barWidth + barGap));
    ctx.fillStyle = isDark ? "#71717A" : "#475569";

    for (let i = 0; i < totalBars; i++) {
      const x = i * (barWidth + barGap);
      let amp = 0.1;

      if (peaks && peaks.length > 0) {
        const peakIdx = Math.min(
          peaks.length - 1,
          Math.floor((i / Math.max(1, totalBars)) * peaks.length)
        );
        const [minVal, maxVal] = peaks[peakIdx] || [0, 0];
        amp = Math.max(0.04, Math.min(1.0, Math.max(Math.abs(minVal), Math.abs(maxVal))));
      } else {
        const t = (x / width) * durationMs;
        const inSilence = silences.some((s) => t >= s.startMs && t <= s.endMs);
        amp = inSilence
          ? 0.05
          : 0.25 + 0.65 * Math.abs(Math.sin(i * 0.28) * Math.cos(i * 0.14));
      }

      const barHeight = Math.max(4, amp * (height * 0.78));
      const y = (height - barHeight) / 2;
      ctx.fillRect(x, y, barWidth, barHeight);
    }

    // 4. Boundary cut markers
    for (const b of boundaries) {
      if (b.timeMs <= 0 || b.timeMs >= durationMs) continue;
      const bx = (b.timeMs / durationMs) * width;

      ctx.strokeStyle = b.flags.length > 0 ? "#D97706" : isDark ? "#A96AF9" : "#2E016A";
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 2]);
      ctx.beginPath();
      ctx.moveTo(bx, 0);
      ctx.lineTo(bx, height);
      ctx.stroke();
      ctx.setLineDash([]);

      const isCrowded = boundaries.length > 15;
      const isFlagged = b.flags.length > 0;
      const isEvery5th = (b.index + 1) % 5 === 0;

      if (!isCrowded || isFlagged || isEvery5th) {
        ctx.fillStyle = isFlagged ? "#B45309" : "#2E016A";
        ctx.fillRect(bx - 12, 2, 24, 14);
        ctx.fillStyle = "#FFFFFF";
        ctx.font = "10px monospace";
        ctx.textAlign = "center";
        ctx.fillText(`#${b.index + 1}`, bx, 12);
      } else {
        ctx.fillStyle = isDark ? "#A96AF9" : "#2E016A";
        ctx.fillRect(bx - 1, 0, 2, 8);
      }
    }

    // 5. Playback playhead
    const playheadX = (currentPlayTimeMs / durationMs) * width;
    ctx.strokeStyle = isDark ? "#FFFFFF" : "#111827";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(playheadX, 0);
    ctx.lineTo(playheadX, height);
    ctx.stroke();
  }, [durationMs, currentPlayTimeMs, boundaries, silences, peaks, theme]);

  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!onSeek || durationMs <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const ratio = Math.max(0, Math.min(1, x / rect.width));
    onSeek(Math.round(ratio * durationMs));
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground font-mono px-0.5">
        <span>{formatDuration(currentPlayTimeMs / 1000)}</span>
        <div className="flex items-center gap-4 text-[11px]">
          <span className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-sm bg-brand-violet dark:bg-brand-lavender inline-block" />
            Scene Cut
          </span>
          <span className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-sm bg-brand-lavender/30 inline-block" />
            Natural Pause
          </span>
          <span className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-sm bg-amber-500 inline-block" />
            Flagged
          </span>
        </div>
        <span>{formatDuration(durationMs / 1000)}</span>
      </div>

      <div className="relative border border-border rounded-lg overflow-hidden bg-secondary cursor-pointer">
        <canvas
          ref={canvasRef}
          width={900}
          height={96}
          onClick={handleClick}
          className="w-full h-24 block"
        />
      </div>
    </div>
  );
}
