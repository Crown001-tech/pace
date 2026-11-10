use std::fs;
use std::path::Path;
use utoipa::OpenApi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("gen");

    match command {
        "gen" => {
            println!("==> Running cargo xtask gen...");
            generate_openapi()?;
            generate_typescript_types()?;
            generate_python_contracts()?;
            println!("==> Code generation complete!");
        }
        "help" | "--help" | "-h" => {
            println!("PACE xtask runner");
            println!();
            println!("Available commands:");
            println!("  gen     Generate openapi.json, TypeScript contracts, and Python models");
        }
        unknown => {
            eprintln!("Unknown command: '{unknown}'. Run `cargo xtask help` for usage.");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn generate_openapi() -> Result<(), Box<dyn std::error::Error>> {
    let openapi = pace_api::openapi::ApiDoc::openapi();
    let json_content = openapi.to_pretty_json()?;

    let target_dir = Path::new("contracts");
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let target_path = target_dir.join("openapi.json");
    fs::write(&target_path, json_content)?;
    println!(
        "  [OK] Exported OpenAPI 3.1 specification to {}",
        target_path.display()
    );
    Ok(())
}

fn generate_typescript_types() -> Result<(), Box<dyn std::error::Error>> {
    let ts_content = r#"/**
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
"#;

    let target_dir = Path::new("packages/shared-types/src");
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let target_path = target_dir.join("index.ts");
    fs::write(&target_path, ts_content)?;
    println!(
        "  [OK] Exported TypeScript contracts to {}",
        target_path.display()
    );
    Ok(())
}

fn generate_python_contracts() -> Result<(), Box<dyn std::error::Error>> {
    let py_content = r#""""
PACE Audio Worker Contracts

Generated automatically by `cargo xtask gen`.
DO NOT EDIT MANUALLY. Source of truth is Rust crates (pace-api, pace-db, pace-engine).
"""

from dataclasses import dataclass
from typing import Optional, Dict, Any, Literal
from datetime import datetime

AspectRatio = Literal["16:9", "9:16"]
ProjectStatus = Literal["draft", "processing", "ready", "error", "deleted"]

@dataclass
class ProjectContract:
    id: str
    owner_id: str
    name: str
    aspect_ratio: AspectRatio
    fps: int
    language: str
    effect_seed: int
    settings: Dict[str, Any]
    version: int
    status: ProjectStatus
    created_at: datetime
    updated_at: datetime
"#;

    let target_dir = Path::new("services/audio/src/pace_audio");
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let target_path = target_dir.join("contracts.py");
    fs::write(&target_path, py_content)?;
    println!("  [OK] Exported Python models to {}", target_path.display());
    Ok(())
}
