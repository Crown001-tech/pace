# Milestone 4 Learning Notes: Media Upload & Assignment

1. Pre-signed S3 URLs allow browsers to stream media directly to object storage with fine-grained time-to-live and size boundaries, completely eliminating API server bandwidth bottlenecks.
2. Generating 320px WebP thumbnails and SHA-256 hashes inside a dedicated Web Worker prevents heavy image processing and hashing operations from freezing user interface interactions.
3. Natural alphanumeric sorting via `Intl.Collator` preserves human file naming order (`image1`, `image2`, `image10`), while atomic order mapping ($k \to k$) guarantees instant scene synchronization.

