/**
 * PACE Shared Data Contracts
 *
 * Generated automatically by `cargo xtask gen`.
 * DO NOT EDIT MANUALLY. Source of truth is Rust crates (pace-api, pace-db, pace-engine).
 */

export type AspectRatio = "16:9" | "9:16";

export type ProjectStatus = "draft" | "processing" | "ready" | "error" | "deleted";

export type UserRole = "creator" | "admin";

export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  detail: string;
  code: string;
  instance?: string | null;
}

export interface UserProfile {
  id: string;
  email: string;
  name: string;
  role: string;
  must_change_password: boolean;
  storage_quota_bytes: number;
  storage_used_bytes: number;
  csrf_token?: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  user: UserProfile;
  csrf_token: string;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

export interface MessageResponse {
  status: string;
  message: string;
}

export interface CreateUserRequest {
  email: string;
  name: string;
  temporary_password: string;
  role?: string | null;
  storage_quota_bytes?: number | null;
}

export interface UpdateUserStatusRequest {
  is_active: boolean;
}

export interface ResetPasswordRequest {
  temporary_password: string;
}

export interface CreateProjectRequest {
  name: string;
  aspect_ratio: AspectRatio | string;
  fps?: number | null;
  language?: string | null;
  effect_seed?: number | null;
  settings?: Record<string, unknown> | null;
}

export interface UpdateProjectRequest {
  name?: string | null;
  aspect_ratio?: AspectRatio | string | null;
  settings?: Record<string, unknown> | null;
  status?: ProjectStatus | string | null;
  version?: number | null;
}

export interface ProjectResponse {
  id: string;
  owner_id: string;
  name: string;
  aspect_ratio: AspectRatio | string;
  fps: number;
  language: string;
  effect_seed: number;
  settings: Record<string, unknown>;
  version: number;
  status: ProjectStatus | string;
  created_at: string;
  updated_at: string;
}

export interface HealthResponse {
  status: string;
  version: string;
}

export interface ReadinessResponse {
  status: string;
  database: string;
}

export interface UpsertSectionItem {
  id?: string | null;
  text: string;
}

export interface SaveScriptRequest {
  raw_text?: string | null;
  sections: UpsertSectionItem[];
}

export interface SectionDto {
  id: string;
  position: number;
  text: string;
  needs_resync: boolean;
  created_at: string;
  updated_at: string;
}

export interface ScriptResponse {
  project_id: string;
  raw_text: string;
  sections: SectionDto[];
  updated_at?: string | null;
}

export interface PresignUploadRequest {
  filename: string;
  content_type: string;
  bytes: number;
}

export interface PresignUploadResponse {
  media_id: string;
  storage_key: string;
  upload_url: string;
  preview_key: string;
  preview_upload_url: string;
  thumb_key: string;
  thumb_upload_url: string;
}

export interface RegisterMediaRequest {
  id?: string | null;
  storage_key: string;
  original_filename: string;
  mime: string;
  bytes: number;
  sha256: string;
  width?: number | null;
  height?: number | null;
  thumb_key?: string | null;
  preview_key?: string | null;
}

export interface MediaResponse {
  id: string;
  project_id: string;
  filename: string;
  mime: string;
  bytes: number;
  sha256: string;
  width?: number | null;
  height?: number | null;
  download_url: string;
  thumb_url?: string | null;
  preview_url?: string | null;
  source?: string;
  prompt?: string | null;
}

export interface StockMediaItem {
  id: string;
  provider: string;
  title: string;
  thumb_url: string;
  image_url: string;
  width: number;
  height: number;
  author: string;
}

export interface StockSearchResponse {
  provider: string;
  query: string;
  items: StockMediaItem[];
}

export interface ImportStockMediaRequest {
  image_url: string;
  title?: string | null;
  provider?: string | null;
  section_id?: string | null;
}

export interface AutoMatchStockRequest {
  section_ids?: string[] | null;
}

export interface AutoMatchedSceneItem {
  section_id: string;
  query: string;
  media: MediaResponse;
}

export interface AutoMatchStockResponse {
  matched_count: number;
  skipped_count: number;
  matched: AutoMatchedSceneItem[];
}

export interface GenerateAiMediaRequest {
  prompt: string;
  section_id?: string | null;
  style?: string | null;
}

export interface AssignSectionRequest {
  media_id: string;
  fit_mode?: "fill" | "fit_blur" | string | null;
  focal_x?: number | null;
  focal_y?: number | null;
}

export interface SwapSectionsRequest {
  section_id_a: string;
  section_id_b: string;
}

export type MotionPreset =
  | "still"
  | "zoom_in"
  | "zoom_out"
  | "punch_zoom_in"
  | "punch_zoom_out"
  | "pan_left"
  | "pan_right"
  | "pan_up"
  | "pan_down"
  | "zoom_in_pan_left"
  | "zoom_in_pan_right"
  | "zoom_in_pan_up"
  | "zoom_in_pan_down"
  | "zoom_out_pan_left"
  | "zoom_out_pan_right"
  | "zoom_out_pan_up"
  | "zoom_out_pan_down"
  | "ken_burns_diagonal_in"
  | "ken_burns_diagonal_out"
  | "handheld_drift"
  | "zoom_snap"
  | "pendulum"
  | "wobble"
  | "fall_left"
  | "fall_right";

export type SceneVisualEffect =
  | "none"
  | "handheld"
  | "camera_shake"
  | "zoom_lens";

export type MotionIntensity = "subtle" | "normal" | "strong";

export type MotionEasing =
  | "steady"
  | "ease_in_out"
  | "punch_out"
  | "ramp_in"
  | "dynamic_curve";

export type FitMode = "fill" | "fit_blur";

export type TransitionKind =
  | "cut"
  | "crossfade"
  | "fade_black"
  | "shutter_flash"
  | "zoom_through"
  | "whip_pan_left"
  | "whip_pan_right"
  | "slide_left"
  | "slide_right"
  | "slide_up"
  | "slide_down";

export type StickerKind =
  | "subscribe"
  | "like"
  | "comment"
  | "bell"
  | "follow";

export type StickerAnimation = "none" | "pop_in" | "bounce" | "float";

export interface StickerPlacementSpec {
  id: string;
  kind: StickerKind | string;
  start_ms: number;
  end_ms: number;
  x: number;
  y: number;
  scale: number;
  animation: StickerAnimation | string;
}

export interface BoundaryTransitionDto {
  kind: TransitionKind | string;
  duration_ms: number;
}

export interface SceneEffectDto {
  manual_preset?: MotionPreset | string | null;
  intensity: MotionIntensity | string;
  easing: MotionEasing | string;
  manual_shake?: boolean | null;
  visual_effect?: SceneVisualEffect | string;
}

export interface UpdateSceneEffectRequest {
  manual_preset?: MotionPreset | string | null;
  clear_manual_preset?: boolean | null;
  intensity?: MotionIntensity | string | null;
  easing?: MotionEasing | string | null;
  manual_shake?: boolean | null;
  clear_manual_shake?: boolean | null;
  visual_effect?: SceneVisualEffect | string | null;
  fit_mode?: FitMode | string | null;
  focal_x?: number | null;
  focal_y?: number | null;
  reset_to_auto?: boolean | null;
}

export interface AssignmentResponse {
  id: string;
  section_id: string;
  media_id: string;
  fit_mode: FitMode | string;
  focal_x: number;
  focal_y: number;
  effect: SceneEffectDto;
  effect_overridden: boolean;
}

export interface JobStatusDto {
  id: string;
  queue: "audio" | "render" | string;
  job_type: string;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled" | string;
  progress: number;
  stage?: string | null;
  attempts: number;
  max_attempts: number;
  error_code?: string | null;
  error_message?: string | null;
  result?: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
}

export interface PresignNarrationRequest {
  filename: string;
  content_type: string;
  bytes: number;
}

export interface PresignNarrationResponse {
  media_id: string;
  storage_key: string;
  upload_url: string;
}

export interface RegisterNarrationRequest {
  media_id?: string | null;
  storage_key: string;
  original_filename: string;
  mime: string;
  bytes: number;
  sha256: string;
}

export interface RegisterNarrationResponse {
  media_id: string;
  job: JobStatusDto;
}

export interface RealignNarrationRequest {
  keep_manual_boundaries?: boolean;
  force_local_transcription?: boolean;
}

export interface SilenceGapDto {
  start_ms: number;
  end_ms: number;
  duration_ms: number;
}

export interface TranscriptWordDto {
  word: string;
  start_ms: number;
  end_ms: number;
  probability: number;
}

export interface TranscriptSummaryDto {
  engine: string;
  model: string;
  language: string;
  word_count: number;
  words: TranscriptWordDto[];
}

export interface BoundaryDto {
  id: string;
  index: number;
  time_ms: number;
  source: "auto" | "manual" | string;
  confidence?: number | null;
  flags: string[];
  transition: Record<string, unknown>;
}

export interface SubtitleCueDto {
  id: string;
  position: number;
  start_ms: number;
  end_ms: number;
  text: string;
  edited: boolean;
}

export interface NarrationStateResponse {
  project_id: string;
  media_id?: string | null;
  filename?: string | null;
  bytes?: number | null;
  audio_url?: string | null;
  duration_ms?: number | null;
  loudness_lufs?: number | null;
  true_peak_db?: number | null;
  has_peaks: boolean;
  silences: SilenceGapDto[];
  transcript?: TranscriptSummaryDto | null;
  boundaries: BoundaryDto[];
  subtitle_cues: SubtitleCueDto[];
  active_job?: JobStatusDto | null;
}

export interface NarrationPeaksResponse {
  media_id: string;
  duration_ms: number;
  peaks: [number, number][];
}

export interface UpdateBoundaryRequest {
  time_ms: number;
  transition?: Record<string, unknown> | null;
}

export interface UpdateSubtitleCueRequest {
  text: string;
  start_ms?: number | null;
  end_ms?: number | null;
}

export type SubtitleFont = "sans" | "mono";

export type SubtitlePosition = "bottom" | "center" | "top";

export interface SubtitleStyleDto {
  enabled: boolean;
  font: SubtitleFont | string;
  size_pct: number;
  position: SubtitlePosition | string;
  text_color: string;
  outline_width_px: number;
  bg_opacity_pct: number;
}

export interface SubtitleCueDetailDto {
  id: string;
  position: number;
  start_ms: number;
  end_ms: number;
  duration_ms: number;
  text: string;
  lines: string[];
  max_line_chars: number;
  char_limit: number;
  violations: string[];
  edited: boolean;
}

export interface SubtitlesStateResponse {
  project_id: string;
  aspect_ratio: string;
  max_chars_per_line: number;
  style: SubtitleStyleDto;
  cues: SubtitleCueDetailDto[];
}

export interface SplitSubtitleCueRequest {
  split_word_index?: number | null;
}

export interface MergeSubtitleCuesRequest {
  first_cue_id: string;
  second_cue_id: string;
}

export interface MusicSettingsDto {
  volume_db: number;
  ducking_enabled: boolean;
  duck_db: number;
  loop_enabled: boolean;
  crossfade_ms: number;
  fade_in_ms: number;
  fade_out_ms: number;
}

export interface MusicStateResponse {
  project_id: string;
  media_id?: string | null;
  filename?: string | null;
  bytes?: number | null;
  audio_url?: string | null;
  settings: MusicSettingsDto;
}

export interface PresignMusicRequest {
  filename: string;
  content_type: string;
  bytes: number;
}

export interface PresignMusicResponse {
  media_id: string;
  storage_key: string;
  upload_url: string;
}

export interface RegisterMusicRequest {
  media_id?: string | null;
  storage_key: string;
  original_filename: string;
  mime: string;
  bytes: number;
  sha256: string;
}

export interface UpdateMusicSettingsRequest {
  volume_db?: number | null;
  ducking_enabled?: boolean | null;
  duck_db?: number | null;
  loop_enabled?: boolean | null;
  crossfade_ms?: number | null;
  fade_in_ms?: number | null;
  fade_out_ms?: number | null;
}

export interface TimelineSceneDto {
  index: number;
  section_id: string;
  text: string;
  start_ms: number;
  end_ms: number;
  duration_ms: number;
  media_id?: string | null;
  filename?: string | null;
  thumb_url?: string | null;
  preview_url?: string | null;
  fit_mode: string;
  focal_x: number;
  focal_y: number;
  effect: SceneEffectDto;
  effect_overridden: boolean;
  resolved_preset: MotionPreset | string;
  resolved_has_shake: boolean;
  flags: string[];
  confidence?: number | null;
  source: string;
}

export interface TimelineStateResponse {
  project_id: string;
  aspect_ratio: string;
  fps: number;
  effect_seed: number;
  duration_ms: number;
  audio_url?: string | null;
  audio_filename?: string | null;
  has_peaks: boolean;
  silences: SilenceGapDto[];
  boundaries: BoundaryDto[];
  flagged_boundary_indices: number[];
  scenes: TimelineSceneDto[];
  subtitle_cues: SubtitleCueDto[];
  subtitle_style: SubtitleStyleDto;
  music_url?: string | null;
  music_filename?: string | null;
  music_settings: MusicSettingsDto;
  stickers?: StickerPlacementSpec[];
  include_tilt_in_auto?: boolean;
}

export interface BatchBoundaryItem {
  index: number;
  time_ms: number;
  transition?: Record<string, unknown> | null;
}

export interface BatchUpdateBoundariesRequest {
  boundaries: BatchBoundaryItem[];
}

export interface RenderSpecAssetDto {
  scene_index: number;
  section_id: string;
  media_id?: string | null;
  filename?: string | null;
  preview_url?: string | null;
  thumb_url?: string | null;
  width: number;
  height: number;
}

export interface SizeFDto {
  w: number;
  h: number;
}

export interface FocalPointDto {
  x: number;
  y: number;
}

export interface CamStateDto {
  cx: number;
  cy: number;
  zoom: number;
}

export interface ShakeDto {
  amplitude: number;
  frequency_hz: number;
  phase: number;
}

export interface MotionDto {
  preset: MotionPreset | string;
  from: CamStateDto;
  to: CamStateDto;
  easing: MotionEasing | string;
  intensity: MotionIntensity | string;
  shake?: ShakeDto | null;
  duration_ms?: number;
}

export interface CompiledSceneDto {
  scene_index: number;
  section_id: string;
  media_id?: string | null;
  image_size: SizeFDto;
  start_frame: number;
  end_frame: number;
  frame_count: number;
  transition_shortened: boolean;
  fit_mode: FitMode | string;
  focal: FocalPointDto;
  preset: MotionPreset | string;
  has_shake: boolean;
  motion: MotionDto;
  effect_overridden: boolean;
}

export interface TransitionWindowDto {
  boundary_index: number;
  kind: TransitionKind | string;
  center_frame: number;
  start_frame: number;
  end_frame: number;
  duration_frames: number;
}

export interface RenderSpecDto {
  spec_hash: string;
  fps: number;
  frame_size: SizeFDto;
  narration_ms: number;
  effect_seed: number;
  total_frames: number;
  scenes: CompiledSceneDto[];
  transitions: TransitionWindowDto[];
}

export interface RenderSpecResponse {
  project_id: string;
  spec_hash: string;
  effect_seed: number;
  fps: number;
  frame_width: number;
  frame_height: number;
  total_frames: number;
  duration_ms: number;
  audio_url?: string | null;
  scene_assets: RenderSpecAssetDto[];
  spec: RenderSpecDto;
}

export interface RerollEffectsResponse {
  project_id: string;
  effect_seed: number;
  spec_hash: string;
}

export type ExportResolution = "360p" | "480p" | "720p" | "1080p";

export interface StartExportRequest {
  resolution: ExportResolution;
}

export interface ExportValidationDto {
  width?: number;
  height?: number;
  video_codec?: string;
  audio_codec?: string;
  duration_ms?: number;
  expected_duration_ms?: number;
  size_bytes?: number;
  black_intervals?: [number, number][];
  silence_intervals?: [number, number][];
}

export interface ExportResponse {
  id: string;
  project_id: string;
  job_id?: string | null;
  spec_hash: string;
  resolution: ExportResolution | string;
  width: number;
  height: number;
  fps: number;
  duration_ms: number;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled" | string;
  progress: number;
  stage?: string | null;
  size_bytes?: number | null;
  download_url?: string | null;
  download_filename?: string | null;
  validation: ExportValidationDto;
  error_code?: string | null;
  error_message?: string | null;
  created_at: string;
  updated_at: string;
  finished_at?: string | null;
}
