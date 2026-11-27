/**
 * PACE Timeline Calculation Engine
 * Implements frame snapping, local boundary invariance, acoustic pause snapping,
 * Alt-snap inversion, and flagged-boundary diagnostics.
 */

export interface SilenceGap {
  startMs: number;
  endMs: number;
  durationMs: number;
}

export interface TimelineBoundaryItem {
  index: number;
  timeMs: number;
  source?: string;
  confidence?: number | null;
  flags?: string[];
  transition?: Record<string, unknown> | null;
}

export interface FlaggedCutItem {
  boundaryIndex: number;
  sceneIndex: number;
  timeMs: number;
  flags: string[];
  confidence: number | null;
  scenePreview: string;
}

export interface MoveBoundaryParams {
  boundaries: TimelineBoundaryItem[];
  index: number; // Cut line index (1 to boundaries.length - 2)
  targetTimeMs: number;
  totalDurationMs: number;
  fps?: number;
  minDurationMs?: number; // 0.5s minimum scene length invariant
  silences?: SilenceGap[];
  enablePauseSnap?: boolean;
  pauseSnapThresholdMs?: number;
}

export interface MoveBoundaryResult {
  boundaries: TimelineBoundaryItem[];
  snappedToPause: boolean;
  snappedToFrame: boolean;
}

/**
 * Resolves effective acoustic pause snapping when holding the Alt override key.
 */
export function getEffectivePauseSnap(
  baseEnabled: boolean,
  altKeyPressed: boolean
): boolean {
  return altKeyPressed ? !baseEnabled : baseEnabled;
}

/**
 * Snaps any timestamp in milliseconds to the nearest integer frame tick at the given FPS.
 * Frame period at 30 fps is ~33.333ms.
 */
export function snapToFrame(timeMs: number, fps = 30): number {
  const framePeriodMs = 1000 / fps;
  const frameIndex = Math.round(timeMs / framePeriodMs);
  return Math.round(frameIndex * framePeriodMs);
}

/**
 * Snaps timestamp to the midpoint of a speech silence gap if within thresholdMs.
 */
export function snapToPause(
  timeMs: number,
  silences: SilenceGap[],
  thresholdMs = 150
): number {
  for (const s of silences) {
    const mid = Math.round(s.startMs + (s.endMs - s.startMs) / 2);
    if (Math.abs(timeMs - mid) <= thresholdMs) {
      return mid;
    }
  }
  return timeMs;
}

/**
 * Moves boundary `index` to `targetTimeMs` with strict local boundary invariance:
 * - Only boundary `index` is modified (with `source: 'manual'` and cleared flags).
 * - Scene `index - 1` and scene `index` are updated; all other scenes remain 100% identical.
 * - Total duration never changes by 1 millisecond (zero ripple drift).
 * - Enforces minimum scene duration (0.5s) on both sides.
 */
export function moveBoundary({
  boundaries,
  index,
  targetTimeMs,
  totalDurationMs: _totalDurationMs,
  fps = 30,
  minDurationMs = 500,
  silences = [],
  enablePauseSnap = true,
  pauseSnapThresholdMs = 150,
}: MoveBoundaryParams): MoveBoundaryResult {
  if (index <= 0 || index >= boundaries.length - 1) {
    return { boundaries, snappedToPause: false, snappedToFrame: false };
  }

  const prevTime = boundaries[index - 1].timeMs;
  const nextTime = boundaries[index + 1].timeMs;

  const minAllowed = prevTime + minDurationMs;
  const maxAllowed = nextTime - minDurationMs;

  if (minAllowed > maxAllowed) {
    return { boundaries, snappedToPause: false, snappedToFrame: false };
  }

  // 1. Clamp to allowable window
  let candidateTime = Math.max(minAllowed, Math.min(maxAllowed, targetTimeMs));
  let snappedToPause = false;

  // 2. Magnetic pause snapping (if enabled)
  if (enablePauseSnap && silences.length > 0) {
    const pauseSnapped = snapToPause(candidateTime, silences, pauseSnapThresholdMs);
    if (
      pauseSnapped !== candidateTime &&
      pauseSnapped >= minAllowed &&
      pauseSnapped <= maxAllowed
    ) {
      candidateTime = pauseSnapped;
      snappedToPause = true;
    }
  }

  // 3. Frame grid snapping (30 fps)
  const frameSnapped = snapToFrame(candidateTime, fps);

  // 4. Final safety clamp against minDurationMs invariant
  const finalTime = Math.max(minAllowed, Math.min(maxAllowed, frameSnapped));

  const updatedBoundaries = boundaries.map((b) =>
    b.index === index
      ? { ...b, timeMs: finalTime, source: "manual", flags: [] }
      : b
  );

  return {
    boundaries: updatedBoundaries,
    snappedToPause,
    snappedToFrame: true,
  };
}

/**
 * Extracts chronological list of boundaries carrying alignment warning flags.
 */
export function extractFlaggedCuts(
  boundaries: TimelineBoundaryItem[],
  sceneTexts: string[]
): FlaggedCutItem[] {
  const result: FlaggedCutItem[] = [];
  for (const b of boundaries) {
    if (!b.flags || b.flags.length === 0) continue;
    const sceneIdx = Math.max(0, Math.min(sceneTexts.length - 1, b.index));
    result.push({
      boundaryIndex: b.index,
      sceneIndex: sceneIdx,
      timeMs: b.timeMs,
      flags: [...b.flags],
      confidence: b.confidence ?? null,
      scenePreview: sceneTexts[sceneIdx] || `Scene ${sceneIdx + 1}`,
    });
  }
  return result.sort((a, b) => a.boundaryIndex - b.boundaryIndex);
}

/**
 * Calculates duration of every scene between consecutive boundaries.
 */
export function calculateSceneDurations(boundaries: TimelineBoundaryItem[]): number[] {
  const durations: number[] = [];
  for (let i = 0; i < boundaries.length - 1; i++) {
    durations.push(boundaries[i + 1].timeMs - boundaries[i].timeMs);
  }
  return durations;
}

/**
 * Parses timestamp string (mm:ss or mm:ss.mmm) into milliseconds.
 * Returns null if format or values are invalid.
 */
export function parseTimestamp(str: string): number | null {
  const clean = str.trim();
  const parts = clean.split(":");
  if (parts.length < 2 || parts.length > 3) return null;

  let hours = 0;
  let minutes = 0;
  let seconds = 0;

  if (parts.length === 3) {
    hours = Number(parts[0]);
    minutes = Number(parts[1]);
    seconds = Number(parts[2]);
  } else {
    minutes = Number(parts[0]);
    seconds = Number(parts[1]);
  }

  if (Number.isNaN(hours) || Number.isNaN(minutes) || Number.isNaN(seconds)) {
    return null;
  }

  if (hours < 0 || minutes < 0 || minutes >= 60 || seconds < 0 || seconds >= 60) {
    return null;
  }

  return Math.round((hours * 3600 + minutes * 60 + seconds) * 1000);
}

/**
 * Formats milliseconds to exact mm:ss.mmm format.
 */
export function formatTimestampPrecise(timeMs: number): string {
  const safeMs = Math.max(0, Math.round(timeMs));
  const totalSec = Math.floor(safeMs / 1000);
  const millis = safeMs % 1000;
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;

  const mm = String(mins).padStart(2, "0");
  const ss = String(secs).padStart(2, "0");
  const mmm = String(millis).padStart(3, "0");

  return `${mm}:${ss}.${mmm}`;
}
