/**
 * PACE Media Web Worker
 * Performs off-main-thread image processing:
 * 1. SHA-256 cryptographic hashing via Web Crypto API (SubtleCrypto)
 * 2. Original dimensions and aspect ratio extraction
 * 3. Fast canvas downscaling to 320px WebP thumbnail and 1280px preview
 */

self.onmessage = async function (e) {
  const { id, file, projectAspectRatio } = e.data;

  try {
    // 1. Compute SHA-256 hash
    const arrayBuffer = await file.arrayBuffer();
    const hashBuffer = await crypto.subtle.digest('SHA-256', arrayBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const sha256 = hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');

    // 2. Decode image bitmap
    const bitmap = await createImageBitmap(file);
    const width = bitmap.width;
    const height = bitmap.height;
    const isPortrait = height > width;
    const imageAspectRatio = isPortrait ? '9:16' : '16:9';
    const isMismatch = projectAspectRatio ? imageAspectRatio !== projectAspectRatio : false;

    // 3. Helper to resize via OffscreenCanvas
    async function createResizedBlob(targetLongSide, quality) {
      if (typeof OffscreenCanvas === 'undefined') {
        return null;
      }
      let targetW, targetH;
      if (width >= height) {
        targetW = Math.min(width, targetLongSide);
        targetH = Math.round((height / width) * targetW);
      } else {
        targetH = Math.min(height, targetLongSide);
        targetW = Math.round((width / height) * targetH);
      }

      const canvas = new OffscreenCanvas(targetW, targetH);
      const ctx = canvas.getContext('2d');
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(bitmap, 0, 0, targetW, targetH);

      return await canvas.convertToBlob({ type: 'image/webp', quality });
    }

    // 4. Generate 320px thumb and 1280px preview
    const thumbBlob = await createResizedBlob(320, 0.8);
    const previewBlob = await createResizedBlob(1280, 0.85);

    bitmap.close();

    self.postMessage({
      id,
      success: true,
      sha256,
      width,
      height,
      imageAspectRatio,
      isMismatch,
      thumbBlob,
      previewBlob,
    });
  } catch (err) {
    self.postMessage({
      id,
      success: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};

