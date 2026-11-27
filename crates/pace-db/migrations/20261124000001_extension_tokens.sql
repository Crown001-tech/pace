-- Migration 20261124000001: Chrome Extension one-time pairing codes and scoped tokens (EXT-01..05).

CREATE TABLE IF NOT EXISTS extension_pairing_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ext_pairing_user ON extension_pairing_codes(user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS extension_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_name TEXT NOT NULL DEFAULT 'Chrome Extension',
    token_hash TEXT NOT NULL UNIQUE,
    last_project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ext_tokens_user ON extension_tokens(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ext_tokens_hash ON extension_tokens(token_hash) WHERE revoked_at IS NULL;
