import { useState, useCallback, useRef, useEffect } from 'react';
import { api, ApiError } from '@/lib/api-client';
import { useMediaStore, MediaItem, checkDuplicateSha256 } from './media-store';

interface UploadOptions {
  projectId: string;
  projectAspectRatio: '16:9' | '9:16';
  onComplete?: () => void;
}

function resolveImageMime(file: File): string | null {
  const t = file.type.trim().toLowerCase();
  if (t === 'image/jpeg' || t === 'image/jpg') return 'image/jpeg';
  if (t === 'image/png') return 'image/png';
  if (t === 'image/webp') return 'image/webp';
  const name = file.name.toLowerCase();
  if (name.endsWith('.jpg') || name.endsWith('.jpeg')) return 'image/jpeg';
  if (name.endsWith('.png')) return 'image/png';
  if (name.endsWith('.webp')) return 'image/webp';
  return null;
}

async function computeSha256Hex(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const hashBuffer = await crypto.subtle.digest('SHA-256', buf);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

async function putObjectWithRetry(
  url: string,
  body: Blob,
  contentType: string,
  maxAttempts = 3
): Promise<void> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      const resp = await fetch(url, {
        method: 'PUT',
        headers: {
          'Content-Type': contentType,
        },
        body,
      });
      if (!resp.ok) {
        throw new Error(`Storage upload failed with HTTP ${resp.status}`);
      }
      return;
    } catch (err) {
      lastError = err;
      if (attempt < maxAttempts) {
        await new Promise((r) => setTimeout(r, 300 * attempt));
      }
    }
  }
  throw lastError instanceof Error ? lastError : new Error('Storage upload failed');
}

export function useMediaUpload({ projectId, projectAspectRatio, onComplete }: UploadOptions) {
  const [isProcessing, setIsProcessing] = useState(false);
  const workerRef = useRef<Worker | null>(null);
  const {
    addMediaItem,
    addDuplicateWarning,
    setUploadProgress,
    clearUploadProgress,
  } = useMediaStore();

  useEffect(() => {
    if (typeof window !== 'undefined' && window.Worker) {
      try {
        workerRef.current = new Worker('/media-worker.js');
      } catch (err) {
        console.warn('Web Worker initialization failed, falling back to main thread', err);
      }
    }
    return () => {
      workerRef.current?.terminate();
    };
  }, []);

  const processFile = useCallback(
    async (file: File): Promise<void> => {
      const mime = resolveImageMime(file);
      if (!mime) return;

      const filename = file.name;
      const reqId = `${filename}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
      setUploadProgress(filename, { status: 'hashing', progress: 10 });

      // 1. Process image with Web Worker (SHA-256 + dimensions + 1280px preview & 320px thumb WebP)
      let workerResult: {
        sha256: string;
        width: number;
        height: number;
        imageAspectRatio: '16:9' | '9:16';
        isMismatch: boolean;
        thumbBlob: Blob | null;
        previewBlob: Blob | null;
      };

      try {
        workerResult = await new Promise((resolve, reject) => {
          if (!workerRef.current) {
            computeSha256Hex(file)
              .then((sha256) =>
                resolve({
                  sha256,
                  width: 1920,
                  height: 1080,
                  imageAspectRatio: '16:9',
                  isMismatch: false,
                  thumbBlob: null,
                  previewBlob: null,
                })
              )
              .catch(reject);
            return;
          }

          const handleMessage = (e: MessageEvent) => {
            if (e.data.id === reqId) {
              workerRef.current?.removeEventListener('message', handleMessage);
              if (e.data.success) {
                resolve(e.data);
              } else {
                reject(new Error(e.data.error || 'Worker processing failed'));
              }
            }
          };

          workerRef.current.addEventListener('message', handleMessage);
          workerRef.current.postMessage({
            id: reqId,
            file,
            projectAspectRatio,
          });
        });
      } catch (err) {
        setUploadProgress(filename, {
          status: 'error',
          errorMessage: err instanceof Error ? err.message : 'Failed reading image',
        });
        return;
      }

      // 2. Check client-side duplicate before presigning (MED-04)
      const currentMedia = useMediaStore.getState().media;
      if (checkDuplicateSha256(workerResult.sha256, currentMedia)) {
        addDuplicateWarning(filename);
        clearUploadProgress(filename);
        return;
      }

      setUploadProgress(filename, { status: 'uploading', progress: 35 });

      try {
        // 3. Request presigned PUT URLs from PACE API
        const presigned = await api.uploads.presign(projectId, {
          filename,
          content_type: mime,
          bytes: file.size,
        });

        setUploadProgress(filename, { status: 'uploading', progress: 55 });

        // 4. Direct browser-to-MinIO/S3 PUT with 3x retry (MED-02)
        const uploadTasks: Promise<void>[] = [
          putObjectWithRetry(presigned.upload_url, file, mime),
        ];

        let previewUploaded = false;
        let thumbUploaded = false;

        if (workerResult.previewBlob && presigned.preview_upload_url) {
          uploadTasks.push(
            putObjectWithRetry(
              presigned.preview_upload_url,
              workerResult.previewBlob,
              'image/webp'
            ).then(() => {
              previewUploaded = true;
            })
          );
        }

        if (workerResult.thumbBlob && presigned.thumb_upload_url) {
          uploadTasks.push(
            putObjectWithRetry(
              presigned.thumb_upload_url,
              workerResult.thumbBlob,
              'image/webp'
            ).then(() => {
              thumbUploaded = true;
            })
          );
        }

        await Promise.all(uploadTasks);
        setUploadProgress(filename, { status: 'uploading', progress: 85 });

        // 5. Confirm upload & persist metadata in PostgreSQL
        const registered = await api.media.register(projectId, {
          id: presigned.media_id,
          storage_key: presigned.storage_key,
          original_filename: filename,
          mime,
          bytes: file.size,
          sha256: workerResult.sha256,
          width: workerResult.width,
          height: workerResult.height,
          preview_key: previewUploaded ? presigned.preview_key : null,
          thumb_key: thumbUploaded ? presigned.thumb_key : null,
        });

        setUploadProgress(filename, { status: 'done', progress: 100 });

        const item: MediaItem = {
          id: registered.id,
          filename: registered.filename,
          bytes: registered.bytes,
          sha256: registered.sha256,
          mime: registered.mime,
          aspectRatio: workerResult.imageAspectRatio,
          storageKey: presigned.storage_key,
          thumbUrl: registered.thumb_url ?? registered.preview_url ?? registered.download_url,
          previewUrl: registered.preview_url ?? registered.download_url,
        };

        addMediaItem(item);

        setTimeout(() => {
          clearUploadProgress(filename);
        }, 1000);
      } catch (err) {
        if (err instanceof ApiError && err.status === 409) {
          addDuplicateWarning(filename);
          clearUploadProgress(filename);
          return;
        }
        setUploadProgress(filename, {
          status: 'error',
          errorMessage: err instanceof Error ? err.message : 'Upload failed',
        });
      }
    },
    [
      projectId,
      projectAspectRatio,
      addMediaItem,
      addDuplicateWarning,
      setUploadProgress,
      clearUploadProgress,
    ]
  );

  const handleUploadFiles = useCallback(
    async (fileList: FileList | File[]) => {
      const files = Array.from(fileList);
      if (files.length === 0) return;

      // Limit to 400 images (MED-01)
      const allowed = files.slice(0, 400);

      setIsProcessing(true);
      try {
        for (const file of allowed) {
          if (!resolveImageMime(file)) {
            continue;
          }
          // Max 20 MB per image (MED-01)
          if (file.size <= 0 || file.size > 20 * 1024 * 1024) {
            continue;
          }

          await processFile(file);
        }
      } finally {
        setIsProcessing(false);
        onComplete?.();
      }
    },
    [processFile, onComplete]
  );

  return {
    handleUploadFiles,
    isProcessing,
  };
}
