import type {
  MusicSettingsDto,
  SubtitleCueDto,
  SubtitleStyleDto,
} from "@pace/shared-types";

export const DEFAULT_SUBTITLE_STYLE: SubtitleStyleDto = {
  enabled: true,
  font: "sans",
  size_pct: 100,
  position: "bottom",
  text_color: "#FFFFFF",
  outline_width_px: 2,
  bg_opacity_pct: 75,
};

export const DEFAULT_MUSIC_SETTINGS: MusicSettingsDto = {
  volume_db: -18,
  ducking_enabled: true,
  duck_db: -10,
  loop_enabled: true,
  crossfade_ms: 1500,
  fade_in_ms: 1500,
  fade_out_ms: 2500,
};

export function maxCharsForAspectRatio(aspectRatio: "16:9" | "9:16"): number {
  return aspectRatio === "9:16" ? 32 : 42;
}

export function wrapSubtitlePreviewLines(
  text: string,
  aspectRatio: "16:9" | "9:16"
): string[] {
  const maxChars = maxCharsForAspectRatio(aspectRatio);
  const raw = text.trim();
  if (!raw) return [];

  if (raw.includes("\n")) {
    const explicit = raw
      .split(/\r?\n/)
      .map((l) => l.trim().replace(/\s+/g, " "))
      .filter(Boolean);
    if (
      explicit.length > 0 &&
      explicit.length <= 2 &&
      explicit.every((l) => l.length <= maxChars)
    ) {
      return explicit;
    }
  }

  const words = raw.split(/\s+/).filter(Boolean);
  const joined = words.join(" ");
  if (joined.length <= maxChars || words.length <= 1) {
    return [joined];
  }

  let bestIdx = -1;
  let bestDiff = Number.POSITIVE_INFINITY;
  for (let i = 1; i < words.length; i++) {
    const left = words.slice(0, i).join(" ");
    const right = words.slice(i).join(" ");
    if (left.length <= maxChars && right.length <= maxChars) {
      const diff = Math.abs(left.length - right.length);
      if (diff < bestDiff) {
        bestDiff = diff;
        bestIdx = i;
      }
    }
  }

  if (bestIdx > 0) {
    return [words.slice(0, bestIdx).join(" "), words.slice(bestIdx).join(" ")];
  }

  const lines: string[] = [];
  let cur = "";
  for (const w of words) {
    const cand = cur ? `${cur} ${w}` : w;
    if (cand.length <= maxChars || !cur) {
      cur = cand;
    } else {
      lines.push(cur);
      cur = w;
    }
  }
  if (cur) lines.push(cur);
  return lines;
}

export function evaluateSubtitleCueRules(
  cue: SubtitleCueDto,
  aspectRatio: "16:9" | "9:16"
): {
  lines: string[];
  maxLineChars: number;
  charLimit: number;
  durationMs: number;
  violations: string[];
} {
  const charLimit = maxCharsForAspectRatio(aspectRatio);
  const lines = wrapSubtitlePreviewLines(cue.text, aspectRatio);
  const maxLineChars = lines.reduce((m, l) => Math.max(m, l.length), 0);
  const durationMs = Math.max(0, cue.end_ms - cue.start_ms);
  const violations: string[] = [];

  if (lines.length === 0) {
    violations.push("Empty cue");
  }
  if (lines.length > 2) {
    violations.push(`${lines.length} lines (max 2)`);
  }
  if (maxLineChars > charLimit) {
    violations.push(`${maxLineChars}/${charLimit} chars/line`);
  }
  if (durationMs < 1000) {
    violations.push(`${(durationMs / 1000).toFixed(1)}s (< 1.0s min)`);
  }
  if (durationMs > 7000) {
    violations.push(`${(durationMs / 1000).toFixed(1)}s (> 7.0s max)`);
  }

  return {
    lines,
    maxLineChars,
    charLimit,
    durationMs,
    violations,
  };
}
