import React from "react";
import { Play, AlertTriangle, Clock } from "lucide-react";
import type { BoundaryOutput } from "./alignment-engine.ts";
import { formatDuration } from "@/lib/utils";
import { Button } from "@/components/ui/button";

interface BoundaryTableProps {
  boundaries: BoundaryOutput[];
  narrationDurationMs: number;
  onJumpToTime: (timeMs: number) => void;
  onNudgeBoundary?: (index: number, deltaMs: number) => void;
}

export function BoundaryTable({
  boundaries,
  narrationDurationMs,
  onJumpToTime,
  onNudgeBoundary,
}: BoundaryTableProps) {
  const getFlagTooltip = (flag: string) => {
    switch (flag) {
      case "Section not found":
        return "Speech-to-text did not find spoken words matching this section (match < 20%). Check script wording.";
      case "Unexpected long gap":
        return "A pause longer than 4.0s was detected between spoken scenes.";
      case "Very short scene":
        return "This scene duration is under 1.0 second. Recommended minimum is 1.0s.";
      case "Low confidence":
        return "Spoken word alignment confidence is low; review cut timing manually.";
      default:
        return "Alignment attention required.";
    }
  };

  return (
    <div className="border border-border bg-card rounded-lg overflow-hidden">
      <div className="px-4 py-3 border-b border-border flex flex-col sm:flex-row sm:items-center justify-between gap-2 bg-secondary/50">
        <div>
          <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
            <Clock className="w-4 h-4 text-brand-violet dark:text-brand-lavender" />
            <span>Scene Cut Timings ({Math.max(0, boundaries.length - 1)} Scenes)</span>
          </h3>
          <p className="text-xs text-muted-foreground mt-0.5">
            Aligned transition points between narration scenes. Use trim buttons to adjust cuts.
          </p>
        </div>
        <span className="text-xs text-muted-foreground font-mono shrink-0">
          Total: {formatDuration(narrationDurationMs / 1000)}
        </span>
      </div>

      <div className="max-h-80 overflow-y-auto divide-y divide-border">
        {boundaries.slice(0, boundaries.length - 1).map((b, idx) => {
          const nextB = boundaries[idx + 1];
          const durationMs = nextB ? nextB.timeMs - b.timeMs : 0;
          const hasFlags = b.flags.length > 0;

          return (
            <div
              key={b.index}
              className={`flex flex-col sm:flex-row sm:items-center justify-between px-4 py-2.5 text-xs transition-colors hover:bg-secondary/40 gap-2 ${
                hasFlags ? "bg-amber-500/[0.06]" : ""
              }`}
            >
              <div className="flex items-center gap-3">
                <span className="font-mono font-semibold text-foreground w-16">
                  Scene #{b.index + 1}
                </span>
                <span className="font-mono text-foreground/90">
                  {formatDuration(b.timeMs / 1000)}
                </span>
                <span className="text-[11px] text-muted-foreground font-mono">
                  ({(durationMs / 1000).toFixed(1)}s)
                </span>
              </div>

              <div className="flex items-center gap-2 flex-wrap">
                {b.flags.map((flag) => (
                  <span
                    key={flag}
                    title={getFlagTooltip(flag)}
                    className="flex items-center gap-1 px-2 py-0.5 text-[11px] font-medium bg-amber-500/15 text-amber-800 dark:text-amber-300 border border-amber-500/35 rounded cursor-help"
                  >
                    <AlertTriangle className="w-3 h-3 text-amber-600 dark:text-amber-400" />
                    <span>{flag}</span>
                  </span>
                ))}

                {b.source === "manual" && (
                  <span className="px-2 py-0.5 text-[11px] font-medium rounded border bg-brand-violet/10 text-brand-violet border-brand-violet/30 dark:bg-brand-lavender/15 dark:text-brand-lavender dark:border-brand-lavender/40">
                    Manual
                  </span>
                )}

                <span
                  title={
                    b.confidenceLevel === "High"
                      ? "High Confidence: Transcript words matched script cleanly at pause boundaries."
                      : "Review boundary timing manually."
                  }
                  className={`px-2 py-0.5 text-[11px] font-medium rounded border cursor-help ${
                    b.confidenceLevel === "High"
                      ? "bg-emerald-500/10 text-emerald-800 border-emerald-500/30 dark:text-emerald-300"
                      : b.confidenceLevel === "Medium"
                      ? "bg-secondary text-secondary-foreground border-border"
                      : "bg-red-500/10 text-red-700 border-red-500/30 dark:text-red-300"
                  }`}
                >
                  {b.confidenceLevel}
                </span>

                {onNudgeBoundary && (
                  <div className="flex items-center gap-1 font-mono text-[11px]">
                    <button
                      type="button"
                      onClick={() => onNudgeBoundary(b.index, -100)}
                      title="Nudge cut 100ms earlier (-0.1s)"
                      className="px-2 py-0.5 bg-secondary hover:bg-secondary/80 text-foreground border border-border rounded transition-colors"
                    >
                      -0.1s
                    </button>
                    <button
                      type="button"
                      onClick={() => onNudgeBoundary(b.index, 100)}
                      title="Nudge cut 100ms later (+0.1s)"
                      className="px-2 py-0.5 bg-secondary hover:bg-secondary/80 text-foreground border border-border rounded transition-colors"
                    >
                      +0.1s
                    </button>
                  </div>
                )}

                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onJumpToTime(b.timeMs)}
                  className="h-7 px-2"
                  title="Jump playback to this scene cut"
                >
                  <Play className="w-3 h-3 mr-1" />
                  <span>Jump</span>
                </Button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
