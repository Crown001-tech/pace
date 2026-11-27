import type { ExportResolution } from "@pace/shared-types";

export interface ResolutionOptionMeta {
  id: ExportResolution;
  label: string;
  tag: string;
  width: number;
  height: number;
  avgBitrateMbps: number;
  renderSpeedFactor: number;
}

const BASE_RESOLUTIONS: Array<{
  id: ExportResolution;
  label: string;
  tag: string;
  w16: number;
  h16: number;
  avgBitrateMbps: number;
  renderSpeedFactor: number;
}> = [
  {
    id: "360p",
    label: "360p",
    tag: "Draft",
    w16: 640,
    h16: 360,
    avgBitrateMbps: 0.9,
    renderSpeedFactor: 0.2,
  },
  {
    id: "480p",
    label: "480p",
    tag: "Compact",
    w16: 854,
    h16: 480,
    avgBitrateMbps: 1.5,
    renderSpeedFactor: 0.35,
  },
  {
    id: "720p",
    label: "720p",
    tag: "Standard HD",
    w16: 1280,
    h16: 720,
    avgBitrateMbps: 2.6,
    renderSpeedFactor: 0.65,
  },
  {
    id: "1080p",
    label: "1080p",
    tag: "Full HD Max",
    w16: 1920,
    h16: 1080,
    avgBitrateMbps: 4.5,
    renderSpeedFactor: 1.25,
  },
];

export function getResolutionOptions(
  aspectRatio: "16:9" | "9:16"
): ResolutionOptionMeta[] {
  return BASE_RESOLUTIONS.map((item) => ({
    id: item.id,
    label: item.label,
    tag: item.tag,
    width: aspectRatio === "9:16" ? item.h16 : item.w16,
    height: aspectRatio === "9:16" ? item.w16 : item.h16,
    avgBitrateMbps: item.avgBitrateMbps,
    renderSpeedFactor: item.renderSpeedFactor,
  }));
}

export function estimateExportStats(
  resolution: ExportResolution,
  durationMs: number
): { estimatedBytes: number; estimatedRenderSeconds: number } {
  const safeMs = Math.max(1000, durationMs);
  const secs = safeMs / 1000;
  const meta =
    BASE_RESOLUTIONS.find((r) => r.id === resolution) ?? BASE_RESOLUTIONS[2];

  // Video + 192 kbps AAC audio
  const totalMegabits = secs * (meta.avgBitrateMbps + 0.192);
  const estimatedBytes = Math.round((totalMegabits * 1_000_000) / 8);
  const estimatedRenderSeconds = Math.max(
    4,
    Math.round(secs * meta.renderSpeedFactor + 3)
  );

  return { estimatedBytes, estimatedRenderSeconds };
}

export function formatDurationShort(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds));
  if (s < 60) return `${s}s`;
  const mins = Math.floor(s / 60);
  const rem = s % 60;
  return rem === 0 ? `${mins}m` : `${mins}m ${rem}s`;
}

export function formatRenderStage(stage?: string | null): string {
  if (!stage) return "Queued for worker";
  if (stage === "planning_segments") return "Planning scene segments";
  if (stage === "muxing_audio") return "Concatenating & muxing AAC audio";
  if (stage === "validating_export") return "Probing streams & frame timing";
  if (stage === "uploading_mp4") return "Uploading MP4 to storage";
  if (stage === "completed") return "Export ready";
  if (stage === "failed") return "Export failed";
  const segMatch = /^rendering_segment_(\d+)_of_(\d+)$/.exec(stage);
  if (segMatch) {
    return `Rendering segment ${segMatch[1]} of ${segMatch[2]}`;
  }
  return stage.replace(/_/g, " ");
}
