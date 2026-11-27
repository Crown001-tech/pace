import { create } from 'zustand';
import type { SceneEffectDto } from '@pace/shared-types';

export type AspectRatio = '16:9' | '9:16';
export type FitMode = 'fill' | 'fit_blur';

export interface MediaItem {
  id: string;
  filename: string;
  bytes: number;
  sha256: string;
  mime: string;
  aspectRatio: AspectRatio;
  width?: number;
  height?: number;
  storageKey: string;
  thumbUrl: string;
  previewUrl?: string;
  blurPreviewUrl?: string;
  isDuplicate?: boolean;
}

export interface SectionAssignment {
  sectionId: string;
  mediaId: string | null;
  fitMode: FitMode;
  focalX: number;
  focalY: number;
  effect?: SceneEffectDto;
  effectOverridden?: boolean;
  resolvedPreset?: string;
  resolvedHasShake?: boolean;
}

export interface SectionRef {
  id: string;
  position: number;
  text: string;
}

export interface MismatchSummary {
  text: string;
  hasEmptySections: boolean;
  emptyCount: number;
  excessCount: number;
}

export interface ExportReadiness {
  canExport: boolean;
  emptySectionIds: string[];
}

/**
 * Natural alphanumeric sort (e.g., scene_1, scene_2, scene_10).
 * Handles mixed case, numbers in string, and leading zeros (MED-03).
 */
export function naturalSortFiles(filenames: string[]): string[] {
  const collator = new Intl.Collator(undefined, {
    numeric: true,
    sensitivity: 'base',
  });
  return [...filenames].sort(collator.compare);
}

/**
 * Check if a file's SHA-256 already exists in the project (MED-04).
 */
export function checkDuplicateSha256(sha256: string, existingMedia: MediaItem[]): boolean {
  return existingMedia.some((item) => item.sha256 === sha256);
}

/**
 * Assign media items to sections in natural order (ASN-01).
 * Image k goes to section k; extra images remain unassigned in library;
 * sections without an image are marked with mediaId: null (Empty).
 */
export function assignMediaInOrder(
  sections: SectionRef[],
  media: MediaItem[]
): SectionAssignment[] {
  const sortedSections = [...sections].sort((a, b) => a.position - b.position);

  return sortedSections.map((sec, idx) => ({
    sectionId: sec.id,
    mediaId: idx < media.length ? media[idx].id : null,
    fitMode: 'fill',
    focalX: 0.5,
    focalY: 0.5,
  }));
}

/**
 * Calculate the mismatch summary banner text (ASN-02).
 * Formats:
 * - "64 sections, 61 images, 3 sections have no image"
 * - "5 sections, 8 images, 3 extra images in library"
 * - "10 sections, 10 images"
 */
export function calculateMismatchSummary(
  sectionCount: number,
  imageCount: number
): MismatchSummary {
  const emptyCount = Math.max(0, sectionCount - imageCount);
  const excessCount = Math.max(0, imageCount - sectionCount);
  const hasEmptySections = emptyCount > 0;

  let text = `${sectionCount} sections, ${imageCount} images`;
  if (emptyCount > 0) {
    const noun = emptyCount === 1 ? 'section has' : 'sections have';
    text += `, ${emptyCount} ${noun} no image`;
  } else if (excessCount > 0) {
    const noun = excessCount === 1 ? 'extra image' : 'extra images';
    text += `, ${excessCount} ${noun} in library`;
  }

  return {
    text,
    hasEmptySections,
    emptyCount,
    excessCount,
  };
}

/**
 * Swap assigned media between two sections (ASN-03).
 */
export function swapSectionAssignments(
  assignments: SectionAssignment[],
  sectionIdA: string,
  sectionIdB: string
): SectionAssignment[] {
  const idxA = assignments.findIndex((a) => a.sectionId === sectionIdA);
  const idxB = assignments.findIndex((a) => a.sectionId === sectionIdB);

  if (idxA === -1 || idxB === -1 || idxA === idxB) {
    return assignments;
  }

  const next = [...assignments];
  const itemA = next[idxA];
  const itemB = next[idxB];

  next[idxA] = {
    ...itemA,
    mediaId: itemB.mediaId,
  };
  next[idxB] = {
    ...itemB,
    mediaId: itemA.mediaId,
  };

  return next;
}

/**
 * Check if the project is ready for export (ASN-05).
 * Export is blocked while any section is Empty.
 */
export function validateExportReadiness(
  assignments: SectionAssignment[]
): ExportReadiness {
  const emptySectionIds = assignments
    .filter((a) => a.mediaId === null)
    .map((a) => a.sectionId);

  return {
    canExport: emptySectionIds.length === 0,
    emptySectionIds,
  };
}

export interface MediaUploadProgress {
  filename: string;
  progress: number;
  status: 'hashing' | 'uploading' | 'done' | 'error';
  errorMessage?: string;
  sha256?: string;
}

interface MediaState {
  media: MediaItem[];
  assignments: SectionAssignment[];
  uploadQueue: MediaUploadProgress[];
  duplicateWarnings: string[];
  sortMode: 'natural' | 'upload';

  // Actions
  setMedia: (media: MediaItem[]) => void;
  addMediaItem: (item: MediaItem) => void;
  removeMediaItem: (mediaId: string) => void;
  setSortMode: (mode: 'natural' | 'upload') => void;

  setAssignments: (assignments: SectionAssignment[]) => void;
  assignToSection: (sectionId: string, mediaId: string) => void;
  unassignSection: (sectionId: string) => void;
  autoAssign: (sections: SectionRef[]) => void;
  swapSections: (sectionIdA: string, sectionIdB: string) => void;
  updateFocalPoint: (sectionId: string, focalX: number, focalY: number) => void;
  updateFitMode: (sectionId: string, fitMode: FitMode) => void;
  updateSceneEffect: (
    sectionId: string,
    patch: Partial<Pick<SectionAssignment, 'fitMode' | 'focalX' | 'focalY' | 'effect' | 'effectOverridden' | 'resolvedPreset' | 'resolvedHasShake'>>
  ) => void;

  setUploadProgress: (filename: string, progress: Partial<MediaUploadProgress>) => void;
  clearUploadProgress: (filename: string) => void;
  addDuplicateWarning: (filename: string) => void;
  clearDuplicateWarnings: () => void;
}

export const useMediaStore = create<MediaState>((set) => ({
  media: [],
  assignments: [],
  uploadQueue: [],
  duplicateWarnings: [],
  sortMode: 'natural',

  setMedia: (media) => set({ media }),

  addMediaItem: (item) =>
    set((state) => {
      // Natural sort by default
      const list = [...state.media, item];
      return { media: list };
    }),

  removeMediaItem: (mediaId) =>
    set((state) => ({
      media: state.media.filter((m) => m.id !== mediaId),
      // If removed item was assigned, that section becomes Empty (MED-06)
      assignments: state.assignments.map((a) =>
        a.mediaId === mediaId ? { ...a, mediaId: null } : a
      ),
    })),

  setSortMode: (mode) => set({ sortMode: mode }),

  setAssignments: (assignments) => set({ assignments }),

  assignToSection: (sectionId, mediaId) =>
    set((state) => ({
      assignments: state.assignments.map((a) =>
        a.sectionId === sectionId
          ? { ...a, mediaId, focalX: 0.5, focalY: 0.5 }
          : a
      ),
    })),

  unassignSection: (sectionId) =>
    set((state) => ({
      assignments: state.assignments.map((a) =>
        a.sectionId === sectionId ? { ...a, mediaId: null } : a
      ),
    })),

  autoAssign: (sections) =>
    set((state) => ({
      assignments: assignMediaInOrder(sections, state.media),
    })),

  swapSections: (sectionIdA, sectionIdB) =>
    set((state) => ({
      assignments: swapSectionAssignments(state.assignments, sectionIdA, sectionIdB),
    })),

  updateFocalPoint: (sectionId, focalX, focalY) =>
    set((state) => ({
      assignments: state.assignments.map((a) =>
        a.sectionId === sectionId ? { ...a, focalX, focalY } : a
      ),
    })),

  updateFitMode: (sectionId, fitMode) =>
    set((state) => ({
      assignments: state.assignments.map((a) =>
        a.sectionId === sectionId ? { ...a, fitMode } : a
      ),
    })),

  updateSceneEffect: (sectionId, patch) =>
    set((state) => {
      const exists = state.assignments.some((a) => a.sectionId === sectionId);
      if (!exists) {
        return {
          assignments: [
            ...state.assignments,
            {
              sectionId,
              mediaId: null,
              fitMode: 'fill',
              focalX: 0.5,
              focalY: 0.5,
              ...patch,
            },
          ],
        };
      }
      return {
        assignments: state.assignments.map((a) =>
          a.sectionId === sectionId ? { ...a, ...patch } : a
        ),
      };
    }),

  setUploadProgress: (filename, progress) =>
    set((state) => {
      const idx = state.uploadQueue.findIndex((p) => p.filename === filename);
      if (idx === -1) {
        return {
          uploadQueue: [
            ...state.uploadQueue,
            {
              filename,
              progress: 0,
              status: 'uploading',
              ...progress,
            },
          ],
        };
      }
      const updated = [...state.uploadQueue];
      updated[idx] = { ...updated[idx], ...progress };
      return { uploadQueue: updated };
    }),

  clearUploadProgress: (filename) =>
    set((state) => ({
      uploadQueue: state.uploadQueue.filter((p) => p.filename !== filename),
    })),

  addDuplicateWarning: (filename) =>
    set((state) => ({
      duplicateWarnings: Array.from(new Set([...state.duplicateWarnings, filename])),
    })),

  clearDuplicateWarnings: () => set({ duplicateWarnings: [] }),
}));

