# Milestone 5 Learning Notes: Narration and Audio Sync

1. Needleman-Wunsch sequence alignment combined with 3-word unique fixed anchors enables sub-second script-to-audio matching even across long documentary narrations with pauses or retakes.
2. Snapping scene cut boundaries into natural speech pauses detected by FFmpeg prevents abrupt mid-word audio interruptions and produces professional documentary pacing.
3. PostgreSQL job queues using FOR UPDATE SKIP LOCKED provide robust, distributed task claiming across Rust render daemons and Python audio workers without requiring external broker infrastructure.

