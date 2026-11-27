-- Migration 20261121000001: Exports and Cached Render Segments (M8, EXP-01..11).

CREATE TABLE IF NOT EXISTS exports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    job_id UUID REFERENCES jobs(id) ON DELETE SET NULL,
    spec_hash TEXT NOT NULL,
    resolution TEXT NOT NULL CHECK (resolution IN ('360p', '480p', '720p', '1080p')),
    width INT NOT NULL,
    height INT NOT NULL,
    fps INT NOT NULL,
    duration_ms INT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    storage_key TEXT,
    size_bytes BIGINT,
    validation JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_exports_project ON exports(project_id, created_at DESC);

CREATE TRIGGER exports_updated_at
    BEFORE UPDATE ON exports
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TABLE IF NOT EXISTS render_segments (
    cache_key TEXT PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    storage_key TEXT NOT NULL,
    frame_count INT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_render_segments_project ON render_segments(project_id);
