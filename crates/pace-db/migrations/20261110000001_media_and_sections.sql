-- Migration 20261110000001: Media, Sections, and Scene Assignments (`MED-01..08`, `ASN-01..05`, TDD 5.1).

-- 1. Scripts table (SCR-01)
CREATE TABLE IF NOT EXISTS scripts (
    project_id UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    raw_text TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER scripts_updated_at
    BEFORE UPDATE ON scripts
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 2. Sections table (SCR-02..07)
CREATE TABLE IF NOT EXISTS sections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    position INT NOT NULL,
    text TEXT NOT NULL DEFAULT '',
    needs_resync BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_sections_project_position UNIQUE (project_id, position) DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX IF NOT EXISTS idx_sections_project_position ON sections(project_id, position ASC);

CREATE TRIGGER sections_updated_at
    BEFORE UPDATE ON sections
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 3. Media Assets table (MED-01..08, TDD 5.1, 9.1)
CREATE TABLE IF NOT EXISTS media_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'image' CHECK (kind IN ('image', 'video', 'narration', 'music')),
    storage_key TEXT NOT NULL,
    preview_key TEXT,
    thumb_key TEXT,
    original_filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    bytes BIGINT NOT NULL,
    sha256 TEXT NOT NULL,
    width INT,
    height INT,
    duration_ms INT,
    source TEXT NOT NULL DEFAULT 'upload' CHECK (source IN ('upload', 'extension')),
    prompt TEXT,
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('uploading', 'ready', 'failed', 'missing')),
    probe JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_media_assets_project_sha256_kind UNIQUE (project_id, sha256, kind)
);

CREATE INDEX IF NOT EXISTS idx_media_assets_project ON media_assets(project_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_media_assets_sha256 ON media_assets(project_id, sha256);

CREATE TRIGGER media_assets_updated_at
    BEFORE UPDATE ON media_assets
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 4. Scene Media (Section Assignment) table (ASN-01..05, EFX-04)
CREATE TABLE IF NOT EXISTS scene_media (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    section_id UUID NOT NULL REFERENCES sections(id) ON DELETE CASCADE,
    media_id UUID NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    position INT NOT NULL DEFAULT 0,
    fit_mode TEXT NOT NULL DEFAULT 'fill' CHECK (fit_mode IN ('fill', 'fit_blur')),
    focal_x REAL NOT NULL DEFAULT 0.5,
    focal_y REAL NOT NULL DEFAULT 0.5,
    effect JSONB NOT NULL DEFAULT '{}'::jsonb,
    effect_overridden BOOLEAN NOT NULL DEFAULT false,
    trim JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_scene_media_section UNIQUE (section_id) -- single slot per section in v1 (ASN-04)
);

CREATE INDEX IF NOT EXISTS idx_scene_media_section ON scene_media(section_id);
CREATE INDEX IF NOT EXISTS idx_scene_media_media ON scene_media(media_id);

CREATE TRIGGER scene_media_updated_at
    BEFORE UPDATE ON scene_media
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

