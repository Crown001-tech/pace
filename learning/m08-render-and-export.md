# Milestone 8 Learning Notes: Render and Export

1. Streaming raw RGBA buffers straight into `ffmpeg` stdin with uniform libx264 flags (`open-gop=0`, `-video_track_timescale 15360`) allows instantaneous `-c:v copy` concat concatenation without disk-heavy frame dumps.
2. Content-addressing segments by image ID, framing, relative local frames, and motion lets edits to Scene `k` invalidate at most 3 segments (`Body(k)` and touching transitions) and lets retried exports resume immediately from object-storage cache.
3. Probing the final MP4 with `ffprobe` (`h264`, `yuv420p`, `aac`, duration parity) and `blackdetect` catches truncated outputs, silent failures, and unexpected gaps before surfacing a signed download URL.
