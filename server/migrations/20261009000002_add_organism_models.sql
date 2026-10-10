CREATE TABLE IF NOT EXISTS organism_models (
    id UUID PRIMARY KEY,
    owner_user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    composition JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (char_length(name) BETWEEN 1 AND 128)
);

CREATE INDEX IF NOT EXISTS organism_models_owner_idx
    ON organism_models (owner_user_id, created_at DESC);
