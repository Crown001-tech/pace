import test from 'node:test';
import assert from 'node:assert/strict';
import {
  naturalSortFiles,
  assignMediaInOrder,
  swapSectionAssignments,
  calculateMismatchSummary,
  validateExportReadiness,
  checkDuplicateSha256,
} from './media-store.ts';
import type { MediaItem, SectionAssignment } from './media-store.ts';

test('naturalSortFiles sorts filenames naturally with mixed case and leading zeros', () => {
  const filenames = [
    'scene_10.png',
    'scene_1.png',
    'scene_2.png',
    'Scene_03.png',
    'scene_20.png',
    'scene_02_b.png',
  ];

  const sorted = naturalSortFiles(filenames);
  assert.deepEqual(sorted, [
    'scene_1.png',
    'scene_02_b.png',
    'scene_2.png',
    'Scene_03.png',
    'scene_10.png',
    'scene_20.png',
  ]);
});

test('checkDuplicateSha256 detects duplicate media content by hash', () => {
  const existing: MediaItem[] = [
    {
      id: 'm1',
      filename: 'hero.png',
      bytes: 1024,
      sha256: 'a1b2c3d4',
      mime: 'image/png',
      aspectRatio: '16:9',
      storageKey: 'u/1/p/1/media/m1/original.png',
      thumbUrl: '/thumb1.webp',
    },
  ];

  assert.equal(checkDuplicateSha256('a1b2c3d4', existing), true);
  assert.equal(checkDuplicateSha256('different_hash', existing), false);
});

test('assignMediaInOrder assigns image k to section k and leaves excess empty or in library', () => {
  const sections = [
    { id: 'sec-1', position: 0, text: 'First paragraph' },
    { id: 'sec-2', position: 1, text: 'Second paragraph' },
    { id: 'sec-3', position: 2, text: 'Third paragraph' },
  ];

  const media: MediaItem[] = [
    {
      id: 'm1',
      filename: 'img1.png',
      bytes: 100,
      sha256: 'hash1',
      mime: 'image/png',
      aspectRatio: '16:9',
      storageKey: 'key1',
      thumbUrl: 'thumb1',
    },
    {
      id: 'm2',
      filename: 'img2.png',
      bytes: 100,
      sha256: 'hash2',
      mime: 'image/png',
      aspectRatio: '16:9',
      storageKey: 'key2',
      thumbUrl: 'thumb2',
    },
  ];

  const assignments = assignMediaInOrder(sections, media);
  assert.equal(assignments.length, 3);
  assert.equal(assignments[0].mediaId, 'm1');
  assert.equal(assignments[1].mediaId, 'm2');
  assert.equal(assignments[2].mediaId, null); // Empty section (MED-06, ASN-01)
});

test('calculateMismatchSummary generates exact PRD mismatch string', () => {
  const summary = calculateMismatchSummary(64, 61);
  assert.equal(summary.text, '64 sections, 61 images, 3 sections have no image');
  assert.equal(summary.hasEmptySections, true);
  assert.equal(summary.emptyCount, 3);

  const perfect = calculateMismatchSummary(10, 10);
  assert.equal(perfect.text, '10 sections, 10 images');
  assert.equal(perfect.hasEmptySections, false);
  assert.equal(perfect.emptyCount, 0);

  const excess = calculateMismatchSummary(5, 8);
  assert.equal(excess.text, '5 sections, 8 images, 3 extra images in library');
  assert.equal(excess.hasEmptySections, false);
});

test('swapSectionAssignments swaps assigned media between two sections', () => {
  const assignments: SectionAssignment[] = [
    { sectionId: 'sec-1', mediaId: 'm1', fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
    { sectionId: 'sec-2', mediaId: 'm2', fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
    { sectionId: 'sec-3', mediaId: null, fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
  ];

  const swapped = swapSectionAssignments(assignments, 'sec-1', 'sec-2');
  assert.equal(swapped[0].mediaId, 'm2');
  assert.equal(swapped[1].mediaId, 'm1');

  const swappedWithEmpty = swapSectionAssignments(swapped, 'sec-1', 'sec-3');
  assert.equal(swappedWithEmpty[0].mediaId, null);
  assert.equal(swappedWithEmpty[2].mediaId, 'm2');
});

test('validateExportReadiness blocks export and returns empty section IDs if any empty', () => {
  const assignments: SectionAssignment[] = [
    { sectionId: 'sec-1', mediaId: 'm1', fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
    { sectionId: 'sec-2', mediaId: null, fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
  ];

  const readiness = validateExportReadiness(assignments);
  assert.equal(readiness.canExport, false);
  assert.deepEqual(readiness.emptySectionIds, ['sec-2']);

  const completeAssignments: SectionAssignment[] = [
    { sectionId: 'sec-1', mediaId: 'm1', fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
    { sectionId: 'sec-2', mediaId: 'm2', fitMode: 'fill', focalX: 0.5, focalY: 0.5 },
  ];

  const completeReadiness = validateExportReadiness(completeAssignments);
  assert.equal(completeReadiness.canExport, true);
  assert.deepEqual(completeReadiness.emptySectionIds, []);
});
