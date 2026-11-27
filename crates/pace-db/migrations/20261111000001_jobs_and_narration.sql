-- Migration 20261111000001: Jobs Queue, Narrations, Transcripts, Boundaries, and Subtitles (TDD 5.1, 8.1, 10, 11).

-- 1. Jobs Queue table (JOB-01..03, TDD 8.1, 8.2)
CREATE TABLE IF NOT EXISTS jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    queue TEXT NOT NULL CHECK (queue IN ('audio', 'render')),
    type TEXT NOT NULL,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    priority INT NOT NULL DEFAULT 0,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    result JSONB NOT NULL DEFAULT '{}'::jsonb,
    progress INT NOT NULL DEFAULT 0,
    stage TEXT NOT NULL DEFAULT 'init',
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 3,
    error_code TEXT,
    error_message TEXT,
    locked_by TEXT,
    heartbeat_at TIMESTAMPTZ,
    run_after TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_jobs_queue_claim ON jobs(queue, status, run_after) WHERE status IN ('queued', 'running');
CREATE INDEX IF NOT EXISTS idx_jobs_project ON jobs(project_id, created_at DESC);

CREATE TRIGGER jobs_updated_at
    BEFORE UPDATE ON jobs
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 2. Narrations table (NAR-01..06, TDD 5.1)
CREATE TABLE IF NOT EXISTS narrations (
    project_id UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    media_id UUID NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    duration_ms INT NOT NULL,
    loudness_lufs REAL,
    true_peak_db REAL,
    peaks_key TEXT,
    silences JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER narrations_updated_at
    BEFORE UPDATE ON narrations
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 3. Transcripts table (ALN-01, TDD 5.1, 10.3)
CREATE TABLE IF NOT EXISTS transcripts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    narration_media_id UUID NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    engine TEXT NOT NULL,
    model TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',
    words JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_transcripts_project ON transcripts(project_id);

CREATE TRIGGER transcripts_updated_at
    BEFORE UPDATE ON transcripts
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 4. Boundaries table (ALN-03..07, TDD 5.1, 11)
CREATE TABLE IF NOT EXISTS boundaries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    index INT NOT NULL,
    time_ms INT NOT NULL,
    source TEXT NOT NULL DEFAULT 'auto' CHECK (source IN ('auto', 'manual')),
    confidence REAL NOT NULL DEFAULT 1.0,
    flags TEXT[] NOT NULL DEFAULT '{}',
    transition JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_boundaries_project_index UNIQUE (project_id, index)
);

CREATE INDEX IF NOT EXISTS idx_boundaries_project ON boundaries(project_id, index ASC);

CREATE TRIGGER boundaries_updated_at
    BEFORE UPDATE ON boundaries
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- 5. Subtitle Cues table (SUB-01..04, TDD 5.1, 11.5)
CREATE TABLE IF NOT EXISTS subtitle_cues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    position INT NOT NULL,
    start_ms INT NOT NULL,
    end_ms INT NOT NULL,
    text TEXT NOT NULL,
    edited BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_subtitles_project_position UNIQUE (project_id, position)
);

CREATE INDEX IF NOT EXISTS idx_subtitle_cues_project ON subtitle_cues(project_id, position ASC);

CREATE TRIGGER subtitle_cues_updated_at
    BEFORE UPDATE ON subtitle_cues
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

