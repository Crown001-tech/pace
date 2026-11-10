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
