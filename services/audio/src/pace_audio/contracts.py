"""
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

@dataclass
class AudioJobPayloadContract:
    project_id: str
    user_id: str
    media_id: str
    storage_key: str
    opus_key: str
    asr_key: str
    peaks_key: str
    mime: str
    original_filename: str
    keep_manual_boundaries: bool = False
    force_local_transcription: bool = False
